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
    let search_prefixes = env::var_os("CMAKE_PREFIX_PATH").map_or_else(Vec::new, |value| {
        env::split_paths(&value).collect::<Vec<_>>()
    });
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
    let exported_includes = read_exported_includes(&setup.build_directory, &setup.profile)?;
    let (prefix, configuration, mut headers) =
        inspect_headers(&probe, explicit.as_deref(), &exported_includes)?;
    headers.implicit_include_dirs = compiler.implicit_include_dirs;
    headers.sysroot.clone_from(&setup.sysroot);
    validate_header_context(&headers, target)?;
    let mut link = inspect_link(&probe, &setup.build_directory, &setup.profile, target)?;
    // CMake's link fragments omit the driver's implicit standard library.
    // Use the evaluated toolchain rather than assuming GCC or Clang defaults.
    let stdlib = compiler.standard_library;
    link.libraries.push(stdlib);
    watch_inputs(&reader, &probe, &link.linked_files, &exported_includes)?;
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
        target: target.to_owned(),
        prefix,
        version: configuration.version,
        mpi_enabled: configuration.mpi_enabled,
        subcommunicators_enabled: configuration.subcommunicators_enabled,
        headers,
        compiler: compiler.path,
        compiler_id: compiler.id,
        compiler_version: compiler.version,
        library: link.library,
        link_search_dirs: link.search_dirs,
        framework_search_dirs: link.framework_search_dirs,
        link_libraries: link.libraries,
        link_options: link.options,
        runtime_library_dirs: link.runtime_dirs,
        bridge_archive,
        exact_library_files: link.library_files_by_name,
        build_directory: setup.build_directory.clone(),
        mpi_probe: read_target(&reader, &setup.profile, "quest_mpi_abi")?
            .artifacts
            .first()
            .map(|artifact| setup.build_directory.join(&artifact.path))
            .ok_or_else(|| invalid("CMake omitted MPI ABI witness target"))?,
    };
    package.validate_link_search()?;
    Ok(package)
}

