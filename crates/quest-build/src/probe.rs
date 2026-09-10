use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cmake_file_api::{objects, query, reply};
use objects::codemodel_v2::Target;

use crate::{
    BridgeInputs, BuildError, HeaderContext, NativePackage, Result, absolute, explicit_prefix,
    invalid, io, parse_header_configuration, run, runtime_link_args, validate_compiler_environment,
    validate_target,
};

pub fn discover(
    work: &Path,
    host: &str,
    target: &str,
    inputs: Option<&BridgeInputs>,
) -> Result<NativePackage> {
    validate_target(host, target)?;
    validate_compiler_environment(target, None)?;
    let explicit = explicit_prefix()?;
    let search_prefixes = env::var_os("CMAKE_PREFIX_PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let setup = configure(
        work,
        host,
        target,
        explicit.as_deref(),
        inputs,
        &search_prefixes,
    )?;
    let reader = reply::Reader::from_build_dir(&setup.build_directory)
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    let probe = read_target(&reader, &setup.profile, "quest_link_query")?;
    let compiler = read_compiler(&reader, target)?;
    validate_compiler_environment(target, Some(&compiler.path))?;
    let (prefix, version, headers) = inspect_headers(&probe, explicit.as_deref())?;
    let mut link = inspect_link(&probe, &setup.build_directory, &setup.profile)?;
    // CMake's link fragments omit the driver's implicit standard library.
    // Use the evaluated toolchain rather than assuming GCC or Clang defaults.
    let stdlib = compiler.standard_library;
    link.libraries.push(stdlib);
    watch_inputs(&reader, &probe, &link.linked_files)?;
    let bridge_archive = inputs
        .map(|_| {
            read_target(&reader, &setup.profile, "quest_bridge").and_then(|bridge| {
                bridge
                    .artifacts
                    .first()
                    .map(|artifact| setup.build_directory.join(&artifact.path))
                    .ok_or_else(|| invalid("CMake omitted the bridge archive"))
            })
        })
        .transpose()?;
    let package = NativePackage {
        prefix,
        version,
        headers,
        compiler: compiler.path,
        compiler_id: compiler.id,
        compiler_version: compiler.version,
        library: link.library,
        link_search_dirs: link.search_dirs,
        link_libraries: link.libraries,
        link_options: link.options,
        runtime_library_dirs: link.runtime_dirs,
        bridge_archive,
        exact_library_files: link.library_files_by_name,
    };
    package.validate_link_search()?;
    Ok(package)
}

struct Setup {
    build_directory: PathBuf,
    profile: String,
}

fn configure(
    work: &Path,
    host: &str,
    target: &str,
    explicit: Option<&Path>,
    inputs: Option<&BridgeInputs>,
    search_prefixes: &[PathBuf],
) -> Result<Setup> {
    let source = work.join("source");
    let mut config = cmake::Config::new(&source);
    config
        .host(host)
        .target(target)
        .out_dir(work)
        .build_target("quest_bridge")
        .always_configure(true)
        .configure_arg("--fresh")
        .no_default_flags(true);
    // Tooling has no Cargo PROFILE/OPT_LEVEL/DEBUG context.
    if env::var_os("OUT_DIR").is_none() {
        config.profile("Release");
    }
    let profile = config.get_profile().to_owned();
    let cxx = env::var("CXX").unwrap_or_else(|_| "c++".to_owned());
    config
        .define("CMAKE_CXX_COMPILER", &cxx)
        .define("CMAKE_CXX_FLAGS", "");
    let mut finder = cmake_package::find_package("QuEST")
        .version("4.3")
        .define("CMAKE_BUILD_TYPE", &profile)
        .define("CMAKE_CXX_COMPILER", &cxx);
    let prefix_value = search_prefixes
        .iter()
        .map(|path| cmake_path(path))
        .collect::<Result<Vec<_>>>()?
        .join(";");
    config.define("CMAKE_PREFIX_PATH", &prefix_value);
    finder = finder.prefix_paths(search_prefixes.to_vec());
    let package_directory = explicit.map(find_package_directory).transpose()?;
    if let Some(directory) = &package_directory {
        finder = finder.define("QuEST_DIR", cmake_path(directory)?);
        config.define("QuEST_DIR", directory);
    }
    // cmake-package 0.2 requires OUT_DIR internally. Use its narrow package
    // preflight in build scripts; tooling uses find_package in the authoritative
    // project below, without mutating process-global environment variables.
    // Never request target properties: they do not evaluate generator expressions.
    if env::var_os("OUT_DIR").is_some() {
        finder
            .find()
            .map_err(|error| invalid(format!("QuEST package discovery: {error}")))?;
    }
    let build_directory = work.join("build");
    fs::create_dir_all(&source).map_err(|error| io(&source, error))?;
    fs::create_dir_all(&build_directory).map_err(|error| io(&build_directory, error))?;
    for (name, content) in [
        ("CMakeLists.txt", include_str!("../native/CMakeLists.txt")),
        ("abi.cpp", include_str!("../native/abi.cpp")),
        ("query.cpp", include_str!("../native/query.cpp")),
    ] {
        write(&source.join(name), content)?;
    }
    write(
        &source.join("bridge-inputs.cmake"),
        &bridge_input_file(inputs)?,
    )?;
    query::Writer::default()
        .request_object::<objects::CodeModelV2>()
        .request_object::<objects::ToolchainsV1>()
        .request_object::<objects::CMakeFilesV1>()
        .write_stateless(&build_directory)
        .map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
    // cmake-rs reports process failures by panicking. Contain just that call;
    // do not replace the process-wide panic hook or convert unrelated panics.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| config.build())).map_err(
        |payload| {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("CMake build failed");
            BuildError::CmakeBuild(message.to_owned())
        },
    )?;
    Ok(Setup {
        build_directory,
        profile,
    })
}

