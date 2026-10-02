#![forbid(unsafe_code)]

//! CMake-owned native bridge compilation and evaluated installed-QuEST metadata.
//!
//! Final executables emit direct dependency RUNPATHs. Installed native libraries
//! remain responsible for the runtime paths of their own dependencies.

mod hdf5;
mod probe;
pub use hdf5::emit_serial_hdf5_runtime_paths;

mod package;
mod rsmpi;
pub use rsmpi::verify_rsmpi_compatibility;

pub use package::{BridgeInputs, HeaderContext, NativePackage};

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A failure to discover, validate, or use an installed native configuration.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("cannot access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid native configuration: {0}")]
    InvalidConfiguration(String),
    #[error(
        "native QuEST discovery supports native Linux GNU and Apple Darwin targets only; host={host}, target={target}; cross compilation requires a separately implemented target toolchain and loader recipe"
    )]
    UnsupportedTarget { host: String, target: String },
    #[error("{program} failed ({status}):\n{output}")]
    Command {
        program: String,
        status: String,
        output: String,
    },
    #[error("CMake File API: {0}")]
    CmakeFileApi(String),
    #[error("CMake bridge build: {0}")]
    CmakeBuild(String),
}

pub type Result<T> = std::result::Result<T, BuildError>;

pub(crate) const QUEST_ENV_VARS: &[&str] = &["QUEST_ROOT"];
const REMOVED_QUEST_ENV_VARS: &[&str] = &["QUEST_DIR", "QuEST_DIR", "QuEST_ROOT"];

/// Discover installed `QuEST` in a Cargo build script's target and profile.
///
/// # Errors
/// Returns an error for missing Cargo context, unsupported overrides or packages.
pub fn discover_from_env() -> Result<NativePackage> {
    let (work, host, target) = cargo_context()?;
    probe::discover(&work, &host, &target, None)
}

/// Compile generated CXX and wrapper sources through the imported `CMake` target.
///
/// # Errors
/// Returns an error when discovery, ABI validation or `CMake` compilation fails.
pub fn build_bridge(inputs: &BridgeInputs) -> Result<NativePackage> {
    let (work, host, target) = cargo_context()?;
    probe::discover(&work, &host, &target, Some(inputs))
}

fn cargo_context() -> Result<(PathBuf, String, String)> {
    watch_environment();
    let host = env::var("HOST").map_err(|_| {
        invalid("HOST is missing; use discover_for_tooling outside Cargo build scripts")
    })?;
    let target = env::var("TARGET").map_err(|_| {
        invalid("TARGET is missing; use discover_for_tooling outside Cargo build scripts")
    })?;
    let work = env::var_os("OUT_DIR").ok_or_else(|| invalid("OUT_DIR is missing"))?;
    Ok((PathBuf::from(work).join("quest-native"), host, target))
}

