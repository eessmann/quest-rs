use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cmake_file_api::{objects, query, reply};

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

const PROBE_SOURCE: &str = r#"#include <quest.h>
#if QUEST_VERSION_MAJOR != 4 || QUEST_VERSION_MINOR != 3
#error The supported native API is QuEST 4.3.x
#endif
#if QUEST_FLOAT_PRECISION != 2 || QUEST_INCLUDE_DEPRECATED_FUNCTIONS != 0
#error QuEST must use binary64 and disable deprecated APIs
#endif
static_assert(sizeof(qreal) == 8);
int main() { return isQuESTEnvInit() ? 1 : 0; }
"#;

pub(crate) fn discover(work_directory: &Path, host: &str, target: &str) -> Result<NativeConfig> {
    validate_target(host, target)?;
    crate::validate_compiler_environment(target, None)?;
    if env::var_os("CMAKE_TOOLCHAIN_FILE").is_some() {
        return Err(invalid(
            "CMAKE_TOOLCHAIN_FILE requires a separately verified target recipe; the initial native Linux discovery does not accept toolchain overrides",
        ));
    }
    let explicit = explicit_prefix()?;
    let requested_runtime_dirs = requested_runtime_dirs()?;
    // Validate representability before placing the values in CMake arguments.
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
        // QuEST_DIR chooses the requested package even if a different package is
        // discoverable through unrelated CMAKE_PREFIX_PATH entries.
        let package_directory = ["lib/cmake/QuEST", "lib64/cmake/QuEST", "share/QuEST/cmake"]
            .into_iter()
            .map(|relative| prefix.join(relative))
            .find(|path| path.join("QuESTConfig.cmake").is_file())
            .ok_or_else(|| {
                invalid(format!(
                    "{} has headers but no installed QuEST CMake package",
                    prefix.display()
                ))
            })?;
        configure.arg(format!("-DQuEST_DIR={}", package_directory.display()));
    }
    run(&mut configure)?;

    let reader = reply::Reader::from_build_dir(&build_directory)
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    let model: objects::CodeModelV2 = reader
        .read_object()
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    let probe = model
        .configurations
        .iter()
        .find(|configuration| configuration.name == "Release")
        .and_then(|configuration| {
            configuration
                .targets
                .iter()
                .find(|target| target.name == "quest_native_probe")
        })
        .ok_or_else(|| invalid("CMake File API omitted the Release quest_native_probe target"))?;
    let toolchains: objects::ToolchainsV1 = reader
        .read_object()
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    let compiler = &toolchains
        .toolchains
        .iter()
        .find(|toolchain| toolchain.language == "CXX")
        .ok_or_else(|| invalid("CMake File API omitted the C++ compiler"))?
        .compiler;
    let compiler_path = compiler
        .path
        .as_ref()
        .ok_or_else(|| invalid("CMake did not identify its C++ compiler path"))?;
    let compiler_path = fs::canonicalize(compiler_path).map_err(|e| io(compiler_path, e))?;
    let compiler_target = run(Command::new(&compiler_path).arg("-dumpmachine"))?;
    let compiler_target = String::from_utf8_lossy(&compiler_target.stdout);
    let expected_arch = target.split('-').next().unwrap_or("");
    if !compiler_target
        .trim()
        .starts_with(&format!("{expected_arch}-"))
    {
        return Err(invalid(format!(
            "C++ compiler targets {}, Rust targets {target}",
            compiler_target.trim()
        )));
    }
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
    if explicit.as_ref().is_some_and(|path| path != &prefix) {
        return Err(invalid(
            "CMake selected an installation different from the explicitly requested QuEST prefix",
        ));
    }
    let header = prefix.join("include/quest/include/config.h");
    let parsed =
        parse_header_configuration(&fs::read_to_string(&header).map_err(|e| io(&header, e))?)?;
    let library_file = build_directory.join("quest-library-Release.txt");
    let library_text = fs::read_to_string(&library_file).map_err(|e| io(&library_file, e))?;
    let library =
        fs::canonicalize(library_text.trim()).map_err(|e| io(Path::new(library_text.trim()), e))?;
    let link = probe
        .link
        .as_ref()
        .ok_or_else(|| invalid("CMake probe has no link model"))?;
    let mut link_search_dirs = Vec::new();
    let mut link_libraries = Vec::new();
    let mut link_options = Vec::new();
    let mut linked_files = BTreeSet::new();
    for fragment in &link.command_fragments {
        let tokens = split_flags(&fragment.fragment)?;
        for token in tokens {
            if token.is_empty()
                || token.starts_with("-O")
                || token == "-g"
                || token == "-DNDEBUG"
                || token.starts_with("-Wl,-rpath,")
                || token == "-Wl,--disable-new-dtags"
            {
                continue;
            }
            if let Some(directory) = token.strip_prefix("-L") {
                push_unique(&mut link_search_dirs, PathBuf::from(directory));
            } else if let Some(name) = token.strip_prefix("-l") {
                if let Some(filename) = name.strip_prefix(':') {
                    link_libraries.push(format!("dylib:+verbatim={filename}"));
                } else {
                    link_libraries.push(name.to_owned());
                }
            } else if Path::new(&token).is_absolute() && Path::new(&token).is_file() {
                let path = fs::canonicalize(&token).map_err(|e| io(Path::new(&token), e))?;
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| invalid("native library filenames must be UTF-8"))?;
                let kind = if name.ends_with(".a") {
                    "static"
                } else if name.ends_with(".so") || name.contains(".so.") {
                    "dylib"
                } else {
                    return Err(invalid(format!(
                        "unrecognized native library: {}",
                        path.display()
                    )));
                };
                push_unique(
                    &mut link_search_dirs,
                    path.parent()
                        .ok_or_else(|| invalid("native library lacks a parent"))?
                        .to_owned(),
                );
                link_libraries.push(format!("{kind}:+verbatim={name}"));
                linked_files.insert(path);
            } else if token.starts_with('-') {
                validate_link_option(&token)?;
                link_options.push(token);
            } else {
                return Err(invalid(format!("unsupported CMake link fragment: {token}")));
            }
        }
    }
    if !linked_files.contains(&library) {
        return Err(invalid(
            "the CMake probe link model does not contain the selected QuEST library",
        ));
    }
    run(Command::new(&cmake)
        .arg("--build")
        .arg(&build_directory)
        .arg("--config")
        .arg("Release")
        .arg("--target")
        .arg("quest_native_probe"))?;
    let executable = probe
        .artifacts
        .first()
        .map(|artifact| build_directory.join(&artifact.path))
        .ok_or_else(|| invalid("CMake probe omitted its executable artifact"))?;
    run(clean_loader_environment(&mut Command::new(&executable))).map_err(|error| invalid(format!(
        "the linked QuEST probe did not run with loader variables unset: {error}\nSet QUEST_RUNTIME_LIBRARY_PATH to explicit directories covering the native dependency closure and configure again.")))?;
    let closure_output = run(clean_loader_environment(
        Command::new("ldd").arg(&executable),
    ))?;
    let closure_text = String::from_utf8_lossy(&closure_output.stdout);
    let closure_files = parse_ldd_paths(&closure_text)?;
    linked_files.extend(closure_files.iter().cloned());
    let mut runtime_library_dirs = Vec::new();
    if library.extension().is_none_or(|extension| extension != "a") {
        push_unique(
            &mut runtime_library_dirs,
            library
                .parent()
                .ok_or_else(|| invalid("QuEST library lacks a parent"))?
                .to_owned(),
        );
    }
    for directory in &requested_runtime_dirs {
        push_unique(&mut runtime_library_dirs, directory.clone());
    }
    for path in &closure_files {
        if let Some(parent) = path.parent() {
            push_unique(&mut runtime_library_dirs, parent.to_owned());
        }
    }
    runtime_link_args("linux", &runtime_library_dirs)?;

    let mut inputs = linked_files;
    inputs.insert(compiler_path.clone());
    collect_files(&prefix.join("include"), &mut inputs, &mut BTreeSet::new())?;
    let cmake_files: objects::CMakeFilesV1 = reader
        .read_object()
        .map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
    for input in cmake_files.inputs {
        if input.path.is_absolute() && input.path.starts_with(&prefix) && input.path.is_file() {
            inputs.insert(fs::canonicalize(&input.path).map_err(|e| io(&input.path, e))?);
        }
    }
    let inputs = inputs
        .into_iter()
        .map(|path| {
            println!("cargo:rerun-if-changed={}", path.display());
            FileIdentity::read(&path)
        })
        .collect::<Result<Vec<_>>>()?;
    let compile_definitions = probe
        .compile_groups
        .iter()
        .flat_map(|group| &group.defines)
        .map(|definition| definition.define.clone())
        .collect();
    let mut compile_options = Vec::new();
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
            compile_options.push(flag);
        }
    }
    Ok(NativeConfig {
        schema: 2,
        target: target.into(),
        prefix,
        version: parsed.version,
        features: parsed.features,
        compiler: compiler_path,
        compiler_id: compiler.id.clone().unwrap_or_default(),
        compiler_version: compiler.version.clone().unwrap_or_default(),
        include_dirs,
        system_include_dirs,
        compile_definitions,
        compile_options,
        library,
        runtime_library_dirs,
        requested_runtime_dirs,
        link_search_dirs,
        link_libraries,
        link_options,
        inputs,
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