struct Setup {
    sysroot: Option<PathBuf>,
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
    let sysroot = if target.ends_with("-apple-darwin") {
        let path = if let Some(root) = env::var_os("SDKROOT") {
            PathBuf::from(root)
        } else {
            let output = run(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]))?;
            PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        };
        Some(validate_sdk(&path)?)
    } else {
        None
    };
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
    if let Some(sysroot) = &sysroot {
        config.define("CMAKE_OSX_SYSROOT", cmake_path(sysroot)?);
    }
    let prefix_value = search_prefixes
        .iter()
        .map(|path| cmake_path(path))
        .collect::<Result<Vec<_>>>()?
        .join(";");
    config.define("CMAKE_PREFIX_PATH", &prefix_value);
    let package_directory = explicit.map(find_package_directory).transpose()?;
    if let Some(directory) = &package_directory {
        config.define("QuEST_DIR", directory);
    }
    let build_directory = work.join("build");
    fs::create_dir_all(&source).map_err(|error| io(&source, error))?;
    fs::create_dir_all(&build_directory).map_err(|error| io(&build_directory, error))?;
    for (name, content) in [
        ("CMakeLists.txt", include_str!("../native/CMakeLists.txt")),
        ("abi.cpp", include_str!("../native/abi.cpp")),
        ("query.cpp", include_str!("../native/query.cpp")),
        ("mpi_abi.c", include_str!("../native/mpi_abi.c")),
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
    if let Some(requested) = &sysroot {
        let evaluated = build_directory.join("quest-sysroot.txt");
        let value = fs::read_to_string(&evaluated).map_err(|error| io(&evaluated, error))?;
        if validate_sdk(Path::new(value.trim()))? != *requested {
            return Err(invalid("CMake changed the selected Darwin SDK"));
        }
    }
    Ok(Setup {
        sysroot,
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
    implicit_include_dirs: Vec<PathBuf>,
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
    validate_compiler_target(target, compiler_target.trim())?;
    let id = compiler.id.unwrap_or_default();
    let standard_library = select_standard_library(target, &id, &compiler.implicit.link_libraries)?;
    let implicit_include_dirs = compiler
        .implicit
        .include_directories
        .into_iter()
        .map(|path| {
            cmake_path(&path)?;
            if !path.is_absolute() {
                return Err(invalid("relative compiler implicit include directory"));
            }
            Ok(path)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CompilerConfiguration {
        path,
        id,
        version: compiler.version.unwrap_or_default(),
        standard_library,
        implicit_include_dirs,
    })
}

fn select_standard_library(target: &str, id: &str, libraries: &[PathBuf]) -> Result<String> {
    libraries
        .iter()
        .filter_map(|path| path.to_str())
        .find(|name| matches!(*name, "stdc++" | "c++"))
        // CMake can omit implicit libraries for the validated Darwin Clang driver.
        .or_else(|| {
            (target.ends_with("-apple-darwin") && matches!(id, "Clang" | "AppleClang"))
                .then_some("c++")
        })
        .map(str::to_owned)
        .ok_or_else(|| invalid("CMake did not identify a supported C++ standard library"))
}

fn validate_compiler_target(target: &str, compiler: &str) -> Result<()> {
    let arch = target.split('-').next().unwrap_or_default();
    let compiler_arch = compiler.split('-').next().unwrap_or_default();
    let darwin = target.ends_with("-apple-darwin");
    let same_arch =
        arch == compiler_arch || (darwin && arch == "aarch64" && compiler_arch == "arm64");
    let same_platform = if darwin {
        compiler.contains("-apple-darwin")
    } else {
        // Red Hat's native GNU/Linux GCC target omits the final GNU suffix.
        compiler.contains("-linux-gnu")
            || compiler.strip_suffix("-redhat-linux") == Some(compiler_arch)
    };
    if !same_arch || !same_platform {
        return Err(invalid(format!(
            "C++ compiler targets {compiler}, Rust targets {target}"
        )));
    }
    Ok(())
}

fn validate_sdk(path: &Path) -> Result<PathBuf> {
    cmake_path(path)?;
    if !path.is_absolute()
        || !path.join("usr/include").is_dir()
        || !path.join("System/Library/Frameworks").is_dir()
        || !(path.join("SDKSettings.plist").is_file() || path.join("SDKSettings.json").is_file())
    {
        return Err(invalid(
            "SDKROOT must select an absolute installed macOS SDK with SDKSettings and system headers",
        ));
    }
    fs::canonicalize(path).map_err(|error| io(path, error))
}

fn validate_header_context(headers: &HeaderContext, target: &str) -> Result<()> {
    let mut flags = headers.frontend_flags.iter();
    while let Some(flag) = flags.next() {
        let (kind, value) = match flag.as_str() {
            "-arch" | "-target" | "--target" | "-isysroot" | "--sysroot" => (
                flag.as_str(),
                flags
                    .next()
                    .map(String::as_str)
                    .ok_or_else(|| invalid(format!("missing compile argument after {flag}")))?,
            ),
            _ => {
                if let Some(value) = flag
                    .strip_prefix("--target=")
                    .or_else(|| flag.strip_prefix("-target="))
                {
                    ("-target", value)
                } else if let Some(value) = flag.strip_prefix("--sysroot=") {
                    ("-isysroot", value)
                } else if let Some(value) = flag.strip_prefix("-isysroot") {
                    ("-isysroot", value)
                } else if flag.starts_with("-Xarch_") || matches!(flag.as_str(), "-m32" | "-m64") {
                    return Err(invalid(format!(
                        "unsupported compile architecture override {flag}"
                    )));
                } else {
                    continue;
                }
            }
        };
        match kind {
            "-arch" => {
                let expected = match target {
                    "aarch64-apple-darwin" => "arm64",
                    "x86_64-apple-darwin" => "x86_64",
                    _ => return Err(invalid("-arch compile option requires a Darwin target")),
                };
                if value != expected {
                    return Err(invalid(
                        "compile architecture differs from the native Cargo target",
                    ));
                }
            }
            "-target" | "--target" => validate_compiler_target(target, value)?,
            _ => {
                let path = Path::new(value);
                if !path.is_absolute() {
                    return Err(invalid("compile sysroot must be absolute"));
                }
                let canonical = fs::canonicalize(path).map_err(|error| io(path, error))?;
                if headers.sysroot.as_ref() != Some(&canonical) {
                    return Err(invalid("compile sysroot differs from the evaluated SDK"));
                }
            }
        }
    }
    Ok(())
}

fn read_exported_includes(build_directory: &Path, profile: &str) -> Result<Vec<PathBuf>> {
    let file = build_directory.join(format!("quest-includes-{profile}.txt"));
    let text = fs::read_to_string(&file).map_err(|error| io(&file, error))?;
    text.lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let path = Path::new(line);
            cmake_path(path)?;
            if !path.is_absolute() {
                return Err(invalid("relative imported QuEST include directory"));
            }
            fs::canonicalize(path).map_err(|error| io(path, error))
        })
        .collect()
}

fn inspect_headers(
    probe: &Target,
    explicit: Option<&Path>,
    exported_includes: &[PathBuf],
) -> Result<(PathBuf, crate::HeaderConfiguration, HeaderContext)> {
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
    for path in exported_includes {
        if !headers.include_dirs.contains(path) {
            // A target include absent from the codemodel is compiler-implicit.
            push_unique(&mut headers.include_dirs, path.clone());
            push_unique(&mut headers.system_include_dirs, path.clone());
        }
    }
    let prefix = exported_includes
        .iter()
        .chain(&headers.include_dirs)
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
    Ok((prefix, parsed, headers))
}

#[derive(Default)]
struct NativeLink {
    target_os: Option<&'static str>,
    architecture: Option<&'static str>,
    sysroot: Option<PathBuf>,
    framework_search_dirs: Vec<PathBuf>,
    library: PathBuf,
    search_dirs: Vec<PathBuf>,
    libraries: Vec<String>,
    options: Vec<String>,
    runtime_dirs: Vec<PathBuf>,
    linked_files: BTreeSet<PathBuf>,
    library_files_by_name: BTreeMap<String, PathBuf>,
}

fn inspect_link(
    probe: &Target,
    build_directory: &Path,
    profile: &str,
    target: &str,
) -> Result<NativeLink> {
    let file = build_directory.join(format!("quest-library-{profile}.txt"));
    let text = fs::read_to_string(&file).map_err(|error| io(&file, error))?;
    let library =
        fs::canonicalize(text.trim()).map_err(|error| io(Path::new(text.trim()), error))?;
    let model = probe
        .link
        .as_ref()
        .ok_or_else(|| invalid("CMake link query has no link model"))?;
    let sysroot = if target.ends_with("-apple-darwin") {
        let file = build_directory.join("quest-sysroot.txt");
        let value = fs::read_to_string(&file).map_err(|error| io(&file, error))?;
        Some(validate_sdk(Path::new(value.trim()))?)
    } else {
        None
    };
    let mut link = NativeLink {
        sysroot,
        target_os: Some(if target.ends_with("-apple-darwin") {
            "macos"
        } else {
            "linux"
        }),
        architecture: match target {
            "aarch64-apple-darwin" => Some("arm64"),
            "x86_64-apple-darwin" => Some("x86_64"),
            _ => None,
        },
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
            validate_link_option(&option, link.target_os.unwrap_or("linux"))?;
            link.options.push(option);
        } else if token == "-framework" && link.target_os == Some("macos") {
            let name = iter
                .next()
                .ok_or_else(|| invalid("missing framework name"))?;
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                return Err(invalid("unsupported framework name"));
            }
            link.libraries.push(format!("framework={name}"));
        } else if token == "-isysroot" && link.target_os == Some("macos") {
            let root = iter
                .next()
                .ok_or_else(|| invalid("missing Darwin sysroot"))?;
            let sdk = validate_sdk(Path::new(root))?;
            if link
                .sysroot
                .as_ref()
                .is_some_and(|expected| expected != &sdk)
            {
                return Err(invalid(
                    "Darwin link sysroot differs from the evaluated SDK",
                ));
            }
            runtime_link_args("macos", std::slice::from_ref(&sdk))?;
            link.options
                .push(format!("-Wl,-syslibroot,{}", sdk.display()));
        } else if token == "-arch" && link.target_os == Some("macos") {
            let arch = iter
                .next()
                .ok_or_else(|| invalid("missing Darwin architecture"))?;
            if !matches!(arch.as_str(), "arm64" | "x86_64")
                || link.architecture.is_some_and(|expected| expected != arch)
            {
                return Err(invalid(
                    "Darwin link architecture differs from the native Cargo target",
                ));
            }
            // Rust already selects the validated native architecture.
        } else if token == "-L"
            || token == "-l"
            || (token == "-F" && link.target_os == Some("macos"))
        {
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
    if let Some(directory) = token
        .strip_prefix("-F")
        .filter(|_| link.target_os == Some("macos"))
    {
        let directory = PathBuf::from(directory);
        runtime_link_args("macos", std::slice::from_ref(&directory))?;
        push_unique(&mut link.framework_search_dirs, directory);
    } else if let Some(directory) = token.strip_prefix("-L") {
        let directory = PathBuf::from(directory);
        runtime_link_args(
            link.target_os.unwrap_or("linux"),
            std::slice::from_ref(&directory),
        )?;
        push_unique(&mut link.search_dirs, directory);
    } else if let Some(name) = token.strip_prefix("-l") {
        if name.is_empty() || name.contains(['=', ',', '/', ' ']) || name.starts_with('-') {
            return Err(invalid("invalid native library name"));
        }
        if link.target_os == Some("macos") && name.starts_with(':') {
            return Err(invalid(
                "Darwin does not support GNU -l:filename library syntax",
            ));
        }
        link.libraries.push(name.strip_prefix(':').map_or_else(
            || name.to_owned(),
            |filename| format!("dylib:+verbatim={filename}"),
        ));
    } else if Path::new(token).is_absolute() {
        record_linked_file(token, link)?;
    } else {
        validate_link_option(token, link.target_os.unwrap_or("linux"))?;
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
    if kind == "dylib" && link.target_os == Some("macos") {
        let stem = name
            .strip_prefix("lib")
            .and_then(|name| name.strip_suffix(".dylib"))
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid("Darwin shared libraries must have a libNAME.dylib filename"))?;
        // ld64 uses -lNAME, and does not implement GNU -l:filename. Keep the
        // version in NAME and validate search resolution against the exact file.
        link.libraries.push(format!("dylib={stem}"));
    } else {
        link.libraries.push(format!("{kind}:+verbatim={name}"));
    }
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

fn validate_link_option(option: &str, target_os: &str) -> Result<()> {
    if matches!(option, "-pthread" | "-fopenmp") || option.starts_with("-fopenmp=") {
        return Ok(());
    }
    if target_os == "linux" && option == "-Wl,--enable-new-dtags" {
        return Ok(());
    }
    if target_os == "macos"
        && matches!(
            option,
            "-Wl,-search_paths_first" | "-Wl,-headerpad_max_install_names"
        )
    {
        // Global Mach-O options: path-first search matches exact-file validation;
        // header padding changes capacity, without changing library ordering.
        return Ok(());
    }
    if target_os == "macos"
        && option
            .strip_prefix("-mmacosx-version-min=")
            .is_some_and(|value| {
                !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
            })
    {
        return Ok(());
    }
    for prefix in ["-Wl,-rpath,", "-Wl,-rpath-link,"] {
        if prefix == "-Wl,-rpath-link," && target_os != "linux" {
            continue;
        }
        if let Some(paths) = option.strip_prefix(prefix) {
            let directories = if target_os == "linux" {
                paths.split(':').map(PathBuf::from).collect()
            } else {
                vec![PathBuf::from(paths)]
            };
            runtime_link_args(target_os, &directories)?;
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
    exported_includes: &[PathBuf],
) -> Result<()> {
    let mut inputs = linked_files.clone();
    for include in probe
        .compile_groups
        .iter()
        .flat_map(|group| &group.includes)
    {
        collect_files(&include.path, &mut inputs, &mut BTreeSet::new())?;
    }
    for include in exported_includes {
        collect_files(include, &mut inputs, &mut BTreeSet::new())?;
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
    if extension
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "so" | "dylib"))
        || versioned_shared
    {
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
            prefix.join(if cfg!(target_os = "macos") {
                "lib/libQuEST.dylib"
            } else {
                "lib/libQuEST.so"
            }),
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
  IMPORTED_LOCATION "${fixture_prefix}/lib/libQuEST${CMAKE_SHARED_LIBRARY_SUFFIX}"
  INTERFACE_INCLUDE_DIRECTORIES "${fixture_prefix}/include"
  INTERFACE_COMPILE_DEFINITIONS "$<$<CONFIG:Release>:QUEST_FIXTURE_EVALUATED=1>;$<$<CONFIG:Debug>:QUEST_FIXTURE_EVALUATED=2>"
  INTERFACE_COMPILE_OPTIONS "$<$<COMPILE_LANGUAGE:CXX>:-DQUEST_FIXTURE_CXX=1>"
  INTERFACE_LINK_OPTIONS "$<$<AND:$<CONFIG:Release>,$<PLATFORM_ID:Linux>>:LINKER:--enable-new-dtags>"
  INTERFACE_LINK_LIBRARIES "$<$<CONFIG:Release>:m>")
"#,
        )
        .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn cargo_discovery_evaluates_the_selected_package_once() -> googletest::Result<()> {
        if env::var_os("QUEST_BUILD_SINGLE_EVALUATION_CHILD").is_none() {
            let work = tempfile::tempdir().or_fail()?;
            let output = Command::new(env::current_exe().or_fail()?)
                .args([
                    "--exact",
                    "probe::tests::cargo_discovery_evaluates_the_selected_package_once",
                    "--nocapture",
                ])
                .env("OUT_DIR", work.path())
                .env("PROFILE", "release")
                .env("OPT_LEVEL", "3")
                .env("DEBUG", "false")
                .env("QUEST_BUILD_SINGLE_EVALUATION_CHILD", "1")
                .output()
                .or_fail()?;
            if !output.status.success() {
                return fail!(
                    "discovery child failed:\n{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return Ok(());
        }

        let fixture = tempfile::tempdir().or_fail()?;
        let prefix = fixture.path().join("package");
        fixture_package(&prefix)?;
        let config = prefix.join("lib/cmake/QuEST/QuESTConfig.cmake");
        let original = fs::read_to_string(&config).or_fail()?;
        let marker = prefix.join("evaluated.marker");
        let guard = format!(
            "if(EXISTS \"{}\")\n  message(FATAL_ERROR \"QuEST package evaluated twice\")\nendif()\nfile(WRITE \"{}\" \"once\")\n",
            marker.display(),
            marker.display()
        );
        fs::write(&config, format!("{guard}{original}")).or_fail()?;
        let host = fixture_host()?;
        let work = fixture.path().join("build");
        configure(&work, &host, &host, Some(&prefix), None, &[]).or_fail()?;
        expect_that!(marker.is_file(), eq(true));
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
            let (selected, _, _) = inspect_headers(&query, None, &[]).or_fail()?;
            expect_that!(selected, eq(&expected.canonicalize().or_fail()?));
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
        let (_, configuration, headers) =
            inspect_headers(&query, Some(&prefix.canonicalize().or_fail()?), &[]).or_fail()?;
        let link = inspect_link(&query, &setup.build_directory, &setup.profile, host).or_fail()?;
        expect_eq!(configuration.version, "4.3.9");
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
            if !host.ends_with("-apple-darwin") {
                expect_that!(link.options, contains(eq("-Wl,--enable-new-dtags")));
            }
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
        expect_that!(
            &link.linked_files,
            contains(eq(&native.canonicalize().or_fail()?))
        );
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
        for path in [
            &header.canonicalize().or_fail()?,
            &headers.join("quest.h"),
            &headers.join("alias.h"),
        ] {
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
    #[gtest]
    fn darwin_dylibs_preserve_versioned_filename() -> googletest::Result<()> {
        let directory = tempfile::tempdir()?;
        for name in ["libQuEST.dylib", "libomp.5.dylib"] {
            let library = directory.path().join(name);
            fs::write(&library, "fixture")?;
            let mut link = NativeLink {
                target_os: Some("macos"),
                ..NativeLink::default()
            };
            record_linked_file(cmake_path(&library)?, &mut link)?;
            expect_eq!(
                link.libraries,
                vec![format!(
                    "dylib={}",
                    name.strip_prefix("lib")
                        .unwrap()
                        .strip_suffix(".dylib")
                        .unwrap()
                )]
            );
            expect_eq!(link.runtime_dirs, vec![directory.path().canonicalize()?]);
        }
        Ok(())
    }

    #[gtest]
    fn darwin_framework_pairs_preserve_order_and_reject_state() -> googletest::Result<()> {
        let mut link = NativeLink {
            target_os: Some("macos"),
            ..NativeLink::default()
        };
        record_link_tokens(
            &split_flags(
                "-F '/SDK/System/Library/Frameworks' -framework Accelerate -lomp -framework Foundation",
            )?,
            &mut link,
        )?;
        expect_eq!(
            link.framework_search_dirs,
            vec![PathBuf::from("/SDK/System/Library/Frameworks")]
        );
        expect_eq!(
            link.libraries,
            vec!["framework=Accelerate", "omp", "framework=Foundation"]
        );
        for flags in [
            "-framework",
            "-framework -lomp",
            "-F",
            "-Frelative",
            "-Wl,-force_load,/tmp/lib.a",
            "-Wl,-all_load",
            "-Wl,--enable-new-dtags",
            "-Wl,-rpath,/a:/b",
        ] {
            expect_true!(
                record_link_tokens(
                    &split_flags(flags)?,
                    &mut NativeLink {
                        target_os: Some("macos"),
                        ..NativeLink::default()
                    }
                )
                .is_err()
            );
        }
        Ok(())
    }

    #[gtest]
    fn compiler_target_validation_preserves_native_platform_and_architecture()
    -> googletest::Result<()> {
        for (rust, compiler) in [
            ("aarch64-apple-darwin", "arm64-apple-darwin25.0.0"),
            ("aarch64-apple-darwin", "aarch64-apple-darwin"),
            ("x86_64-apple-darwin", "x86_64-apple-darwin24.6"),
            ("x86_64-unknown-linux-gnu", "x86_64-pc-linux-gnu"),
            ("x86_64-unknown-linux-gnu", "x86_64-redhat-linux"),
            ("aarch64-unknown-linux-gnu", "aarch64-redhat-linux"),
            ("x86_64-unknown-linux-gnu", "x86_64-redhat-linux-gnu"),
        ] {
            validate_compiler_target(rust, compiler)?;
        }
        for (rust, compiler) in [
            ("aarch64-apple-darwin", "x86_64-apple-darwin"),
            ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu"),
            ("x86_64-unknown-linux-gnu", "x86_64-apple-darwin"),
            ("x86_64-unknown-linux-gnu", "x86_64-linux-musl"),
            ("x86_64-unknown-linux-gnu", "x86_64-redhat-linux-musl"),
            ("aarch64-unknown-linux-gnu", "x86_64-redhat-linux"),
            ("x86_64-unknown-linux-gnu", "aarch64-redhat-linux"),
            ("x86_64-unknown-linux-gnu", "x86_64-w64-mingw32"),
        ] {
            expect_true!(validate_compiler_target(rust, compiler).is_err());
        }
        Ok(())
    }

    #[gtest]
    fn darwin_sdk_requires_an_absolute_installed_sdk() -> googletest::Result<()> {
        let directory = tempfile::tempdir()?;
        expect_true!(validate_sdk(Path::new("relative.sdk")).is_err());
        expect_true!(validate_sdk(directory.path()).is_err());
        fs::create_dir_all(directory.path().join("usr/include"))?;
        fs::create_dir_all(directory.path().join("System/Library/Frameworks"))?;
        fs::write(directory.path().join("SDKSettings.json"), "{}")?;
        expect_eq!(
            validate_sdk(directory.path())?,
            directory.path().canonicalize()?
        );
        Ok(())
    }

    #[gtest]
    fn darwin_link_architecture_must_match_the_cargo_target() -> googletest::Result<()> {
        let mut link = NativeLink {
            target_os: Some("macos"),
            architecture: Some("arm64"),
            ..NativeLink::default()
        };
        record_link_tokens(&split_flags("-arch arm64")?, &mut link)?;
        expect_true!(record_link_tokens(&split_flags("-arch x86_64")?, &mut link).is_err());
        Ok(())
    }
    #[gtest]
    fn libcxx_fallback_is_confined_to_darwin_clang() -> googletest::Result<()> {
        for id in ["Clang", "AppleClang"] {
            expect_eq!(
                select_standard_library("aarch64-apple-darwin", id, &[])?,
                "c++"
            );
        }
        expect_true!(select_standard_library("aarch64-apple-darwin", "GNU", &[]).is_err());
        expect_true!(select_standard_library("x86_64-unknown-linux-gnu", "Clang", &[]).is_err());
        expect_eq!(
            select_standard_library(
                "x86_64-unknown-linux-gnu",
                "GNU",
                &[PathBuf::from("stdc++")]
            )?,
            "stdc++"
        );
        Ok(())
    }
    #[gtest]
    fn darwin_link_sysroot_must_match_the_evaluated_sdk() -> googletest::Result<()> {
        let directory = tempfile::tempdir()?;
        let mut roots = Vec::new();
        for name in ["first.sdk", "second.sdk"] {
            let root = directory.path().join(name);
            fs::create_dir_all(root.join("usr/include"))?;
            fs::create_dir_all(root.join("System/Library/Frameworks"))?;
            fs::write(root.join("SDKSettings.json"), "{}")?;
            roots.push(root.canonicalize()?);
        }
        let mut link = NativeLink {
            target_os: Some("macos"),
            sysroot: Some(roots[0].clone()),
            ..NativeLink::default()
        };
        expect_true!(
            record_link_tokens(
                &["-isysroot".into(), roots[1].to_string_lossy().into_owned()],
                &mut link
            )
            .is_err()
        );
        record_link_tokens(
            &["-isysroot".into(), roots[0].to_string_lossy().into_owned()],
            &mut link,
        )?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    #[gtest]
    fn darwin_exact_dylib_metadata_links_a_real_rust_consumer() -> googletest::Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?;
        let source = root.join("native.cpp");
        let dylib = root.join("libnative_fixture.7.dylib");
        fs::write(&source, "extern \"C\" int native_value() { return 73; }")?;
        run(
            Command::new(env::var_os("CXX").unwrap_or_else(|| "c++".into()))
                .arg("-dynamiclib")
                .arg(&source)
                .arg("-o")
                .arg(&dylib),
        )?;
        let rust = root.join("main.rs");
        fs::write(
            &rust,
            "unsafe extern \"C\" { fn native_value() -> i32; } fn main() { assert_eq!(unsafe { native_value() }, 73); }",
        )?;
        let mut link = NativeLink {
            target_os: Some("macos"),
            ..NativeLink::default()
        };
        record_linked_file(cmake_path(&dylib)?, &mut link)?;
        let executable = root.join("consumer");
        let mut command = Command::new("rustc");
        command.arg(&rust).arg("-o").arg(&executable);
        for dir in &link.search_dirs {
            command.arg("-L").arg(format!("native={}", dir.display()));
        }
        for library in &link.libraries {
            command.arg("-l").arg(library);
        }
        run(&mut command)?;
        run(&mut Command::new(&executable))?;
        Ok(())
    }

    #[gtest]
    fn compile_only_target_and_sdk_overrides_cannot_diverge_from_native_context()
    -> googletest::Result<()> {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().canonicalize()?;
        let other = root.join("other.sdk");
        fs::create_dir(&other)?;
        let mut headers = HeaderContext {
            sysroot: Some(root.clone()),
            ..HeaderContext::default()
        };
        for flags in [
            "-arch x86_64".to_owned(),
            "-target x86_64-apple-darwin".to_owned(),
            "--target=x86_64-apple-darwin".to_owned(),
            format!("-isysroot {}", other.display()),
            format!("--sysroot={}", other.display()),
        ] {
            headers.frontend_flags = split_flags(&flags)?;
            expect_true!(
                validate_header_context(&headers, "aarch64-apple-darwin").is_err(),
                "admitted {flags}"
            );
        }
        for flags in [
            "-arch arm64".to_owned(),
            "-target arm64-apple-darwin25.0.0".to_owned(),
            "--target=aarch64-apple-darwin".to_owned(),
            format!("-isysroot {}", root.display()),
            format!("--sysroot={}", root.display()),
        ] {
            headers.frontend_flags = split_flags(&flags)?;
            validate_header_context(&headers, "aarch64-apple-darwin")?;
        }
        Ok(())
    }
    #[cfg(unix)]
    #[gtest]
    fn imported_header_identity_survives_compiler_implicit_include_suppression()
    -> googletest::Result<()> {
        use std::os::unix::fs::PermissionsExt as _;

        if let Some(prefix) = env::var_os("QUEST_IMPLICIT_INCLUDE_CHILD") {
            let prefix = PathBuf::from(prefix).canonicalize()?;
            let work = tempfile::tempdir()?;
            let host = fixture_host()?;
            let result = discover(work.path(), &host, &host, None);
            if env::var_os("QUEST_IMPLICIT_NO_EXPORT").is_some() {
                expect_that!(
                    result.unwrap_err().to_string(),
                    contains_substring("did not supply installed include/quest.h")
                );
                return Ok(());
            }
            let package = result?;
            expect_eq!(&package.prefix, &prefix);
            expect_that!(
                &package.headers.include_dirs,
                contains(eq(&prefix.join("include")))
            );
            expect_that!(
                &package.headers.implicit_include_dirs,
                contains(eq(&prefix.join("include")))
            );
            return Ok(());
        }
        let fixture = tempfile::tempdir()?;
        let prefix = fixture.path().join("package");
        fixture_package(&prefix)?;
        let compiler = env::var_os("CXX").unwrap_or_else(|| "c++".into());
        let compiler = if Path::new(&compiler).is_absolute() {
            PathBuf::from(compiler)
        } else {
            env::split_paths(&env::var_os("PATH").ok_or_else(|| invalid("missing PATH"))?)
                .map(|dir| dir.join(&compiler))
                .find(|path| path.is_file())
                .ok_or_else(|| invalid("missing compiler"))?
        };
        let wrapper = fixture.path().join("cxx-wrapper");
        let quote = |value: &Path| format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"));
        fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\nexec {} -isystem {} \"$@\"\n",
                quote(&compiler),
                quote(&prefix.join("include").canonicalize()?)
            ),
        )?;
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755))?;
        let mut command = Command::new(env::current_exe()?);
        command.args(["--exact", "probe::tests::imported_header_identity_survives_compiler_implicit_include_suppression", "--nocapture"])
            .env("QUEST_IMPLICIT_INCLUDE_CHILD", &prefix).env("QUEST_ROOT", &prefix).env("CXX", &wrapper);
        for key in ["QUEST_DIR", "QuEST_DIR", "QuEST_ROOT"] {
            command.env_remove(key);
        }
        let output = run(&mut command)?;
        expect_that!(
            String::from_utf8_lossy(&output.stdout),
            contains_substring(format!(
                "cargo:rerun-if-changed={}/include/quest.h",
                prefix.canonicalize()?.display()
            ))
        );
        let config = prefix.join("lib/cmake/QuEST/QuESTConfig.cmake");
        let source = fs::read_to_string(&config)?;
        fs::write(
            &config,
            source.replace(
                "  INTERFACE_INCLUDE_DIRECTORIES \"${fixture_prefix}/include\"\n",
                "",
            ),
        )?;
        command.env("QUEST_IMPLICIT_NO_EXPORT", "1");
        run(&mut command)?;
        Ok(())
    }
}
