use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cmake_file_api::{objects, query, reply};
use objects::codemodel_v2::Target;

use crate::{
    BuildError, FileIdentity, NativeConfig, Result, clean_loader_environment, explicit_prefix,
    invalid, io, parse_header_configuration, requested_runtime_dirs, run, runtime_link_args,
    validate_target,
};

const CMAKE_SOURCE: &str = r#"cmake_minimum_required(VERSION 3.24)
project(quest_native_configuration LANGUAGES CXX)
find_package(QuEST 4.3 REQUIRED CONFIG)
if(NOT TARGET QuEST::QuEST)
    message(FATAL_ERROR "The installed package does not export QuEST::QuEST")
endif()
add_executable(quest_native_probe probe.cpp)
target_compile_features(quest_native_probe PRIVATE cxx_std_20)
target_link_libraries(quest_native_probe PRIVATE QuEST::QuEST)
set(probe_paths "$<TARGET_FILE_DIR:QuEST::QuEST>")
if(NOT QUEST_PROBE_RUNTIME_PATHS STREQUAL "")
    string(APPEND probe_paths ":${QUEST_PROBE_RUNTIME_PATHS}")
endif()
target_link_options(quest_native_probe PRIVATE "LINKER:--disable-new-dtags" "LINKER:-rpath,${probe_paths}")
file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/quest-library-$<CONFIG>.txt" CONTENT "$<TARGET_FILE:QuEST::QuEST>\n")
"#;

const PROBE_SOURCE: &str = r"#include <quest.h>
#if QUEST_VERSION_MAJOR != 4 || QUEST_VERSION_MINOR != 3
#error The supported native API is QuEST 4.3.x
#endif
#if QUEST_FLOAT_PRECISION != 2 || QUEST_INCLUDE_DEPRECATED_FUNCTIONS != 0
#error QuEST must use binary64 and disable deprecated APIs
#endif
static_assert(sizeof(qreal) == 8);
int main() { return isQuESTEnvInit() ? 1 : 0; }
";

pub fn discover(work_directory: &Path, host: &str, target: &str) -> Result<NativeConfig> {
    validate_target(host, target)?;
    crate::validate_compiler_environment(target, None)?;
    let setup = configure_probe(work_directory)?;
    let reader = reply::Reader::from_build_dir(&setup.build_directory)
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    let probe = read_probe_target(&reader)?;
    let compiler = read_compiler(&reader, target)?;
    let headers = inspect_headers(&probe, setup.explicit.as_deref())?;
    let mut link = inspect_link(&probe, &setup.build_directory)?;
    let runtime_library_dirs =
        verify_probe_and_runtime_closure(&setup, &probe, &link.library, &mut link.linked_files)?;
    let inputs = collect_inputs(&reader, &headers.prefix, &compiler.path, link.linked_files)?;
    let compilation = inspect_compilation(&probe)?;
    Ok(NativeConfig {
        schema: 2,
        target: target.into(),
        prefix: headers.prefix,
        version: headers.version,
        features: headers.features,
        compiler: compiler.path,
        compiler_id: compiler.id,
        compiler_version: compiler.version,
        include_dirs: headers.include_dirs,
        system_include_dirs: headers.system_include_dirs,
        compile_definitions: compilation.definitions,
        compile_options: compilation.options,
        library: link.library,
        runtime_library_dirs,
        requested_runtime_dirs: setup.requested_runtime_dirs,
        link_search_dirs: link.search_dirs,
        link_libraries: link.libraries,
        link_options: link.options,
        inputs,
    })
}

struct ProbeSetup {
    build_directory: PathBuf,
    cmake: OsString,
    explicit: Option<PathBuf>,
    requested_runtime_dirs: Vec<PathBuf>,
}

