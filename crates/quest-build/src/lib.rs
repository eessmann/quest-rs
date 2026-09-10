#![forbid(unsafe_code)]

//! Shared installed-QuEST configuration for native bridges and final executables.
//!
//! The verified development recipe currently supports native Linux GNU targets.
//! It uses executable `DT_RPATH` (not `DT_RUNPATH`) so the recorded directories
//! also participate in resolving indirect dependencies such as cuQuantum's CUDA
//! dependencies. This is an absolute-path development policy, not a relocatable
//! distribution format.

mod config;
mod probe;

pub use config::{FileIdentity, NativeConfig};

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
        "native QuEST discovery supports native Linux GNU targets only; host={host}, target={target}; cross compilation requires a separately implemented target toolchain and loader recipe"
    )]
    UnsupportedTarget { host: String, target: String },
    #[error("{program} failed ({status}):\n{output}")]
    Command {
        program: String,
        status: String,
        output: String,
    },
    #[error("invalid native configuration JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("CMake File API: {0}")]
    CmakeFileApi(String),
}

pub type Result<T> = std::result::Result<T, BuildError>;

pub(crate) const QUEST_ENV_VARS: &[&str] = &["QUEST_DIR", "QUEST_ROOT", "QuEST_DIR", "QuEST_ROOT"];

/// Load and validate `QUEST_NATIVE_CONFIG`, or discover an installation.
///
/// Discovery uses the current Cargo target. It is convenient but is not a shared
/// attestation across different build scripts; use a recorded configuration for
/// consumers.
///
/// # Errors
///
/// Returns an error when Cargo's target context is missing or unsupported, or
/// when the recorded or discovered native configuration is invalid.
pub fn discover_from_env() -> Result<NativeConfig> {
    watch_environment();
    let host = env::var("HOST")
        .map_err(|_| invalid("HOST is missing; call this from a Cargo build script"))?;
    let target = env::var("TARGET")
        .map_err(|_| invalid("TARGET is missing; call this from a Cargo build script"))?;
    validate_target(&host, &target)?;
    if let Some(path) = env::var_os("QUEST_NATIVE_CONFIG") {
        let path = PathBuf::from(path);
        println!("cargo:rerun-if-changed={}", path.display());
        return NativeConfig::load(&path, &target);
    }
    let out = env::var_os("OUT_DIR").ok_or_else(|| invalid("OUT_DIR is missing"))?;
    probe::discover(&PathBuf::from(out), &host, &target)
}

/// Emit recorded native loader paths in a final executable's build script.
///
/// Add `quest-build` as a build dependency there, even when `quest` is reached
/// through another library. No host build of `quest-sys` is involved.
///
/// # Errors
///
/// Returns an error when native configuration discovery or runtime-path
/// validation fails.
pub fn emit_final_target_runtime_paths() -> Result<()> {
    let configuration = discover_from_env()?;
    configuration.emit_runtime_paths()
}

/// Discover and verify an installation, then write a shared native record.
///
/// The optional `target` defaults to the active rustc host. Cross-target
/// discovery fails before probing, instead of accidentally recording a host
/// installation.
///
/// # Errors
///
/// Returns an error when the Rust host cannot be identified, the target is
/// unsupported, native discovery or its probe fails, or the record cannot be
/// written.
pub fn configure_native(
    output_path: impl AsRef<Path>,
    target: Option<&str>,
) -> Result<NativeConfig> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = run(Command::new(rustc).arg("-vV"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let host = text
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| invalid("rustc -vV did not identify its host"))?;
    let target = target.unwrap_or(host);
    validate_target(host, target)?;
    let output_path = absolute(output_path.as_ref())?;
    let parent = output_path
        .parent()
        .ok_or_else(|| invalid("native record requires a parent directory"))?;
    fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
    let config = probe::discover(&parent.join("quest-native-config-work"), host, target)?;
    config.write(&output_path)?;
    Ok(config)
}

fn watch_environment() {
    for name in QUEST_ENV_VARS.iter().copied().chain([
        "QUEST_NATIVE_CONFIG",
        "QUEST_RUNTIME_LIBRARY_PATH",
        "CMAKE_PREFIX_PATH",
        "CMAKE",
        "CMAKE_GENERATOR",
        "CMAKE_TOOLCHAIN_FILE",
        "CXX",
        "CXXFLAGS",
        "CC",
        "CFLAGS",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "LD_AUDIT",
    ]) {
        println!("cargo:rerun-if-env-changed={name}");
    }
}

pub(crate) fn validate_target(host: &str, target: &str) -> Result<()> {
    if host != target || !target.ends_with("-linux-gnu") {
        return Err(BuildError::UnsupportedTarget {
            host: host.into(),
            target: target.into(),
        });
    }
    Ok(())
}