/// Discover an installation for tooling without Cargo build-script variables.
///
/// # Errors
/// Returns an error for unsupported targets or failed package/ABI discovery.
pub fn discover_for_tooling(
    work_directory: impl AsRef<Path>,
    target: Option<&str>,
) -> Result<NativePackage> {
    watch_environment();
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = run(Command::new(rustc).arg("-vV"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let host = text
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| invalid("rustc -vV did not identify its host"))?;
    probe::discover(
        &absolute(work_directory.as_ref())?,
        host,
        target.unwrap_or(host),
        None,
    )
}

/// Emit evaluated final-link options and direct library RUNPATHs.
///
/// Call this from each final executable package's build script: Cargo does not
/// propagate non-library link arguments through Rust library dependencies.
///
/// # Errors
/// Returns an error when discovery or link argument validation fails.
pub fn emit_final_target_runtime_paths() -> Result<()> {
    discover_from_env()?.emit_runtime_paths()
}

pub(crate) fn reject_obsolete_environment(
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) -> Result<()> {
    for name in REMOVED_QUEST_ENV_VARS
        .iter()
        .copied()
        .chain(["QUEST_NATIVE_CONFIG", "QUEST_RUNTIME_LIBRARY_PATH"])
    {
        if lookup(name).is_some() {
            return Err(invalid(format!(
                "{name} is obsolete; unset it, select installed QuEST with QUEST_ROOT or CMAKE_PREFIX_PATH, and give native libraries RUNPATHs for their own dependencies"
            )));
        }
    }
    Ok(())
}

fn watch_environment() {
    for name in QUEST_ENV_VARS
        .iter()
        .chain(REMOVED_QUEST_ENV_VARS)
        .copied()
        .chain([
            "QUEST_NATIVE_CONFIG",
            "QUEST_RUNTIME_LIBRARY_PATH",
            "CMAKE_PREFIX_PATH",
            "CMAKE",
            "CMAKE_GENERATOR",
            "CMAKE_TOOLCHAIN_FILE",
            // CMake package dependencies and compiler lookup share this environment
            // across discovery, bridge compilation, and final-target build scripts.
            "CUDAToolkit_ROOT",
            "CUDATOOLKIT_ROOT",
            "CUDA_PATH",
            "CUQUANTUM_ROOT",
            "PATH",
            "CXX",
            "CXXFLAGS",
            "CC",
            "CFLAGS",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "LD_AUDIT",
            "SDKROOT",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "DYLD_INSERT_LIBRARIES",
        ])
    {
        println!("cargo:rerun-if-env-changed={name}");
    }
}

pub(crate) fn validate_target(host: &str, target: &str) -> Result<()> {
    if host != target
        || !(target.ends_with("-linux-gnu")
            || matches!(target, "aarch64-apple-darwin" | "x86_64-apple-darwin"))
    {
        return Err(BuildError::UnsupportedTarget {
            host: host.into(),
            target: target.into(),
        });
    }
    Ok(())
}

// Compiler and header-search overrides can make discovery and bridge compilation
// disagree. Admit the single global CXX compiler selection, and reject other
// overrides until their interaction with both CMake entry points is supported.
fn compiler_override_variables(target: &str) -> Vec<String> {
    let target_underscores = target.replace(['-', '.'], "_");
    let mut names = Vec::new();
    for base in [
        "CXX",
        "CXXFLAGS",
        "CXXSTDLIB",
        "CMAKE_TOOLCHAIN_FILE",
        "CMAKE_PREFIX_PATH",
    ] {
        names.extend([
            format!("{base}_{target}"),
            format!("{base}_{target_underscores}"),
            format!("HOST_{base}"),
            format!("TARGET_{base}"),
        ]);
        // Global CXX and prefix paths are shared by both discovery entry points.
        if !matches!(base, "CXX" | "CMAKE_PREFIX_PATH") {
            names.push(base.into());
        }
    }
    names.extend(
        [
            "CPATH",
            "CPLUS_INCLUDE_PATH",
            "C_INCLUDE_PATH",
            "OBJC_INCLUDE_PATH",
            "GCC_EXEC_PREFIX",
            "COMPILER_PATH",
            "LIBRARY_PATH",
            "CRATE_CC_NO_DEFAULTS",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    if !target.ends_with("-apple-darwin") {
        names.push("SDKROOT".into());
    }
    names
}

fn reject_compiler_overrides(
    target: &str,
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) -> Result<()> {
    for name in compiler_override_variables(target) {
        if lookup(&name).is_some_and(|value| !value.is_empty()) {
            return Err(invalid(format!(
                "{name} overrides the CMake C++ configuration; unset it for the supported native recipe"
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_compiler_environment(
    target: &str,
    selected_compiler: Option<&Path>,
) -> Result<()> {
    for name in compiler_override_variables(target) {
        println!("cargo:rerun-if-env-changed={name}");
    }
    reject_obsolete_environment(|name| env::var_os(name))?;
    reject_compiler_overrides(target, |name| env::var_os(name))?;
    if let (Some(compiler), Some(requested)) = (selected_compiler, env::var_os("CXX")) {
        let requested = PathBuf::from(requested);
        let resolved = if requested.components().count() == 1 {
            env::var_os("PATH").and_then(|paths| {
                env::split_paths(&paths)
                    .map(|directory| directory.join(&requested))
                    .find(|candidate| candidate.is_file())
            })
        } else {
            Some(requested)
        };
        let resolved = resolved
            .and_then(|path| fs::canonicalize(path).ok())
            .ok_or_else(|| invalid("CXX must name a compiler executable"))?;
        if resolved != compiler {
            return Err(invalid(
                "CXX differs from the C++ compiler selected by CMake; use a fresh build directory",
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct HeaderConfiguration {
    version: String,
    mpi_enabled: bool,
    subcommunicators_enabled: bool,
}

pub(crate) fn parse_header_configuration(header: &str) -> Result<HeaderConfiguration> {
    let mut features = BTreeMap::new();
    for line in header.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("#define") {
            continue;
        }
        let (Some(name), Some(value)) = (fields.next(), fields.next()) else {
            continue;
        };
        if name.starts_with("QUEST_") {
            features.insert(name.to_owned(), value.to_owned());
        }
    }
    let value = |name: &str| {
        features
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| invalid(format!("installed config.h lacks {name}")))
    };
    if value("QUEST_VERSION_MAJOR")? != "4" || value("QUEST_VERSION_MINOR")? != "3" {
        return Err(invalid("only the reviewed QuEST 4.3.x API is supported"));
    }
    if value("QUEST_FLOAT_PRECISION")? != "2" {
        return Err(invalid("QuEST must use binary64 (QUEST_FLOAT_PRECISION=2)"));
    }
    if value("QUEST_INCLUDE_DEPRECATED_FUNCTIONS")? != "0" {
        return Err(invalid(
            "QuEST must disable deprecated APIs (QUEST_INCLUDE_DEPRECATED_FUNCTIONS=0)",
        ));
    }
    let patch = value("QUEST_VERSION_PATCH")?;
    patch
        .parse::<u32>()
        .map_err(|_| invalid("invalid QUEST_VERSION_PATCH"))?;
    let flag = |name: &str| match features.get(name).map(String::as_str) {
        None | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(_) => Err(invalid(format!("invalid native feature flag {name}"))),
    };
    let mpi_enabled = flag("QUEST_COMPILE_MPI")?;
    let subcommunicators_enabled = flag("QUEST_COMPILE_SUBCOMM")?;
    if subcommunicators_enabled && !mpi_enabled {
        return Err(invalid("native subcommunicator support requires MPI"));
    }
    Ok(HeaderConfiguration {
        version: format!("4.3.{patch}"),
        mpi_enabled,
        subcommunicators_enabled,
    })
}

/// Link arguments for the supported absolute-path development loader policy.
///
/// # Errors
///
/// Returns an error for unsupported targets, relative or non-UTF-8 paths, or
/// paths containing characters that cannot be represented safely.
pub fn runtime_link_args(target_os: &str, directories: &[PathBuf]) -> Result<Vec<String>> {
    if !matches!(target_os, "linux" | "macos") {
        return Err(invalid(format!(
            "no verified runtime loader recipe for {target_os}"
        )));
    }
    if directories.is_empty() {
        return Ok(Vec::new());
    }
    let paths = directories
        .iter()
        .map(|path| {
            let text = path
                .to_str()
                .ok_or_else(|| invalid("runtime paths must be UTF-8"))?;
            if !path.is_absolute() || text.contains([':', ',', '\n', '\r']) {
                return Err(invalid(format!(
                    "runtime path cannot be represented safely: {}",
                    path.display()
                )));
            }
            Ok(text)
        })
        .collect::<Result<Vec<_>>>()?;
    if target_os == "macos" {
        Ok(paths
            .into_iter()
            .map(|path| format!("-Wl,-rpath,{path}"))
            .collect())
    } else {
        Ok(vec![
            "-Wl,--enable-new-dtags".into(),
            format!("-Wl,-rpath,{}", paths.join(":")),
        ])
    }
}

pub(crate) fn explicit_prefix() -> Result<Option<PathBuf>> {
    env::var_os("QUEST_ROOT")
        .map(|candidate| installation_prefix(Path::new(&candidate)))
        .transpose()
}

fn installation_prefix(candidate: &Path) -> Result<PathBuf> {
    let prefix = absolute(candidate)?;
    if !prefix.join("include/quest.h").is_file() {
        return Err(invalid(format!(
            "QUEST_ROOT={} must name the exact installed QuEST prefix containing include/quest.h; package subdirectories are not prefixes",
            prefix.display()
        )));
    }
    fs::canonicalize(&prefix).map_err(|error| io(&prefix, error))
}

pub(crate) fn run(command: &mut Command) -> Result<Output> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command.output().map_err(|e| io(Path::new(&program), e))?;
    if !output.status.success() {
        return Err(BuildError::Command {
            program,
            status: output.status.to_string(),
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(output)
}

pub(crate) fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(env::current_dir()
            .map_err(|e| io(Path::new("."), e))?
            .join(path))
    }
}
pub(crate) fn io(path: &Path, source: std::io::Error) -> BuildError {
    BuildError::Io {
        path: path.to_owned(),
        source,
    }
}
pub(crate) fn invalid(message: impl Into<String>) -> BuildError {
    BuildError::InvalidConfiguration(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    fn header(precision: u8, deprecated: u8, minor: u8) -> String {
        format!(
            "#define QUEST_VERSION_MAJOR 4\n#define QUEST_VERSION_MINOR {minor}\n#define QUEST_VERSION_PATCH 2\n#define QUEST_FLOAT_PRECISION {precision}\n#define QUEST_INCLUDE_DEPRECATED_FUNCTIONS {deprecated}\n#define QUEST_COMPILE_OMP 1\n"
        )
    }

    #[gtest]
    fn admits_only_reviewed_version_precision_and_api_configuration() -> googletest::Result<()> {
        let parsed = parse_header_configuration(&header(2, 0, 3)).or_fail()?;
        expect_that!(parsed.version, eq("4.3.2"));
        expect_that!(
            parse_header_configuration(&header(1, 0, 3)).is_err(),
            eq(true)
        );
        expect_that!(
            parse_header_configuration(&header(2, 1, 3)).is_err(),
            eq(true)
        );
        expect_that!(
            parse_header_configuration(&header(2, 0, 4)).is_err(),
            eq(true)
        );
        Ok(())
    }

    #[gtest]
    fn native_mpi_api_requires_both_mpi_and_subcommunicators() -> googletest::Result<()> {
        for (mpi, subcomm, expected) in [(0, 0, false), (1, 0, false), (1, 1, true)] {
            let text = format!(
                "{}#define QUEST_COMPILE_MPI {mpi}\n#define QUEST_COMPILE_SUBCOMM {subcomm}\n",
                header(2, 0, 3)
            );
            let configuration = parse_header_configuration(&text).or_fail()?;
            verify_that!(
                configuration.mpi_enabled && configuration.subcommunicators_enabled,
                eq(expected)
            )?;
        }
        for (mpi, subcomm) in [(0, 1), (2, 0)] {
            let text = format!(
                "{}#define QUEST_COMPILE_MPI {mpi}\n#define QUEST_COMPILE_SUBCOMM {subcomm}\n",
                header(2, 0, 3)
            );
            verify_that!(parse_header_configuration(&text).is_err(), eq(true))?;
        }
        Ok(())
    }

    #[gtest]
    fn linux_paths_use_direct_runpath_and_reject_unrepresentable_paths() -> googletest::Result<()> {
        let args = runtime_link_args(
            "linux",
            &[
                PathBuf::from("/opt/quest/lib"),
                PathBuf::from("/opt/cuda/lib"),
            ],
        )
        .or_fail()?;
        expect_that!(
            args,
            elements_are![
                eq("-Wl,--enable-new-dtags"),
                eq("-Wl,-rpath,/opt/quest/lib:/opt/cuda/lib")
            ]
        );
        expect_that!(
            runtime_link_args("linux", &[PathBuf::from("/opt/ambiguous:path")]).is_err(),
            eq(true)
        );
        expect_that!(
            runtime_link_args("windows", &[PathBuf::from("C:/quest")]).is_err(),
            eq(true)
        );
        Ok(())
    }

    #[gtest]
    fn cross_target_is_rejected_before_host_discovery() -> googletest::Result<()> {
        expect_that!(
            validate_target("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu").is_err(),
            eq(true)
        );
        verify_that!(
            validate_target("x86_64-unknown-linux-gnu", "x86_64-unknown-linux-gnu").is_ok(),
            eq(true)
        )
    }

    #[gtest]
    fn rejects_compiler_flags_and_header_search_overrides() -> googletest::Result<()> {
        let target = "x86_64-unknown-linux-gnu";
        for name in [
            "CXXFLAGS",
            "HOST_CXXFLAGS",
            "TARGET_CXXFLAGS",
            "CXXFLAGS_x86_64-unknown-linux-gnu",
            "CXXFLAGS_x86_64_unknown_linux_gnu",
            "CXXSTDLIB",
            "HOST_CXX",
            "HOST_CMAKE_TOOLCHAIN_FILE",
            "CMAKE_TOOLCHAIN_FILE_x86_64_unknown_linux_gnu",
            "HOST_CMAKE_PREFIX_PATH",
            "CPATH",
            "CPLUS_INCLUDE_PATH",
            "GCC_EXEC_PREFIX",
            "COMPILER_PATH",
            "CRATE_CC_NO_DEFAULTS",
        ] {
            let error = reject_compiler_overrides(target, |candidate| {
                (candidate == name).then(|| "override".into())
            })
            .unwrap_err();
            expect_that!(error.to_string(), contains_substring(name));
        }
        reject_compiler_overrides(target, |_| None).or_fail()?;
        reject_compiler_overrides(target, |name| (name == "CXX").then(|| "c++".into()))
            .or_fail()?;
        Ok(())
    }

    #[gtest]
    fn obsolete_overrides_fail_with_migration_guidance() -> googletest::Result<()> {
        for name in [
            "QUEST_DIR",
            "QuEST_DIR",
            "QuEST_ROOT",
            "QUEST_NATIVE_CONFIG",
            "QUEST_RUNTIME_LIBRARY_PATH",
        ] {
            let result = reject_obsolete_environment(|key| (key == name).then(|| "".into()));
            let error = result
                .err()
                .ok_or_else(|| invalid("obsolete override admitted"))
                .or_fail()?;
            expect_that!(error.to_string(), contains_substring(name));
            expect_that!(error.to_string(), contains_substring("unset it"));
        }
        reject_obsolete_environment(|_| None).or_fail()?;
        Ok(())
    }

    #[gtest]
    fn explicit_installation_selection_rejects_package_subdirectories() -> googletest::Result<()> {
        let directory = tempfile::tempdir().or_fail()?;
        fs::create_dir_all(directory.path().join("include")).or_fail()?;
        fs::write(directory.path().join("include/quest.h"), "").or_fail()?;
        expect_that!(
            installation_prefix(directory.path()).or_fail()?,
            eq(&directory.path().canonicalize().or_fail()?)
        );
        for lib in ["lib", "lib64"] {
            let package = directory.path().join(lib).join("cmake/QuEST");
            fs::create_dir_all(&package).or_fail()?;
            expect_that!(installation_prefix(&package).is_err(), eq(true));
        }
        Ok(())
    }
    #[gtest]
    fn native_darwin_targets_and_individual_rpaths_are_supported() -> googletest::Result<()> {
        for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
            expect_true!(validate_target(target, target).is_ok());
        }
        expect_true!(validate_target("aarch64-apple-darwin", "x86_64-apple-darwin").is_err());
        expect_true!(
            validate_target("x86_64-unknown-linux-musl", "x86_64-unknown-linux-musl").is_err()
        );
        expect_eq!(
            runtime_link_args(
                "macos",
                &[
                    PathBuf::from("/opt/quest/lib"),
                    PathBuf::from("/opt/omp/lib")
                ]
            )?,
            vec!["-Wl,-rpath,/opt/quest/lib", "-Wl,-rpath,/opt/omp/lib"]
        );
        Ok(())
    }
}