fn configure_probe(work_directory: &Path) -> Result<ProbeSetup> {
    if env::var_os("CMAKE_TOOLCHAIN_FILE").is_some() {
        return Err(invalid(
            "CMAKE_TOOLCHAIN_FILE requires a separately verified target recipe; the initial native Linux discovery does not accept toolchain overrides",
        ));
    }
    let explicit = explicit_prefix()?;
    let requested_runtime_dirs = requested_runtime_dirs()?;
    runtime_link_args("linux", &requested_runtime_dirs)?;
    let source_directory = work_directory.join("source");
    let build_directory = work_directory.join("build");
    fs::create_dir_all(&source_directory).map_err(|e| io(&source_directory, e))?;
    fs::create_dir_all(&build_directory).map_err(|e| io(&build_directory, e))?;
    write(&source_directory.join("CMakeLists.txt"), CMAKE_SOURCE)?;
    write(&source_directory.join("probe.cpp"), PROBE_SOURCE)?;
    query::Writer::default()
        .request_object::<objects::CodeModelV2>()
        .request_object::<objects::ToolchainsV1>()
        .request_object::<objects::CMakeFilesV1>()
        .write_stateless(&build_directory)
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;

    let cmake = env::var_os("CMAKE").unwrap_or_else(|| "cmake".into());
    let mut configure = Command::new(&cmake);
    configure
        .arg("--fresh")
        .arg("-S")
        .arg(&source_directory)
        .arg("-B")
        .arg(&build_directory)
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!(
            "-DQUEST_PROBE_RUNTIME_PATHS={}",
            requested_runtime_dirs
                .iter()
                .map(|path| path.to_string_lossy())
                .collect::<Vec<_>>()
                .join(":")
        ));
    if let Some(prefix) = &explicit {
        let package_directory = find_package_directory(prefix)?;
        configure.arg(format!("-DQuEST_DIR={}", package_directory.display()));
    }
    run(&mut configure)?;
    Ok(ProbeSetup {
        build_directory,
        cmake,
        explicit,
        requested_runtime_dirs,
    })
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

fn read_probe_target(reader: &reply::Reader) -> Result<Target> {
    let model: objects::CodeModelV2 = reader
        .read_object()
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    model
        .configurations
        .into_iter()
        .find(|configuration| configuration.name == "Release")
        .and_then(|configuration| {
            configuration
                .targets
                .into_iter()
                .find(|target| target.name == "quest_native_probe")
        })
        .ok_or_else(|| invalid("CMake File API omitted the Release quest_native_probe target"))
}

struct CompilerConfiguration {
    path: PathBuf,
    id: String,
    version: String,
}

fn read_compiler(reader: &reply::Reader, target: &str) -> Result<CompilerConfiguration> {
    let toolchains: objects::ToolchainsV1 = reader
        .read_object()
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    let compiler = toolchains
        .toolchains
        .into_iter()
        .find(|toolchain| toolchain.language == "CXX")
        .ok_or_else(|| invalid("CMake File API omitted the C++ compiler"))?
        .compiler;
    let compiler_path = compiler
        .path
        .as_ref()
        .ok_or_else(|| invalid("CMake did not identify its C++ compiler path"))?;
    let path = fs::canonicalize(compiler_path).map_err(|e| io(compiler_path, e))?;
    let compiler_target = run(Command::new(&path).arg("-dumpmachine"))?;
    let compiler_target = String::from_utf8_lossy(&compiler_target.stdout);
    let expected_arch = target.split_once('-').map_or(target, |(arch, _)| arch);
    if !compiler_target
        .trim()
        .starts_with(&format!("{expected_arch}-"))
    {
        return Err(invalid(format!(
            "C++ compiler targets {}, Rust targets {target}",
            compiler_target.trim()
        )));
    }
    Ok(CompilerConfiguration {
        path,
        id: compiler.id.unwrap_or_default(),
        version: compiler.version.unwrap_or_default(),
    })
}

struct NativeHeaders {
    prefix: PathBuf,
    version: String,
    features: BTreeMap<String, String>,
    include_dirs: Vec<PathBuf>,
    system_include_dirs: Vec<PathBuf>,
}

fn inspect_headers(probe: &Target, explicit: Option<&Path>) -> Result<NativeHeaders> {
    // Include search order is part of the imported target's semantics: sorting
    // paths can select a different header when multiple directories contain it.
    let mut include_dirs = Vec::new();
    let mut system_include_dirs = Vec::new();
    for include in probe
        .compile_groups
        .iter()
        .flat_map(|group| &group.includes)
    {
        let path = fs::canonicalize(&include.path).map_err(|e| io(&include.path, e))?;
        if include.is_system {
            push_unique(&mut system_include_dirs, path.clone());
        }
        push_unique(&mut include_dirs, path);
    }
    let prefix = include_dirs
        .iter()
        .find(|directory| directory.join("quest.h").is_file())
        .and_then(|directory| directory.parent())
        .ok_or_else(|| invalid("QuEST::QuEST did not supply installed include/quest.h"))?;
    let prefix = fs::canonicalize(prefix).map_err(|e| io(prefix, e))?;
    if explicit.is_some_and(|path| path != prefix) {
        return Err(invalid(
            "CMake selected an installation different from the explicitly requested QuEST prefix",
        ));
    }
    let header = prefix.join("include/quest/include/config.h");
    let parsed =
        parse_header_configuration(&fs::read_to_string(&header).map_err(|e| io(&header, e))?)?;
    Ok(NativeHeaders {
        prefix,
        version: parsed.version,
        features: parsed.features,
        include_dirs,
        system_include_dirs,
    })
}