fn bridge_input_file(inputs: Option<&BridgeInputs>) -> Result<String> {
    let mut text = String::new();
    let empty = BridgeInputs::default();
    let inputs = inputs.unwrap_or(&empty);
    for (name, paths) in [
        ("QUEST_BRIDGE_SOURCES", &inputs.sources),
        (
            "QUEST_BRIDGE_INCLUDE_DIRECTORIES",
            &inputs.include_directories,
        ),
    ] {
        let _ = writeln!(text, "set({name}");
        for path in paths {
            let path = absolute(path)?;
            let value = cmake_path(&path)?;
            let mut equals = String::new();
            while value.contains(&format!("]{equals}]")) {
                equals.push('=');
            }
            let _ = writeln!(text, "  [{equals}[{value}]{equals}]");
            println!("cargo:rerun-if-changed={}", path.display());
        }
        text.push_str(")\n");
    }
    Ok(text)
}

fn cmake_path(path: &Path) -> Result<&str> {
    let text = path
        .to_str()
        .ok_or_else(|| invalid("CMake paths must be UTF-8"))?;
    if text.contains([';', '\n', '\r', '\0']) || text.contains("$<") {
        return Err(invalid(format!(
            "CMake path cannot be represented safely: {}",
            path.display()
        )));
    }
    Ok(text)
}

fn read_target(reader: &reply::Reader, profile: &str, name: &str) -> Result<Target> {
    let model: objects::CodeModelV2 = reader
        .read_object()
        .map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
    model
        .configurations
        .into_iter()
        .find(|configuration| configuration.name == profile)
        .and_then(|configuration| {
            configuration
                .targets
                .into_iter()
                .find(|target| target.name == name)
        })
        .ok_or_else(|| invalid(format!("CMake File API omitted {profile} {name}")))
}

struct CompilerConfiguration {
    path: PathBuf,
    id: String,
    version: String,
    standard_library: String,
}

fn read_compiler(reader: &reply::Reader, target: &str) -> Result<CompilerConfiguration> {
    let toolchains: objects::ToolchainsV1 = reader
        .read_object()
        .map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
    let compiler = toolchains
        .toolchains
        .into_iter()
        .find(|toolchain| toolchain.language == "CXX")
        .ok_or_else(|| invalid("CMake File API omitted the C++ compiler"))?
        .compiler;
    let compiler_path = compiler
        .path
        .as_ref()
        .ok_or_else(|| invalid("CMake did not identify its C++ compiler"))?;
    let path = fs::canonicalize(compiler_path).map_err(|error| io(compiler_path, error))?;
    let compiler_target = run(Command::new(&path).arg("-dumpmachine"))?;
    let compiler_target = String::from_utf8_lossy(&compiler_target.stdout);
    let arch = target.split_once('-').map_or(target, |(arch, _)| arch);
    if !compiler_target.trim().starts_with(&format!("{arch}-"))
        || !compiler_target.contains("linux")
    {
        return Err(invalid(format!(
            "C++ compiler targets {}, Rust targets {target}",
            compiler_target.trim()
        )));
    }
    let standard_library = compiler
        .implicit
        .link_libraries
        .iter()
        .filter_map(|path| path.to_str())
        .find(|name| matches!(*name, "stdc++" | "c++"))
        .ok_or_else(|| invalid("CMake did not identify a supported C++ standard library"))?
        .to_owned();
    Ok(CompilerConfiguration {
        path,
        id: compiler.id.unwrap_or_default(),
        version: compiler.version.unwrap_or_default(),
        standard_library,
    })
}