// cc-rs appends all CXXFLAGS variants after build-script flags, while the C++
// driver itself reads header/tool search overrides. The initial recipe admits
// none of these: otherwise loading a record could compile a different bridge.
fn compiler_override_variables(target: &str) -> Vec<String> {
    let target_underscores = target.replace(['-', '.'], "_");
    let mut names = Vec::new();
    for base in ["CXX", "CXXFLAGS", "CXXSTDLIB"] {
        names.extend([
            format!("{base}_{target}"),
            format!("{base}_{target_underscores}"),
            format!("HOST_{base}"),
            format!("TARGET_{base}"),
        ]);
        // A global CXX selects the compiler when configuring a record. Loading
        // that record separately verifies that any current CXX still agrees.
        if base != "CXX" {
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
            "SDKROOT",
            "CRATE_CC_NO_DEFAULTS",
            "CMAKE_TOOLCHAIN_FILE",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    names
}

fn reject_compiler_overrides(
    target: &str,
    mut lookup: impl FnMut(&str) -> Option<OsString>,
) -> Result<()> {
    for name in compiler_override_variables(target) {
        if lookup(&name).is_some_and(|value| !value.is_empty()) {
            return Err(invalid(format!(
                "{name} overrides the recorded C++ configuration; unset it for the supported native recipe"
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
            .ok_or_else(|| invalid("CXX must name the recorded compiler executable"))?;
        if resolved != compiler {
            return Err(invalid(
                "CXX differs from the compiler selected by QUEST_NATIVE_CONFIG; unset it or configure a new record",
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct HeaderConfiguration {
    version: String,
    features: BTreeMap<String, String>,
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
    Ok(HeaderConfiguration {
        version: format!("4.3.{patch}"),
        features,
    })
}

/// Link arguments for the supported absolute-path development loader policy.
///
/// # Errors
///
/// Returns an error for unsupported targets, relative or non-UTF-8 paths, or
/// paths containing characters that cannot be represented safely.
pub fn runtime_link_args(target_os: &str, directories: &[PathBuf]) -> Result<Vec<String>> {
    if target_os != "linux" {
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
    Ok(vec![
        "-Wl,--disable-new-dtags".into(),
        format!("-Wl,-rpath,{}", paths.join(":")),
    ])
}

pub(crate) fn explicit_prefix() -> Result<Option<PathBuf>> {
    let mut found = None;
    for name in QUEST_ENV_VARS {
        let Some(candidate) = env::var_os(name) else {
            continue;
        };
        let prefix = normalize_quest_root(Path::new(&candidate))?;
        if found.as_ref().is_some_and(|earlier| earlier != &prefix) {
            return Err(invalid(
                "the explicit QUEST_ROOT/QUEST_DIR aliases select different installations",
            ));
        }
        found = Some(prefix);
    }
    Ok(found)
}

fn normalize_quest_root(candidate: &Path) -> Result<PathBuf> {
    let candidate = absolute(candidate)?;
    let prefix = candidate
        .ancestors()
        .find(|path| path.join("include/quest.h").is_file())
        .ok_or_else(|| {
            invalid(format!(
                "{} does not identify an installed QuEST prefix",
                candidate.display()
            ))
        })?;
    fs::canonicalize(prefix).map_err(|e| io(prefix, e))
}

pub(crate) fn requested_runtime_dirs() -> Result<Vec<PathBuf>> {
    env::var_os("QUEST_RUNTIME_LIBRARY_PATH")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| fs::canonicalize(&path).map_err(|e| io(&path, e)))
        .collect()
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

pub(crate) fn clean_loader_environment(command: &mut Command) -> &mut Command {
    command
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_PRELOAD")
        .env_remove("LD_AUDIT")
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
    fn linux_paths_use_transitive_rpath_and_reject_unrepresentable_paths() -> googletest::Result<()>
    {
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
                eq("-Wl,--disable-new-dtags"),
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
    fn rejects_unrecorded_compiler_flags_and_header_search_overrides() -> googletest::Result<()> {
        let target = "x86_64-unknown-linux-gnu";
        for name in [
            "CXXFLAGS",
            "HOST_CXXFLAGS",
            "TARGET_CXXFLAGS",
            "CXXFLAGS_x86_64-unknown-linux-gnu",
            "CXXFLAGS_x86_64_unknown_linux_gnu",
            "CXXSTDLIB",
            "HOST_CXX",
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
    fn identities_detect_changed_native_bytes() -> googletest::Result<()> {
        let directory = tempfile::tempdir().or_fail()?;
        let file = directory.path().join("native-library");
        fs::write(&file, b"native version one").or_fail()?;
        let identity = FileIdentity::read(&file).or_fail()?;
        expect_eq!(
            identity.sha256,
            "f94fcfd9a0df90089a64afbd3f743c9b4f04a1164e9ae5c883eb3f88526e9820"
        );
        identity.validate().or_fail()?;
        fs::write(&file, b"native version two").or_fail()?;
        verify_that!(identity.validate().is_err(), eq(true))
    }

    #[cfg(unix)]
    #[gtest]
    fn identities_detect_retargeted_loader_symlinks_even_with_equal_bytes() -> googletest::Result<()>
    {
        let directory = tempfile::tempdir().or_fail()?;
        let first = directory.path().join("library-one");
        let second = directory.path().join("library-two");
        let alias = directory.path().join("libnative.so.1");
        fs::write(&first, b"identical bytes").or_fail()?;
        fs::write(&second, b"identical bytes").or_fail()?;
        std::os::unix::fs::symlink(&first, &alias).or_fail()?;
        let identity = FileIdentity::read(&alias).or_fail()?;
        fs::remove_file(&alias).or_fail()?;
        std::os::unix::fs::symlink(&second, &alias).or_fail()?;
        verify_that!(identity.validate().is_err(), eq(true))
    }

    #[gtest]
    fn normalizes_installed_package_roots_in_lib_and_lib64() -> googletest::Result<()> {
        let directory = tempfile::tempdir().or_fail()?;
        fs::create_dir_all(directory.path().join("include")).or_fail()?;
        fs::write(directory.path().join("include/quest.h"), "").or_fail()?;
        for lib in ["lib", "lib64"] {
            let package = directory.path().join(lib).join("cmake/QuEST");
            fs::create_dir_all(&package).or_fail()?;
            expect_that!(
                normalize_quest_root(&package).or_fail()?,
                eq(&directory.path().canonicalize().or_fail()?)
            );
        }
        Ok(())
    }
}