struct NativeLink {
    library: PathBuf,
    search_dirs: Vec<PathBuf>,
    libraries: Vec<String>,
    options: Vec<String>,
    linked_files: BTreeSet<PathBuf>,
}

fn inspect_link(probe: &Target, build_directory: &Path) -> Result<NativeLink> {
    let library_file = build_directory.join("quest-library-Release.txt");
    let library_text = fs::read_to_string(&library_file).map_err(|e| io(&library_file, e))?;
    let library =
        fs::canonicalize(library_text.trim()).map_err(|e| io(Path::new(library_text.trim()), e))?;
    let link = probe
        .link
        .as_ref()
        .ok_or_else(|| invalid("CMake probe has no link model"))?;
    let mut native_link = NativeLink {
        library,
        search_dirs: Vec::new(),
        libraries: Vec::new(),
        options: Vec::new(),
        linked_files: BTreeSet::new(),
    };
    for fragment in &link.command_fragments {
        for token in split_flags(&fragment.fragment)? {
            record_link_token(&token, &mut native_link)?;
        }
    }
    if !native_link.linked_files.contains(&native_link.library) {
        return Err(invalid(
            "the CMake probe link model does not contain the selected QuEST library",
        ));
    }
    Ok(native_link)
}

fn record_link_token(token: &str, link: &mut NativeLink) -> Result<()> {
    if token.is_empty()
        || token.starts_with("-O")
        || token == "-g"
        || token == "-DNDEBUG"
        || token.starts_with("-Wl,-rpath,")
        || token == "-Wl,--disable-new-dtags"
    {
        return Ok(());
    }
    if let Some(directory) = token.strip_prefix("-L") {
        push_unique(&mut link.search_dirs, PathBuf::from(directory));
    } else if let Some(name) = token.strip_prefix("-l") {
        link.libraries.push(name.strip_prefix(':').map_or_else(
            || name.to_owned(),
            |filename| format!("dylib:+verbatim={filename}"),
        ));
    } else if Path::new(token).is_absolute() && Path::new(token).is_file() {
        record_linked_file(token, link)?;
    } else if token.starts_with('-') {
        validate_link_option(token)?;
        link.options.push(token.to_owned());
    } else {
        return Err(invalid(format!("unsupported CMake link fragment: {token}")));
    }
    Ok(())
}

fn record_linked_file(token: &str, link: &mut NativeLink) -> Result<()> {
    let path = fs::canonicalize(token).map_err(|e| io(Path::new(token), e))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("native library filenames must be UTF-8"))?;
    let kind = native_library_kind(&path, name)?;
    push_unique(
        &mut link.search_dirs,
        path.parent()
            .ok_or_else(|| invalid("native library lacks a parent"))?
            .to_owned(),
    );
    link.libraries.push(format!("{kind}:+verbatim={name}"));
    link.linked_files.insert(path);
    Ok(())
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

fn verify_probe_and_runtime_closure(
    setup: &ProbeSetup,
    probe: &Target,
    library: &Path,
    linked_files: &mut BTreeSet<PathBuf>,
) -> Result<Vec<PathBuf>> {
    run(Command::new(&setup.cmake)
        .arg("--build")
        .arg(&setup.build_directory)
        .arg("--config")
        .arg("Release")
        .arg("--target")
        .arg("quest_native_probe"))?;
    let executable = probe
        .artifacts
        .first()
        .map(|artifact| setup.build_directory.join(&artifact.path))
        .ok_or_else(|| invalid("CMake probe omitted its executable artifact"))?;
    run(clean_loader_environment(&mut Command::new(&executable))).map_err(|error| invalid(format!(
        "the linked QuEST probe did not run with loader variables unset: {error}\nSet QUEST_RUNTIME_LIBRARY_PATH to explicit directories covering the native dependency closure and configure again.")))?;
    let closure_output = run(clean_loader_environment(
        Command::new("ldd").arg(&executable),
    ))?;
    let closure_files = parse_ldd_paths(&String::from_utf8_lossy(&closure_output.stdout))?;
    linked_files.extend(closure_files.iter().cloned());
    let mut runtime_library_dirs = Vec::new();
    let extension = library.extension().and_then(|extension| extension.to_str());
    if extension.is_none_or(|extension| !extension.eq_ignore_ascii_case("a")) {
        push_unique(
            &mut runtime_library_dirs,
            library
                .parent()
                .ok_or_else(|| invalid("QuEST library lacks a parent"))?
                .to_owned(),
        );
    }
    for directory in &setup.requested_runtime_dirs {
        push_unique(&mut runtime_library_dirs, directory.clone());
    }
    for path in &closure_files {
        if let Some(parent) = path.parent() {
            push_unique(&mut runtime_library_dirs, parent.to_owned());
        }
    }
    runtime_link_args("linux", &runtime_library_dirs)?;
    Ok(runtime_library_dirs)
}