fn inspect_headers(
    probe: &Target,
    explicit: Option<&Path>,
) -> Result<(PathBuf, String, HeaderContext)> {
    let mut headers = HeaderContext::default();
    for group in &probe.compile_groups {
        for include in &group.includes {
            let path = fs::canonicalize(&include.path).map_err(|error| io(&include.path, error))?;
            cmake_path(&path)?;
            if include.is_system {
                push_unique(&mut headers.system_include_dirs, path.clone());
            }
            push_unique(&mut headers.include_dirs, path);
        }
        headers.definitions.extend(
            group
                .defines
                .iter()
                .map(|definition| definition.define.clone()),
        );
        for fragment in &group.compile_command_fragments {
            for flag in split_flags(&fragment.fragment)? {
                if flag.starts_with("-O") || flag.starts_with("-g") || flag == "-DNDEBUG" {
                    continue;
                }
                headers.frontend_flags.push(flag);
            }
        }
    }
    let prefix = headers
        .include_dirs
        .iter()
        .find(|directory| directory.join("quest.h").is_file())
        .and_then(|directory| directory.parent())
        .ok_or_else(|| invalid("QuEST::QuEST did not supply installed include/quest.h"))?;
    let prefix = fs::canonicalize(prefix).map_err(|error| io(prefix, error))?;
    if explicit.is_some_and(|requested| requested != prefix) {
        return Err(invalid(
            "CMake selected a different installation from the explicit QuEST prefix",
        ));
    }
    let config_header = prefix.join("include/quest/include/config.h");
    let parsed = parse_header_configuration(
        &fs::read_to_string(&config_header).map_err(|error| io(&config_header, error))?,
    )?;
    Ok((prefix, parsed.version, headers))
}

#[derive(Default)]
struct NativeLink {
    library: PathBuf,
    search_dirs: Vec<PathBuf>,
    libraries: Vec<String>,
    options: Vec<String>,
    runtime_dirs: Vec<PathBuf>,
    linked_files: BTreeSet<PathBuf>,
    library_files_by_name: BTreeMap<String, PathBuf>,
}

fn inspect_link(probe: &Target, build_directory: &Path, profile: &str) -> Result<NativeLink> {
    let file = build_directory.join(format!("quest-library-{profile}.txt"));
    let text = fs::read_to_string(&file).map_err(|error| io(&file, error))?;
    let library =
        fs::canonicalize(text.trim()).map_err(|error| io(Path::new(text.trim()), error))?;
    let model = probe
        .link
        .as_ref()
        .ok_or_else(|| invalid("CMake link query has no link model"))?;
    let mut link = NativeLink {
        library,
        ..NativeLink::default()
    };
    let tokens = model
        .command_fragments
        .iter()
        .map(|fragment| split_flags(&fragment.fragment))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    record_link_tokens(&tokens, &mut link)?;
    if !link.linked_files.contains(&link.library) {
        return Err(invalid(
            "CMake link model does not contain the selected QuEST library",
        ));
    }
    Ok(link)
}

fn record_link_tokens(tokens: &[String], link: &mut NativeLink) -> Result<()> {
    let mut iter = tokens.iter();
    while let Some(token) = iter.next() {
        if matches!(token.as_str(), "-Wl,-rpath" | "-Wl,-rpath-link") {
            let value = iter
                .next()
                .and_then(|value| value.strip_prefix("-Wl,"))
                .ok_or_else(|| invalid(format!("missing paired path after {token}")))?;
            let option = format!("{token},{value}");
            validate_link_option(&option)?;
            link.options.push(option);
        } else if token == "-L" || token == "-l" {
            let value = iter
                .next()
                .ok_or_else(|| invalid(format!("missing argument after {token}")))?;
            record_link_token(&format!("{token}{value}"), link)?;
        } else {
            record_link_token(token, link)?;
        }
    }
    Ok(())
}

fn record_link_token(token: &str, link: &mut NativeLink) -> Result<()> {
    if token.is_empty()
        || token.starts_with("-O")
        || token == "-g"
        || token == "-DNDEBUG"
        || token == "-w"
    {
        return Ok(());
    }
    if token.contains(['\n', '\r', '\0']) || token.contains("$<") {
        return Err(invalid(format!(
            "unevaluated or invalid CMake link token: {token}"
        )));
    }
    if let Some(directory) = token.strip_prefix("-L") {
        let directory = PathBuf::from(directory);
        if !directory.is_absolute() {
            return Err(invalid("relative native link search directory"));
        }
        push_unique(&mut link.search_dirs, directory);
    } else if let Some(name) = token.strip_prefix("-l") {
        if name.is_empty() {
            return Err(invalid("empty native library name"));
        }
        link.libraries.push(name.strip_prefix(':').map_or_else(
            || name.to_owned(),
            |filename| format!("dylib:+verbatim={filename}"),
        ));
    } else if Path::new(token).is_absolute() {
        record_linked_file(token, link)?;
    } else {
        validate_link_option(token)?;
        link.options.push(token.to_owned());
    }
    Ok(())
}

fn record_linked_file(token: &str, link: &mut NativeLink) -> Result<()> {
    let path = fs::canonicalize(token).map_err(|error| io(Path::new(token), error))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("native library filename must be UTF-8"))?;
    if link
        .library_files_by_name
        .get(name)
        .is_some_and(|earlier| earlier != &path)
    {
        return Err(invalid(format!(
            "ambiguous native library basename {name}: Cargo's global link search paths cannot preserve distinct absolute libraries"
        )));
    }
    let kind = native_library_kind(&path, name)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("native library lacks a parent"))?;
    cmake_path(parent)?;
    push_unique(&mut link.search_dirs, parent.to_owned());
    if kind == "dylib" {
        push_unique(&mut link.runtime_dirs, parent.to_owned());
    }
    link.libraries.push(format!("{kind}:+verbatim={name}"));
    link.library_files_by_name
        .insert(name.to_owned(), path.clone());
    link.linked_files.insert(PathBuf::from(token));
    link.linked_files.insert(path);
    Ok(())
}

fn split_flags(fragment: &str) -> Result<Vec<String>> {
    shlex::split(fragment)
        .ok_or_else(|| invalid(format!("cannot parse CMake command fragment: {fragment}")))
}

fn validate_link_option(option: &str) -> Result<()> {
    if matches!(option, "-pthread" | "-fopenmp" | "-Wl,--enable-new-dtags")
        || option.starts_with("-fopenmp=")
    {
        return Ok(());
    }
    for prefix in ["-Wl,-rpath,", "-Wl,-rpath-link,"] {
        if let Some(paths) = option.strip_prefix(prefix) {
            let directories = paths.split(':').map(PathBuf::from).collect::<Vec<_>>();
            runtime_link_args("linux", &directories)?;
            return Ok(());
        }
    }
    Err(invalid(format!(
        "unsupported or order-sensitive CMake link option {option}; Cargo cannot preserve its placement among libraries"
    )))
}

fn watch_inputs(
    reader: &reply::Reader,
    probe: &Target,
    linked_files: &BTreeSet<PathBuf>,
) -> Result<()> {
    let mut inputs = linked_files.clone();
    for include in probe
        .compile_groups
        .iter()
        .flat_map(|group| &group.includes)
    {
        collect_files(&include.path, &mut inputs, &mut BTreeSet::new())?;
    }
    let cmake_files: objects::CMakeFilesV1 = reader
        .read_object()
        .map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
    for input in cmake_files.inputs {
        if input.path.is_absolute() && input.path.is_file() {
            inputs.insert(fs::canonicalize(&input.path).map_err(|error| io(&input.path, error))?);
            inputs.insert(input.path);
        }
    }
    for path in inputs {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    Ok(())
}
fn find_package_directory(prefix: &Path) -> Result<PathBuf> {
    // QuEST_DIR chooses the requested package even if a different package is
    // discoverable through unrelated CMAKE_PREFIX_PATH entries.
    ["lib/cmake/QuEST", "lib64/cmake/QuEST", "share/QuEST/cmake"]
        .into_iter()
        .map(|relative| prefix.join(relative))
        .find(|path| path.join("QuESTConfig.cmake").is_file())
        .ok_or_else(|| {
            invalid(format!(
                "{} has headers but no installed QuEST CMake package",
                prefix.display()
            ))
        })
}

fn native_library_kind(path: &Path, name: &str) -> Result<&'static str> {
    let extension = path.extension().and_then(|extension| extension.to_str());
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("a")) {
        return Ok("static");
    }
    let versioned_shared = name
        .as_bytes()
        .windows(4)
        .any(|window| window.eq_ignore_ascii_case(b".so."));
    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("so")) || versioned_shared {
        return Ok("dylib");
    }
    Err(invalid(format!(
        "unrecognized native library: {}",
        path.display()
    )))
}