fn collect_inputs(
    reader: &reply::Reader,
    prefix: &Path,
    compiler_path: &Path,
    mut inputs: BTreeSet<PathBuf>,
) -> Result<Vec<FileIdentity>> {
    inputs.insert(compiler_path.to_owned());
    collect_files(&prefix.join("include"), &mut inputs, &mut BTreeSet::new())?;
    let cmake_files: objects::CMakeFilesV1 = reader
        .read_object()
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    for input in cmake_files.inputs {
        if input.path.is_absolute() && input.path.starts_with(prefix) && input.path.is_file() {
            inputs.insert(fs::canonicalize(&input.path).map_err(|e| io(&input.path, e))?);
        }
    }
    inputs
        .into_iter()
        .map(|path| {
            println!("cargo:rerun-if-changed={}", path.display());
            FileIdentity::read(&path)
        })
        .collect()
}

struct Compilation {
    definitions: Vec<String>,
    options: Vec<String>,
}

fn inspect_compilation(probe: &Target) -> Result<Compilation> {
    let definitions = probe
        .compile_groups
        .iter()
        .flat_map(|group| &group.defines)
        .map(|definition| definition.define.clone())
        .collect();
    let mut options = Vec::new();
    for fragment in probe
        .compile_groups
        .iter()
        .flat_map(|group| &group.compile_command_fragments)
    {
        for flag in split_flags(&fragment.fragment)? {
            if flag.starts_with("-O")
                || flag.starts_with("-g")
                || flag.starts_with("-std=")
                || flag == "-DNDEBUG"
            {
                continue;
            }
            options.push(flag);
        }
    }
    Ok(Compilation {
        definitions,
        options,
    })
}

fn parse_ldd_paths(output: &str) -> Result<Vec<PathBuf>> {
    let mut paths = BTreeSet::new();
    for line in output.lines() {
        if line.contains("not found") {
            return Err(invalid(format!(
                "unresolved native dependency: {}",
                line.trim()
            )));
        }
        let path = line
            .split_once("=>")
            .map_or(line, |(_, resolved)| resolved)
            .trim();
        let path = path.rsplit_once(" (").map_or(path, |(path, _)| path).trim();
        if path.starts_with('/') {
            // Preserve SONAME lookup paths as well as canonical identities: a
            // retargeted symlink must invalidate a previously verified closure.
            fs::canonicalize(path).map_err(|e| io(Path::new(path), e))?;
            paths.insert(PathBuf::from(path));
        }
    }
    if paths.is_empty() {
        return Err(invalid(
            "ldd did not report the probe's native library closure",
        ));
    }
    Ok(paths.into_iter().collect())
}

fn split_flags(fragment: &str) -> Result<Vec<String>> {
    shlex::split(fragment)
        .ok_or_else(|| invalid(format!("cannot parse CMake command fragment: {fragment}")))
}

fn validate_link_option(option: &str) -> Result<()> {
    // Cargo emits link libraries separately from final link arguments. Admit
    // only order-independent driver options; archive groups, as-needed state,
    // -Bstatic/-Bdynamic and arbitrary linker scripts cannot be reordered.
    if matches!(option, "-pthread" | "-fopenmp") || option.starts_with("-fopenmp=") {
        Ok(())
    } else {
        Err(invalid(format!(
            "unsupported or order-sensitive CMake link option {option}; the current Cargo recipe cannot preserve its placement among libraries"
        )))
    }
}

fn collect_files(
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let directory = fs::canonicalize(directory).map_err(|e| io(directory, e))?;
    if !visited.insert(directory.clone()) {
        return Ok(());
    }
    for entry in fs::read_dir(&directory).map_err(|e| io(&directory, e))? {
        let path = entry.map_err(|e| io(&directory, e))?.path();
        if path.is_dir() {
            collect_files(&path, files, visited)?;
        } else if path.is_file() {
            files.insert(fs::canonicalize(&path).map_err(|e| io(&path, e))?);
        }
    }
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

    #[gtest]
    fn rejects_linker_state_that_cargo_cannot_keep_between_libraries() -> googletest::Result<()> {
        for option in [
            "-Wl,--start-group",
            "-Wl,--end-group",
            "-Wl,--whole-archive",
            "-Wl,--as-needed",
            "-Bstatic",
            "-Tcustom.ld",
        ] {
            expect_that!(validate_link_option(option).is_err(), eq(true));
        }
        validate_link_option("-pthread").or_fail()?;
        validate_link_option("-fopenmp").or_fail()?;
        Ok(())
    }
}