fn collect_files(
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let canonical = fs::canonicalize(directory).map_err(|e| io(directory, e))?;
    files.insert(directory.to_owned());
    files.insert(canonical.clone());
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(|e| io(directory, e))? {
        let path = entry.map_err(|e| io(directory, e))?.path();
        if path.is_dir() {
            collect_files(&path, files, visited)?;
        } else if path.is_file() {
            files.insert(fs::canonicalize(&path).map_err(|e| io(&path, e))?);
            files.insert(path);
        }
    }
    visited.remove(&canonical);
    Ok(())
}

fn write(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).map_err(|e| io(path, e))
}
fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    fn fixture_package(prefix: &Path) -> googletest::Result<()> {
        let include = prefix.join("include");
        let package = prefix.join("lib/cmake/QuEST");
        fs::create_dir_all(include.join("quest/include")).or_fail()?;
        fs::create_dir_all(&package).or_fail()?;
        fs::write(include.join("quest.h"), "#include <quest/include/config.h>\nusing qreal = double;\ninline bool isQuESTEnvInit() { return false; }\n").or_fail()?;
        fs::write(include.join("quest/include/config.h"), "#define QUEST_VERSION_MAJOR 4\n#define QUEST_VERSION_MINOR 3\n#define QUEST_VERSION_PATCH 9\n#define QUEST_FLOAT_PRECISION 2\n#define QUEST_INCLUDE_DEPRECATED_FUNCTIONS 0\n").or_fail()?;
        fs::write(
            prefix.join("lib/libQuEST.so"),
            "unused imported location: archive build never links it",
        )
        .or_fail()?;
        fs::write(
            package.join("QuESTConfigVersion.cmake"),
            "set(PACKAGE_VERSION 4.3.9)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\n",
        )
        .or_fail()?;
        fs::write(
            package.join("QuESTConfig.cmake"),
            r#"set(QuEST_VERSION 4.3.9)
get_filename_component(fixture_prefix "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
add_library(QuEST::QuEST SHARED IMPORTED)
set_target_properties(QuEST::QuEST PROPERTIES
  IMPORTED_LOCATION "${fixture_prefix}/lib/libQuEST.so"
  INTERFACE_INCLUDE_DIRECTORIES "${fixture_prefix}/include"
  INTERFACE_COMPILE_DEFINITIONS "$<$<CONFIG:Release>:QUEST_FIXTURE_EVALUATED=1>;$<$<CONFIG:Debug>:QUEST_FIXTURE_EVALUATED=2>"
  INTERFACE_COMPILE_OPTIONS "$<$<COMPILE_LANGUAGE:CXX>:-DQUEST_FIXTURE_CXX=1>"
  INTERFACE_LINK_OPTIONS "$<$<CONFIG:Release>:LINKER:--enable-new-dtags>"
  INTERFACE_LINK_LIBRARIES "$<$<CONFIG:Release>:m>")
"#,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn reconfiguration_uses_current_prefix_after_explicit_selection_is_removed()
    -> googletest::Result<()> {
        let fixture = tempfile::tempdir().or_fail()?;
        let first = fixture.path().join("first");
        let second = fixture.path().join("second");
        fixture_package(&first)?;
        fixture_package(&second)?;
        let work = fixture.path().join("reused build");
        let output = run(Command::new("rustc").arg("-vV")).or_fail()?;
        let version = String::from_utf8_lossy(&output.stdout);
        let host = version
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or_else(|| invalid("missing test host"))
            .or_fail()?;
        for (explicit, search, expected) in [
            (Some(first.as_path()), second.as_path(), first.as_path()),
            (None, second.as_path(), second.as_path()),
            (None, first.as_path(), first.as_path()),
        ] {
            let setup =
                configure(&work, host, host, explicit, None, &[search.to_owned()]).or_fail()?;
            let reader = reply::Reader::from_build_dir(&setup.build_directory).or_fail()?;
            let query = read_target(&reader, &setup.profile, "quest_link_query").or_fail()?;
            let (selected, _, _) = inspect_headers(&query, None).or_fail()?;
            expect_that!(selected, eq(expected));
        }
        Ok(())
    }

    #[gtest]
    fn cmake_build_evaluates_target_requirements_and_paths_with_spaces() -> googletest::Result<()> {
        let fixture = tempfile::tempdir().or_fail()?;
        let prefix = fixture.path().join("prefix with spaces");
        fixture_package(&prefix)?;
        let source = fixture.path().join("generated bridge.cpp");
        fs::write(&source, "#include <quest.h>\n#if !QUEST_FIXTURE_EVALUATED || !QUEST_FIXTURE_CXX\n#error imported target requirements were lost\n#endif\nqreal bridge_value() { return 1.0; }\n").or_fail()?;
        let work = fixture.path().join("build with spaces");
        let host_output = run(Command::new("rustc").arg("-vV")).or_fail()?;
        let host_text = String::from_utf8_lossy(&host_output.stdout);
        let host = host_text
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or_else(|| invalid("missing test host"))
            .or_fail()?;
        let setup = configure(
            &work,
            host,
            host,
            Some(&prefix),
            Some(&BridgeInputs {
                sources: vec![source],
                include_directories: Vec::new(),
            }),
            &[],
        )
        .or_fail()?;
        let reader = reply::Reader::from_build_dir(&setup.build_directory).or_fail()?;
        let query = read_target(&reader, &setup.profile, "quest_link_query").or_fail()?;
        let (_, version, headers) = inspect_headers(&query, Some(&prefix)).or_fail()?;
        let link = inspect_link(&query, &setup.build_directory, &setup.profile).or_fail()?;
        expect_eq!(version, "4.3.9");
        let expected_profile =
            env::var("QUEST_BUILD_FIXTURE_PROFILE").unwrap_or_else(|_| "Release".to_owned());
        expect_eq!(&setup.profile, &expected_profile);
        let compiler = read_compiler(&reader, host).or_fail()?;
        validate_compiler_environment(host, Some(&compiler.path)).or_fail()?;
        let definition = if expected_profile == "Debug" {
            "QUEST_FIXTURE_EVALUATED=2"
        } else {
            "QUEST_FIXTURE_EVALUATED=1"
        };
        expect_that!(headers.definitions, contains(eq(definition)));
        expect_that!(
            headers.frontend_flags,
            contains(eq("-DQUEST_FIXTURE_CXX=1"))
        );
        if expected_profile == "Release" {
            expect_that!(link.options, contains(eq("-Wl,--enable-new-dtags")));
            expect_that!(link.libraries, contains(eq("m")));
        } else {
            expect_that!(
                link.options
                    .iter()
                    .any(|option| option == "-Wl,--enable-new-dtags"),
                eq(false)
            );
            expect_that!(
                link.libraries.iter().any(|library| library == "m"),
                eq(false)
            );
        }
        let archive = setup.build_directory.join("libquest_bridge.a");
        expect_that!(archive.is_file(), eq(true));
        let contents = run(Command::new("ar").arg("t").arg(archive)).or_fail()?;
        expect_that!(
            String::from_utf8_lossy(&contents.stdout),
            contains_substring("generated_bridge.cpp.o")
        );
        Ok(())
    }

    #[gtest]
    fn cargo_profiles_and_explicit_cxx_remain_coherent_in_child_processes() -> googletest::Result<()>
    {
        let executable = env::current_exe().or_fail()?;
        let paths = env::var_os("PATH")
            .ok_or_else(|| invalid("test PATH missing"))
            .or_fail()?;
        let compiler = env::split_paths(&paths)
            .map(|directory| directory.join("c++"))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| invalid("test C++ compiler missing"))
            .or_fail()?;
        let compiler = fs::canonicalize(compiler).or_fail()?;
        for (profile, optimization, debug, expected) in [
            ("debug", "0", "true", "Debug"),
            ("release", "3", "false", "Release"),
        ] {
            let out = tempfile::tempdir().or_fail()?;
            let output = run(Command::new(&executable)
                .args([
                    "--exact",
                    "probe::tests::cmake_build_evaluates_target_requirements_and_paths_with_spaces",
                    "--nocapture",
                ])
                .env("OUT_DIR", out.path())
                .env("PROFILE", profile)
                .env("OPT_LEVEL", optimization)
                .env("DEBUG", debug)
                .env("CXX", &compiler)
                .env("QUEST_BUILD_FIXTURE_PROFILE", expected))
            .or_fail()?;
            expect_that!(
                String::from_utf8_lossy(&output.stdout),
                contains_substring("1 passed")
            );
        }
        Ok(())
    }

    fn fixture_host() -> googletest::Result<String> {
        let output = run(Command::new("rustc").arg("-vV")).or_fail()?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .map(str::to_owned)
            .ok_or_else(|| invalid("missing test host"))
            .or_fail()
    }

    #[gtest]
    fn missing_imported_dependency_returns_typed_failure_without_an_archive()
    -> googletest::Result<()> {
        let fixture = tempfile::tempdir().or_fail()?;
        let prefix = fixture.path().join("package");
        fixture_package(&prefix)?;
        let package = prefix.join("lib/cmake/QuEST/QuESTConfig.cmake");
        let source = fs::read_to_string(&package).or_fail()?;
        fs::write(
            &package,
            source.replace("$<$<CONFIG:Release>:m>", "Missing::Dependency"),
        )
        .or_fail()?;
        let work = fixture.path().join("build");
        let host = fixture_host()?;
        let result = configure(&work, &host, &host, Some(&prefix), None, &[]);
        expect_that!(matches!(result, Err(BuildError::CmakeBuild(_))), eq(true));
        expect_that!(work.join("build/libquest_bridge.a").exists(), eq(false));
        Ok(())
    }

    #[gtest]
    fn compiled_abi_guard_rejects_incompatible_precision_before_archiving() -> googletest::Result<()>
    {
        let fixture = tempfile::tempdir().or_fail()?;
        let prefix = fixture.path().join("package");
        fixture_package(&prefix)?;
        let header = prefix.join("include/quest/include/config.h");
        let source = fs::read_to_string(&header).or_fail()?;
        fs::write(
            &header,
            source.replace("QUEST_FLOAT_PRECISION 2", "QUEST_FLOAT_PRECISION 1"),
        )
        .or_fail()?;
        let work = fixture.path().join("build");
        let host = fixture_host()?;
        let result = configure(&work, &host, &host, Some(&prefix), None, &[]);
        expect_that!(matches!(result, Err(BuildError::CmakeBuild(_))), eq(true));
        expect_that!(work.join("build/libquest_bridge.a").exists(), eq(false));
        Ok(())
    }

    #[gtest]
    fn rejects_an_unreferenced_shadow_file_in_an_earlier_search_directory() -> googletest::Result<()>
    {
        let fixture = tempfile::tempdir().or_fail()?;
        let earlier = fixture.path().join("earlier");
        let intended = fixture.path().join("intended");
        fs::create_dir_all(&earlier).or_fail()?;
        fs::create_dir_all(&intended).or_fail()?;
        fs::write(earlier.join("libQuEST.so"), "quest").or_fail()?;
        fs::write(earlier.join("libdependency.so"), "stale dependency").or_fail()?;
        let dependency = intended.join("libdependency.so");
        fs::write(&dependency, "intended dependency").or_fail()?;
        let mut link = NativeLink::default();
        record_linked_file(
            cmake_path(&earlier.join("libQuEST.so")).or_fail()?,
            &mut link,
        )
        .or_fail()?;
        record_linked_file(cmake_path(&dependency).or_fail()?, &mut link).or_fail()?;
        expect_that!(
            crate::package::validate_library_resolution(
                &link.search_dirs,
                &link.library_files_by_name
            )
            .is_err(),
            eq(true)
        );
        Ok(())
    }

    #[gtest]
    fn rejects_distinct_absolute_libraries_with_the_same_basename() -> googletest::Result<()> {
        let directory = tempfile::tempdir().or_fail()?;
        let mut link = NativeLink::default();
        for child in ["first", "second"] {
            let folder = directory.path().join(child);
            fs::create_dir_all(&folder).or_fail()?;
            fs::write(folder.join("libsame.so"), child).or_fail()?;
        }
        let first = directory.path().join("first/libsame.so");
        let second = directory.path().join("second/libsame.so");
        record_linked_file(cmake_path(&first).or_fail()?, &mut link).or_fail()?;
        let result = record_linked_file(cmake_path(&second).or_fail()?, &mut link);
        expect_that!(result.is_err(), eq(true));
        Ok(())
    }

    #[cfg(unix)]
    #[gtest]
    fn watches_library_and_header_symlink_lookups_as_well_as_targets() -> googletest::Result<()> {
        let directory = tempfile::tempdir().or_fail()?;
        let native = directory.path().join("libnative.so.7");
        let alias = directory.path().join("libnative.so");
        fs::write(&native, "native").or_fail()?;
        std::os::unix::fs::symlink(&native, &alias).or_fail()?;
        let mut link = NativeLink::default();
        record_linked_file(cmake_path(&alias).or_fail()?, &mut link).or_fail()?;
        expect_that!(&link.linked_files, contains(eq(&alias)));
        expect_that!(&link.linked_files, contains(eq(&native)));
        let headers = directory.path().join("include");
        let target_headers = directory.path().join("actual headers");
        fs::create_dir_all(&target_headers).or_fail()?;
        let header = target_headers.join("quest.h");
        fs::write(&header, "header").or_fail()?;
        std::os::unix::fs::symlink(&target_headers, &headers).or_fail()?;
        let header_alias = target_headers.join("alias.h");
        std::os::unix::fs::symlink(&header, &header_alias).or_fail()?;
        let mut watched = BTreeSet::new();
        collect_files(&headers, &mut watched, &mut BTreeSet::new()).or_fail()?;
        for path in [&header, &headers.join("quest.h"), &headers.join("alias.h")] {
            expect_that!(&watched, contains(eq(path)));
        }
        Ok(())
    }

    #[gtest]
    fn paired_mpi_paths_and_library_order_survive_cargo_translation() -> googletest::Result<()> {
        let mut link = NativeLink::default();
        let tokens = split_flags(
            "-Wl,-rpath '-Wl,/opt/mpi lib' -Wl,--enable-new-dtags -lfirst -lsecond -lfirst",
        )
        .or_fail()?;
        record_link_tokens(&tokens, &mut link).or_fail()?;
        expect_that!(
            link.options,
            elements_are![eq("-Wl,-rpath,/opt/mpi lib"), eq("-Wl,--enable-new-dtags")]
        );
        expect_that!(
            link.libraries,
            elements_are![eq("first"), eq("second"), eq("first")]
        );
        Ok(())
    }

    #[gtest]
    fn rejects_unevaluated_expressions_and_order_sensitive_link_state() -> googletest::Result<()> {
        for text in [
            "-Wl,--start-group",
            "-Wl,--end-group",
            "-Wl,--whole-archive",
            "-Wl,--as-needed",
            "-Wl,--no-as-needed",
            "-Wl,-Bstatic",
            "-Bdynamic",
            "-Tcustom.ld",
            "$<LINK_ONLY:foo>",
            "-Wl,-rpath",
            "-Wl,-rpath -Wl,/opt/mpi,--as-needed",
        ] {
            let mut link = NativeLink::default();
            let tokens = split_flags(text).or_fail()?;
            expect_that!(record_link_tokens(&tokens, &mut link).is_err(), eq(true));
        }
        Ok(())
    }

    #[gtest]
    fn bridge_paths_preserve_spaces_and_literal_cmake_variable_syntax() -> googletest::Result<()> {
        let inputs = BridgeInputs {
            sources: vec![PathBuf::from("/opt/path with spaces/${literal}/bridge.cpp")],
            include_directories: vec![PathBuf::from("/opt/headers]")],
        };
        let file = bridge_input_file(Some(&inputs)).or_fail()?;
        expect_that!(
            file,
            contains_substring("[[/opt/path with spaces/${literal}/bridge.cpp]]")
        );
        for path in ["/opt/foo;bar", "/opt/$<CONFIG>", "/opt/foo\nbar"] {
            expect_that!(cmake_path(Path::new(path)).is_err(), eq(true));
        }
        Ok(())
    }

    #[gtest]
    fn explicit_shared_files_keep_exact_filename_and_direct_runtime_directory()
    -> googletest::Result<()> {
        let directory = tempfile::tempdir().or_fail()?;
        let shared = directory.path().join("libfixture.so.7");
        let static_archive = directory.path().join("libstatic.a");
        fs::write(&shared, "fixture").or_fail()?;
        fs::write(&static_archive, "fixture").or_fail()?;
        let mut link = NativeLink::default();
        record_linked_file(cmake_path(&shared).or_fail()?, &mut link).or_fail()?;
        record_linked_file(cmake_path(&static_archive).or_fail()?, &mut link).or_fail()?;
        expect_that!(
            link.libraries,
            elements_are![
                eq("dylib:+verbatim=libfixture.so.7"),
                eq("static:+verbatim=libstatic.a")
            ]
        );
        expect_that!(
            link.runtime_dirs,
            elements_are![eq(&directory.path().canonicalize().or_fail()?)]
        );
        Ok(())
    }
}
