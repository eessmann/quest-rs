#[cfg(any(target_os = "macos", test))]
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::generate::DynError;

const EXECUTABLES: &[(&str, &str)] = &[
    ("direct", "quest-consumer-direct"),
    ("facade", "quest-consumer-facade"),
    ("wrapped", "quest-consumer-wrapped"),
    ("renamed", "quest-consumer-renamed"),
];

pub fn run(requested_work_dir: Option<PathBuf>) -> Result<(), DynError> {
    let repository = crate::generate::find_workspace_root()?;
    let work = prepare_work_directory(requested_work_dir)?;
    let package = quest_build::discover_for_tooling(work.join("native-discovery"), None)?;
    let toolchain = read_toolchain(&repository.join("rust-toolchain.toml"))?;
    write_consumer_workspace(&repository, &work, &toolchain)?;

    let manifest = work.join("Cargo.toml");
    let target = work.join("target");
    let mut lock = cargo_command(&work, &package.prefix, &package.compiler);
    lock.args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(&manifest);
    run_logged(
        &mut lock,
        &work.join("lock.log"),
        "consumer dependency locking",
    )?;

    let mut build = cargo_command(&work, &package.prefix, &package.compiler);
    build
        .args([
            "build",
            "--workspace",
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target);
    run_logged(&mut build, &work.join("build.log"), "consumer build")?;

    for (fixture, binary) in EXECUTABLES {
        let executable = target.join("debug").join(binary);
        #[cfg(target_os = "linux")]
        inspect_elf(&work, fixture, &executable)?;
        let mut execute = Command::new(&executable);
        execute.current_dir(&work);
        clean_loader_environment(&mut execute);
        run_logged(
            &mut execute,
            &work.join(format!("{fixture}-run.log")),
            &format!("{fixture} consumer execution"),
        )?;
        #[cfg(target_os = "macos")]
        inspect_macho(&work, fixture, &executable, &package)?;
        #[cfg(target_os = "linux")]
        println!("{fixture}: RUNPATH, complete native closure, and numerical check passed");
        #[cfg(target_os = "macos")]
        println!("{fixture}: LC_RPATH, installed native closure, and numerical check passed");
    }

    println!(
        "Checked QuEST {} from {} with {}.",
        package.version,
        package.prefix.display(),
        package.compiler.display()
    );
    println!(
        "Preserved consumer fixture and evidence: {}",
        work.display()
    );
    Ok(())
}

fn prepare_work_directory(requested: Option<PathBuf>) -> Result<PathBuf, DynError> {
    let path = if let Some(requested) = requested {
        let path = if requested.is_absolute() {
            requested
        } else {
            env::current_dir()?.join(requested)
        };
        if path.try_exists()? {
            if !path.is_dir() {
                return Err(
                    format!("consumer work path is not a directory: {}", path.display()).into(),
                );
            }
            if fs::read_dir(&path)?.next().transpose()?.is_some() {
                return Err(
                    format!("consumer work directory must be empty: {}", path.display()).into(),
                );
            }
        } else {
            fs::create_dir_all(&path)?;
        }
        path
    } else {
        tempfile::Builder::new()
            .prefix("quest-native-consumers-")
            .tempdir()?
            .keep()
    };
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn read_toolchain(path: &Path) -> Result<String, DynError> {
    let contents = fs::read_to_string(path)?;
    for line in contents.lines().map(str::trim) {
        let Some(value) = line.strip_prefix("channel") else {
            continue;
        };
        let Some(value) = value.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim();
        if let Some(value) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            return Ok(value.to_owned());
        }
    }
    Err(format!(
        "{} does not define a quoted toolchain channel",
        path.display()
    )
    .into())
}

fn cargo_command(work: &Path, quest_prefix: &Path, compiler: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(work);
    pin_native_environment(&mut command, quest_prefix, compiler);
    command
        .env_remove("QUEST_DIR")
        .env_remove("QuEST_ROOT")
        .env_remove("QuEST_DIR")
        .env_remove("QUEST_NATIVE_CONFIG")
        .env_remove("QUEST_RUNTIME_LIBRARY_PATH");
    clean_loader_environment(&mut command);
    if env::var_os("CARGO_BUILD_JOBS").is_none() {
        command.env("CARGO_BUILD_JOBS", "4");
    }
    command
}

fn pin_native_environment<'a>(
    command: &'a mut Command,
    quest_prefix: &Path,
    compiler: &Path,
) -> &'a mut Command {
    command.env("QUEST_ROOT", quest_prefix).env("CXX", compiler)
}

fn write_consumer_workspace(
    repository: &Path,
    work: &Path,
    toolchain: &str,
) -> Result<(), DynError> {
    let repository = repository.canonicalize()?;
    let quest = repository.join("crates/quest");
    let quest_sys = repository.join("crates/quest-sys");
    let quest_build = repository.join("crates/quest-build");
    write_file(
        &work.join("Cargo.toml"),
        "[workspace]\nresolver = \"3\"\nmembers = [\"direct\", \"facade\", \"wrapper\", \"wrapped\", \"renamed\"]\n",
    )?;
    write_file(
        &work.join("rust-toolchain.toml"),
        &format!(
            "[toolchain]\nchannel = {}\nprofile = \"minimal\"\n",
            toml_string(toolchain)?
        ),
    )?;

    let build_dependency = format!(
        "\n[build-dependencies]\nquest-build = {{ path = {} }}\n",
        toml_path(&quest_build)?
    );
    let build_script =
        "fn main() -> quest_build::Result<()> { quest_build::emit_final_target_runtime_paths() }\n";
    let quest_dependency = format!(
        "quest = {{ package = \"quest-rs\", path = {} }}\n",
        toml_path(&quest)?
    );
    let quest_sys_dependency = format!("quest-sys = {{ path = {} }}\n", toml_path(&quest_sys)?);

    write_package(
        work,
        "direct",
        &quest_sys_dependency,
        Some(build_dependency.as_str()),
        Some(build_script),
    )?;
    write_package(
        work,
        "facade",
        &quest_dependency,
        Some(build_dependency.as_str()),
        Some(build_script),
    )?;
    write_package(work, "wrapper", &quest_dependency, None, None)?;
    write_package(
        work,
        "wrapped",
        "quest-consumer-wrapper = { path = \"../wrapper\" }\n",
        Some(build_dependency.as_str()),
        Some(build_script),
    )?;
    write_package(
        work,
        "renamed",
        &format!(
            "quantum = {{ package = \"quest-rs\", path = {} }}\n",
            toml_path(&quest)?
        ),
        Some(build_dependency.as_str()),
        Some(build_script),
    )?;

    write_file(&work.join("direct/src/main.rs"), DIRECT_SOURCE)?;
    write_file(&work.join("facade/src/main.rs"), &facade_source("quest"))?;
    write_file(
        &work.join("wrapper/src/lib.rs"),
        &facade_library_source("quest"),
    )?;
    write_file(
        &work.join("wrapped/src/main.rs"),
        "fn main() -> Result<(), Box<dyn std::error::Error>> { quest_consumer_wrapper::run() }\n",
    )?;
    write_file(&work.join("renamed/src/main.rs"), &facade_source("quantum"))?;
    Ok(())
}

fn write_package(
    work: &Path,
    name: &str,
    dependencies: &str,
    build_dependencies: Option<&str>,
    build_script: Option<&str>,
) -> Result<(), DynError> {
    let manifest = format!(
        "[package]\nname = \"quest-consumer-{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n{dependencies}{}",
        build_dependencies.unwrap_or_default()
    );
    write_file(&work.join(name).join("Cargo.toml"), &manifest)?;
    if let Some(build_script) = build_script {
        write_file(&work.join(name).join("build.rs"), build_script)?;
    }
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), DynError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn toml_path(path: &Path) -> Result<String, DynError> {
    let value = path
        .to_str()
        .ok_or_else(|| format!("Cargo dependency path is not UTF-8: {}", path.display()))?;
    toml_string(value)
}

fn toml_string(value: &str) -> Result<String, DynError> {
    serde_json::to_string(value).map_err(Into::into)
}

fn facade_source(crate_name: &str) -> String {
    format!(
        "fn main() -> Result<(), Box<dyn std::error::Error>> {{\n    run()\n}}\n\n{}",
        facade_library_source(crate_name)
    )
}

fn facade_library_source(crate_name: &str) -> String {
    format!(
        r"pub fn run() -> Result<(), Box<dyn std::error::Error>> {{
    let program = {crate_name}::circuit! {{
        qubit[2] q;
        h q[0];
        cx q[0], q[1];
    }}?;
    let environment = {crate_name}::Environment::builder().build()?;
    let mut prepared = environment.prepare_structured(program)?;
    let mut register = environment.state_vector({crate_name}::QubitCount::new(2)?)?;
    prepared.run(&mut register, &{crate_name}::RunInputs::default())?;
    let state = register.snapshot()?;
    let expected = std::f64::consts::FRAC_1_SQRT_2;
    assert_eq!((state.nrows(), state.ncols()), (4, 1));
    for (row, expected_re) in [expected, 0.0, 0.0, expected].into_iter().enumerate() {{
        assert!((state[(row, 0)].re - expected_re).abs() < 1e-12);
        assert!(state[(row, 0)].im.abs() < 1e-12);
    }}
    Ok(())
}}
"
    )
}

const DIRECT_SOURCE: &str = r"fn main() -> Result<(), Box<dyn std::error::Error>> {
    quest_sys::init_custom_quest_env(false, false, false)?;
    let mut register = quest_sys::create_qureg(2)?;
    quest_sys::init_zero_state(register.pin_mut())?;
    quest_sys::apply_hadamard(register.pin_mut(), 0)?;
    quest_sys::apply_controlled_pauli_x(register.pin_mut(), 0, 1)?;
    let expected = std::f64::consts::FRAC_1_SQRT_2;
    for (index, expected_re) in [expected, 0.0, 0.0, expected].into_iter().enumerate() {
        let amplitude = quest_sys::get_qureg_amp(&register, i64::try_from(index)?)?;
        assert!((amplitude.re - expected_re).abs() < 1e-12);
        assert!(amplitude.im.abs() < 1e-12);
    }
    drop(register);
    quest_sys::finalize_quest_env()?;
    Ok(())
}
";

#[cfg(target_os = "linux")]
fn inspect_elf(work: &Path, fixture: &str, executable: &Path) -> Result<(), DynError> {
    let mut readelf = Command::new("readelf");
    readelf.args([OsStr::new("-d"), executable.as_os_str()]);
    readelf.current_dir(work);
    clean_loader_environment(&mut readelf);
    let dynamic = run_captured(
        &mut readelf,
        &work.join(format!("{fixture}-readelf.log")),
        &format!("{fixture} ELF inspection"),
    )?;
    let dynamic = String::from_utf8_lossy(&dynamic.stdout);
    if !dynamic.contains("(RUNPATH)") || dynamic.contains("(RPATH)") {
        return Err(format!(
            "{fixture} does not use the required DT_RUNPATH loader policy; inspect {}",
            work.join(format!("{fixture}-readelf.log")).display()
        )
        .into());
    }

    let mut ldd = Command::new("ldd");
    ldd.arg(executable).current_dir(work);
    clean_loader_environment(&mut ldd);
    let closure = run_captured(
        &mut ldd,
        &work.join(format!("{fixture}-ldd.log")),
        &format!("{fixture} native closure inspection"),
    )?;
    let closure = String::from_utf8_lossy(&closure.stdout);
    if closure.contains("not found") || !closure.contains("libQuEST") {
        return Err(format!(
            "{fixture} has an incomplete native closure; inspect {}",
            work.join(format!("{fixture}-ldd.log")).display()
        )
        .into());
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Debug)]
struct MachoEvidence {
    executable: PathBuf,
    rpaths: Vec<PathBuf>,
    quest_rpaths: Vec<PathBuf>,
    binary_dependencies: Vec<String>,
    quest_id: String,
    quest_dependencies: Vec<String>,
    loaded_paths: Vec<PathBuf>,
}

#[cfg(target_os = "macos")]
fn inspect_macho(
    work: &Path,
    fixture: &str,
    executable: &Path,
    package: &quest_build::NativePackage,
) -> Result<(), DynError> {
    let mut load_commands = Command::new("otool");
    load_commands.arg("-l").arg(executable).current_dir(work);
    clean_loader_environment(&mut load_commands);
    let load_commands = run_captured(
        &mut load_commands,
        &work.join(format!("{fixture}-otool-load-commands.log")),
        &format!("{fixture} Mach-O load command inspection"),
    )?;

    let mut binary_libraries = Command::new("otool");
    binary_libraries.arg("-L").arg(executable).current_dir(work);
    clean_loader_environment(&mut binary_libraries);
    let binary_libraries = run_captured(
        &mut binary_libraries,
        &work.join(format!("{fixture}-otool-libraries.log")),
        &format!("{fixture} Mach-O dependency inspection"),
    )?;

    let mut quest_id = Command::new("otool");
    quest_id.arg("-D").arg(&package.library).current_dir(work);
    clean_loader_environment(&mut quest_id);
    let quest_id = run_captured(
        &mut quest_id,
        &work.join(format!("{fixture}-quest-install-name.log")),
        &format!("{fixture} QuEST install name inspection"),
    )?;

    let mut quest_libraries = Command::new("otool");
    quest_libraries
        .arg("-L")
        .arg(&package.library)
        .current_dir(work);
    clean_loader_environment(&mut quest_libraries);
    let quest_libraries = run_captured(
        &mut quest_libraries,
        &work.join(format!("{fixture}-quest-libraries.log")),
        &format!("{fixture} QuEST dependency inspection"),
    )?;

    let mut quest_load_commands = Command::new("otool");
    quest_load_commands
        .arg("-l")
        .arg(&package.library)
        .current_dir(work);
    clean_loader_environment(&mut quest_load_commands);
    let quest_load_commands = run_captured(
        &mut quest_load_commands,
        &work.join(format!("{fixture}-quest-load-commands.log")),
        &format!("{fixture} QuEST Mach-O load command inspection"),
    )?;

    let mut diagnostic = Command::new(executable);
    diagnostic.current_dir(work);
    clean_loader_environment(&mut diagnostic);
    diagnostic.env("DYLD_PRINT_LIBRARIES", "1");
    let loaded = run_captured(
        &mut diagnostic,
        &work.join(format!("{fixture}-dyld-loaded.log")),
        &format!("{fixture} dyld native closure diagnostic"),
    )?;
    let loaded_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&loaded.stdout),
        String::from_utf8_lossy(&loaded.stderr)
    );
    let evidence = MachoEvidence {
        executable: executable.to_owned(),
        rpaths: parse_otool_rpaths(&String::from_utf8_lossy(&load_commands.stdout)),
        quest_rpaths: parse_otool_rpaths(&String::from_utf8_lossy(&quest_load_commands.stdout)),
        binary_dependencies: parse_otool_install_names(&String::from_utf8_lossy(
            &binary_libraries.stdout,
        )),
        quest_id: parse_otool_id(&String::from_utf8_lossy(&quest_id.stdout))
            .ok_or_else(|| format!("{fixture} QuEST dylib has no install name"))?,
        quest_dependencies: parse_otool_install_names(&String::from_utf8_lossy(
            &quest_libraries.stdout,
        )),
        loaded_paths: parse_dyld_loaded_paths(&loaded_text),
    };
    validate_macho_evidence(&evidence, &package.library, &package.runtime_library_dirs).map_err(
        |error| {
            format!(
                "{fixture}: {error}; inspect {}/{}-*.log",
                work.display(),
                fixture
            )
            .into()
        },
    )
}

#[cfg(any(target_os = "macos", test))]
fn parse_otool_rpaths(output: &str) -> Vec<PathBuf> {
    let mut in_rpath = false;
    let mut rpaths = Vec::new();
    for line in output.lines().map(str::trim) {
        if line.starts_with("Load command ") {
            in_rpath = false;
        } else if line == "cmd LC_RPATH" {
            in_rpath = true;
        } else if in_rpath
            && let Some(path) = line
                .strip_prefix("path ")
                .and_then(|path| path.split_once(" (offset ").map(|(path, _)| path))
        {
            rpaths.push(PathBuf::from(path));
            in_rpath = false;
        }
    }
    rpaths
}

#[cfg(any(target_os = "macos", test))]
fn parse_otool_install_names(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().split_once(" (compatibility version "))
        .map(|(name, _)| name.to_owned())
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn parse_otool_id(output: &str) -> Option<String> {
    output
        .lines()
        .nth(1)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

#[cfg(any(target_os = "macos", test))]
fn parse_dyld_loaded_paths(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .filter_map(|line| {
            line.strip_prefix("dyld[")?
                .split_once(": ")
                .map(|(_, value)| value)
        })
        .filter_map(|value| {
            let value = value.trim_start();
            let value = if value.starts_with('<') {
                value.split_once("> ")?.1
            } else {
                value.strip_prefix("loaded: ").unwrap_or(value)
            };
            let path = PathBuf::from(value);
            path.is_absolute().then_some(path)
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn validate_macho_evidence(
    evidence: &MachoEvidence,
    quest_library: &Path,
    runtime_dirs: &[PathBuf],
) -> Result<(), DynError> {
    let executable_rpaths =
        expand_rpaths(&evidence.rpaths, &evidence.executable, &evidence.executable)?;
    let quest_rpaths = expand_rpaths(&evidence.quest_rpaths, quest_library, &evidence.executable)?;
    for directory in runtime_dirs {
        let expected = directory.canonicalize()?;
        if !executable_rpaths
            .iter()
            .any(|path| path.canonicalize().is_ok_and(|path| path == expected))
        {
            return Err(format!("LC_RPATH is missing {}", directory.display()).into());
        }
    }
    if !evidence.binary_dependencies.contains(&evidence.quest_id) {
        return Err(format!(
            "executable does not reference QuEST install name {}",
            evidence.quest_id
        )
        .into());
    }
    let selected = quest_library.canonicalize()?;
    let binary_quest = resolve_install_name(
        &evidence.quest_id,
        &evidence.executable,
        &evidence.executable,
        &executable_rpaths,
    )?;
    if binary_quest != selected || !loaded_exactly(&evidence.loaded_paths, &selected) {
        return Err(format!(
            "dyld did not resolve and load selected QuEST library {}",
            quest_library.display()
        )
        .into());
    }
    if !evidence.quest_dependencies.iter().any(|name| {
        let file = Path::new(name)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        file.starts_with("libomp") || file.starts_with("libgomp") || file.starts_with("libiomp")
    }) {
        return Err("installed QuEST does not declare an indirect OpenMP dependency".into());
    }
    for dependency in &evidence.binary_dependencies {
        if is_system_install_name(dependency) {
            continue;
        }
        let expected = resolve_install_name(
            dependency,
            &evidence.executable,
            &evidence.executable,
            &executable_rpaths,
        )?;
        if !loaded_exactly(&evidence.loaded_paths, &expected) {
            return Err(format!("dyld did not load executable dependency {dependency}").into());
        }
    }
    let inherited_rpaths = executable_rpaths
        .iter()
        .chain(quest_rpaths.iter())
        .cloned()
        .collect::<Vec<_>>();
    for dependency in &evidence.quest_dependencies {
        if dependency == &evidence.quest_id || is_system_install_name(dependency) {
            continue;
        }
        let expected = resolve_install_name(
            dependency,
            quest_library,
            &evidence.executable,
            &inherited_rpaths,
        )?;
        if !loaded_exactly(&evidence.loaded_paths, &expected) {
            return Err(
                format!("dyld did not load installed QuEST dependency {dependency}").into(),
            );
        }
    }
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn expand_rpaths(
    paths: &[PathBuf],
    image: &Path,
    executable: &Path,
) -> Result<Vec<PathBuf>, DynError> {
    paths
        .iter()
        .map(|path| {
            let text = path
                .to_str()
                .ok_or_else(|| format!("non-UTF-8 Mach-O LC_RPATH: {}", path.display()))?;
            expand_load_path(text, image, executable)
        })
        .collect()
}

#[cfg(any(target_os = "macos", test))]
fn expand_load_path(reference: &str, image: &Path, executable: &Path) -> Result<PathBuf, DynError> {
    let path = if reference == "@loader_path" {
        image
            .parent()
            .ok_or_else(|| format!("Mach-O image has no parent: {}", image.display()))?
            .to_owned()
    } else if reference == "@executable_path" {
        executable
            .parent()
            .ok_or_else(|| format!("Mach-O executable has no parent: {}", executable.display()))?
            .to_owned()
    } else if let Some(relative) = reference.strip_prefix("@loader_path/") {
        image
            .parent()
            .ok_or_else(|| format!("Mach-O image has no parent: {}", image.display()))?
            .join(relative)
    } else if let Some(relative) = reference.strip_prefix("@executable_path/") {
        executable
            .parent()
            .ok_or_else(|| format!("Mach-O executable has no parent: {}", executable.display()))?
            .join(relative)
    } else if Path::new(reference).is_absolute() {
        PathBuf::from(reference)
    } else {
        return Err(format!("unsupported Mach-O load path {reference}").into());
    };
    Ok(path)
}

#[cfg(any(target_os = "macos", test))]
fn resolve_install_name(
    reference: &str,
    image: &Path,
    executable: &Path,
    rpaths: &[PathBuf],
) -> Result<PathBuf, DynError> {
    if let Some(relative) = reference.strip_prefix("@rpath/") {
        let candidates = rpaths
            .iter()
            .filter_map(|directory| directory.join(relative).canonicalize().ok())
            .collect::<BTreeSet<_>>();
        if candidates.len() > 1 {
            return Err(format!("ambiguous Mach-O install name {reference}").into());
        }
        return candidates
            .into_iter()
            .next()
            .ok_or_else(|| format!("unresolved Mach-O install name {reference}").into());
    }
    Ok(expand_load_path(reference, image, executable)?.canonicalize()?)
}

#[cfg(any(target_os = "macos", test))]
fn loaded_exactly(loaded: &[PathBuf], expected: &Path) -> bool {
    loaded
        .iter()
        .any(|path| path.canonicalize().is_ok_and(|path| path == expected))
}

#[cfg(any(target_os = "macos", test))]
fn is_system_install_name(name: &str) -> bool {
    name.starts_with("/usr/lib/") || name.starts_with("/System/Library/")
}

fn run_logged(command: &mut Command, log_path: &Path, action: &str) -> Result<(), DynError> {
    let log = File::create(log_path)?;
    let error_log = log.try_clone()?;
    let status = command
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(error_log))
        .status()?;
    if !status.success() {
        return Err(format!("{action} failed ({status}); inspect {}", log_path.display()).into());
    }
    Ok(())
}

fn run_captured(command: &mut Command, log_path: &Path, action: &str) -> Result<Output, DynError> {
    let output = command.output()?;
    let mut evidence = output.stdout.clone();
    evidence.extend_from_slice(&output.stderr);
    fs::write(log_path, evidence)?;
    if !output.status.success() {
        return Err(format!(
            "{action} failed ({}); inspect {}",
            output.status,
            log_path.display()
        )
        .into());
    }
    Ok(output)
}

fn clean_loader_environment(command: &mut Command) -> &mut Command {
    let explicit = command
        .get_envs()
        .map(|(name, _)| name.to_os_string())
        .collect::<Vec<_>>();
    for name in env::vars_os().map(|(name, _)| name).chain(explicit) {
        let text = name.to_string_lossy();
        if text.starts_with("LD_") || text.starts_with("DYLD_") {
            command.env_remove(name);
        }
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;
    use std::ffi::OsStr;
    use std::fs;
    use std::process::Command;

    #[gtest]
    fn fixture_is_a_separate_workspace_with_all_consumer_shapes() -> googletest::Result<()> {
        let temporary = tempfile::tempdir().or_fail()?;
        let fixture = temporary.path().join("consumer fixture with spaces");
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .or_fail()?;

        write_consumer_workspace(repository, &fixture, "nightly-2026-09-06").or_fail()?;

        let metadata = Command::new("cargo")
            .args([
                OsStr::new("metadata"),
                OsStr::new("--offline"),
                OsStr::new("--no-deps"),
                OsStr::new("--format-version=1"),
                OsStr::new("--manifest-path"),
            ])
            .arg(fixture.join("Cargo.toml"))
            .output()
            .or_fail()?;
        expect_that!(metadata.status.success(), eq(true));

        let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).or_fail()?;
        let packages = metadata["packages"].as_array().or_fail()?;
        let names = packages
            .iter()
            .filter_map(|package| package["name"].as_str())
            .collect::<Vec<_>>();
        expect_that!(
            names.iter().all(|name| name.starts_with("quest-consumer-")),
            eq(true)
        );
        expect_that!(names, len(eq(5)));

        let direct = fs::read_to_string(fixture.join("direct/src/main.rs")).or_fail()?;
        let facade = fs::read_to_string(fixture.join("facade/src/main.rs")).or_fail()?;
        let wrapped = fs::read_to_string(fixture.join("wrapped/Cargo.toml")).or_fail()?;
        let renamed = fs::read_to_string(fixture.join("renamed/Cargo.toml")).or_fail()?;
        expect_that!(
            direct,
            contains_substring("quest_sys::init_custom_quest_env(false, false, false)")
        );
        expect_that!(facade, contains_substring("prepare_structured"));
        expect_that!(wrapped, contains_substring("quest-consumer-wrapper"));
        verify_that!(renamed, contains_substring("package = \"quest-rs\""))
    }

    #[gtest]
    fn child_process_loader_overrides_are_removed() {
        let mut command = Command::new("true");
        command
            .env("LD_LIBRARY_PATH", "/wrong")
            .env("LD_PRELOAD", "/wrong.so")
            .env("LD_AUDIT", "/wrong-audit.so")
            .env("LD_DEBUG", "libs")
            .env("DYLD_LIBRARY_PATH", "/wrong")
            .env("DYLD_FALLBACK_LIBRARY_PATH", "/wrong")
            .env("DYLD_FRAMEWORK_PATH", "/wrong")
            .env("DYLD_INSERT_LIBRARIES", "/wrong.dylib")
            .env("DYLD_PRINT_LIBRARIES", "1");

        clean_loader_environment(&mut command);

        let entries = command.get_envs().collect::<Vec<_>>();
        for variable in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "LD_AUDIT",
            "LD_DEBUG",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "DYLD_FRAMEWORK_PATH",
            "DYLD_INSERT_LIBRARIES",
            "DYLD_PRINT_LIBRARIES",
        ] {
            expect_that!(
                entries
                    .iter()
                    .any(|(name, value)| { *name == OsStr::new(variable) && value.is_none() }),
                eq(true)
            );
        }
    }

    #[gtest]
    fn macho_parsers_extract_rpaths_install_names_and_loaded_libraries() -> googletest::Result<()> {
        let commands = "Load command 1\n          cmd LC_RPATH\n      cmdsize 56\n         path /opt/QuEST lib (offset 12)\nLoad command 2\n          cmd LC_LOAD_DYLIB\n";
        let dependencies = "/tmp/consumer:\n\t@rpath/libQuEST.dylib (compatibility version 4.3.0, current version 4.3.0)\n\t/usr/lib/libc++.1.dylib (compatibility version 1.0.0, current version 1.0.0)\n";
        let id = "/opt/QuEST/lib/libQuEST.dylib:\n@rpath/libQuEST.dylib\n";
        let loaded = "dyld[2718]: <A1B2> /opt/QuEST lib/libQuEST.dylib\ndyld[2718]: <C3D4> /opt/OpenMP/lib/libomp.dylib\n";

        expect_that!(
            parse_otool_rpaths(commands),
            elements_are![eq(&PathBuf::from("/opt/QuEST lib"))]
        );
        expect_that!(
            parse_otool_install_names(dependencies),
            elements_are![eq("@rpath/libQuEST.dylib"), eq("/usr/lib/libc++.1.dylib")]
        );
        expect_that!(parse_otool_id(id), some(eq("@rpath/libQuEST.dylib")));
        verify_that!(
            parse_dyld_loaded_paths(loaded),
            elements_are![
                eq(&PathBuf::from("/opt/QuEST lib/libQuEST.dylib")),
                eq(&PathBuf::from("/opt/OpenMP/lib/libomp.dylib"))
            ]
        )
    }

    #[gtest]
    fn bare_macho_path_anchors_resolve_to_image_directories() -> googletest::Result<()> {
        let image = Path::new("/opt/quest/lib/libQuEST.dylib");
        let executable = Path::new("/opt/consumer/bin/consumer");
        verify_that!(
            expand_rpaths(
                &[
                    PathBuf::from("@loader_path"),
                    PathBuf::from("@executable_path")
                ],
                image,
                executable,
            )
            .or_fail()?,
            elements_are![
                eq(&PathBuf::from("/opt/quest/lib")),
                eq(&PathBuf::from("/opt/consumer/bin"))
            ]
        )
    }

    #[gtest]
    fn macho_evidence_requires_selected_quest_and_indirect_openmp() -> googletest::Result<()> {
        let temporary = tempfile::tempdir().or_fail()?;
        let quest_dir = temporary.path().join("quest/lib");
        let omp_dir = temporary.path().join("omp/lib");
        fs::create_dir_all(&quest_dir).or_fail()?;
        fs::create_dir_all(&omp_dir).or_fail()?;
        let quest = quest_dir.join("libQuEST.dylib");
        let omp = omp_dir.join("libomp.dylib");
        fs::write(&quest, "quest").or_fail()?;
        fs::write(&omp, "omp").or_fail()?;
        let evidence = MachoEvidence {
            executable: temporary.path().join("consumer"),
            rpaths: vec![quest_dir.clone(), omp_dir.clone()],
            quest_rpaths: Vec::new(),
            binary_dependencies: vec!["@rpath/libQuEST.dylib".into()],
            quest_id: "@rpath/libQuEST.dylib".into(),
            quest_dependencies: vec!["@rpath/libQuEST.dylib".into(), "@rpath/libomp.dylib".into()],
            loaded_paths: vec![quest.clone(), omp.clone()],
        };
        validate_macho_evidence(&evidence, &quest, &[quest_dir.clone(), omp_dir]).or_fail()?;

        let wrong_quest = MachoEvidence {
            loaded_paths: vec![temporary.path().join("other/libQuEST.dylib"), omp],
            ..evidence.clone()
        };
        expect_that!(
            validate_macho_evidence(&wrong_quest, &quest, std::slice::from_ref(&quest_dir))
                .is_err(),
            eq(true)
        );
        let missing_omp = MachoEvidence {
            loaded_paths: vec![quest.clone()],
            ..evidence
        };
        verify_that!(
            validate_macho_evidence(&missing_omp, &quest, &[quest_dir]).is_err(),
            eq(true)
        )
    }

    #[gtest]
    fn macho_evidence_rejects_a_shadowed_absolute_openmp_install_name() -> googletest::Result<()> {
        let temporary = tempfile::tempdir().or_fail()?;
        let quest_dir = temporary.path().join("quest/lib");
        let omp_dir = temporary.path().join("omp/lib");
        let shadow_dir = temporary.path().join("shadow/lib");
        for directory in [&quest_dir, &omp_dir, &shadow_dir] {
            fs::create_dir_all(directory).or_fail()?;
        }
        let quest = quest_dir.join("libQuEST.dylib");
        let omp = omp_dir.join("libomp.dylib");
        let shadow = shadow_dir.join("libomp.dylib");
        for file in [&quest, &omp, &shadow] {
            fs::write(file, "fixture").or_fail()?;
        }
        let evidence = MachoEvidence {
            executable: temporary.path().join("consumer"),
            rpaths: vec![quest_dir.clone(), omp_dir.clone()],
            quest_rpaths: Vec::new(),
            binary_dependencies: vec!["@rpath/libQuEST.dylib".into()],
            quest_id: "@rpath/libQuEST.dylib".into(),
            quest_dependencies: vec!["@rpath/libQuEST.dylib".into(), omp.display().to_string()],
            loaded_paths: vec![quest.clone(), shadow],
        };

        verify_that!(
            validate_macho_evidence(&evidence, &quest, &[quest_dir, omp_dir]).is_err(),
            eq(true)
        )
    }

    #[gtest]
    fn macho_evidence_rejects_shadowed_rpath_and_loader_path_dependencies() -> googletest::Result<()>
    {
        let temporary = tempfile::tempdir().or_fail()?;
        let quest_dir = temporary.path().join("quest/lib");
        let omp_dir = temporary.path().join("omp/lib");
        let shadow_dir = temporary.path().join("shadow/lib");
        for directory in [&quest_dir, &omp_dir, &shadow_dir] {
            fs::create_dir_all(directory).or_fail()?;
        }
        let quest = quest_dir.join("libQuEST.dylib");
        let omp = omp_dir.join("libomp.dylib");
        let loader_omp = quest_dir.join("libomp.dylib");
        let shadow = shadow_dir.join("libomp.dylib");
        for file in [&quest, &omp, &loader_omp, &shadow] {
            fs::write(file, "fixture").or_fail()?;
        }
        let mut evidence = MachoEvidence {
            executable: temporary.path().join("consumer"),
            rpaths: vec![quest_dir.clone(), omp_dir.clone()],
            quest_rpaths: Vec::new(),
            binary_dependencies: vec!["@rpath/libQuEST.dylib".into()],
            quest_id: "@rpath/libQuEST.dylib".into(),
            quest_dependencies: vec!["@rpath/libQuEST.dylib".into(), "@rpath/libomp.dylib".into()],
            loaded_paths: vec![quest.clone(), shadow],
        };
        expect_that!(
            validate_macho_evidence(&evidence, &quest, &[quest_dir.clone(), omp_dir.clone()])
                .is_err(),
            eq(true)
        );

        evidence.quest_dependencies[1] = "@loader_path/libomp.dylib".into();
        verify_that!(
            validate_macho_evidence(&evidence, &quest, &[quest_dir, omp_dir]).is_err(),
            eq(true)
        )
    }

    #[gtest]
    fn macho_evidence_rejects_ambiguous_rpath_resolution() -> googletest::Result<()> {
        let temporary = tempfile::tempdir().or_fail()?;
        let quest_dir = temporary.path().join("quest/lib");
        let omp_dir = temporary.path().join("omp/lib");
        let shadow_dir = temporary.path().join("shadow/lib");
        for directory in [&quest_dir, &omp_dir, &shadow_dir] {
            fs::create_dir_all(directory).or_fail()?;
        }
        let quest = quest_dir.join("libQuEST.dylib");
        let omp = omp_dir.join("libomp.dylib");
        let shadow = shadow_dir.join("libomp.dylib");
        for file in [&quest, &omp, &shadow] {
            fs::write(file, "fixture").or_fail()?;
        }
        let evidence = MachoEvidence {
            executable: temporary.path().join("consumer"),
            rpaths: vec![quest_dir.clone(), omp_dir.clone(), shadow_dir],
            quest_rpaths: Vec::new(),
            binary_dependencies: vec!["@rpath/libQuEST.dylib".into()],
            quest_id: "@rpath/libQuEST.dylib".into(),
            quest_dependencies: vec!["@rpath/libQuEST.dylib".into(), "@rpath/libomp.dylib".into()],
            loaded_paths: vec![quest.clone(), omp],
        };
        verify_that!(
            validate_macho_evidence(&evidence, &quest, &[quest_dir, omp_dir]).is_err(),
            eq(true)
        )
    }

    #[gtest]
    fn native_pin_preserves_dependency_prefix_search_paths() {
        let mut command = Command::new("true");
        command.env("CMAKE_PREFIX_PATH", "/opt/mpi;/opt/other dependency");

        pin_native_environment(
            &mut command,
            Path::new("/opt/quest"),
            Path::new("/usr/bin/c++"),
        );

        expect_that!(
            command.get_envs().any(|(name, value)| {
                name == OsStr::new("CMAKE_PREFIX_PATH")
                    && value == Some(OsStr::new("/opt/mpi;/opt/other dependency"))
            }),
            eq(true)
        );
    }

    #[gtest]
    fn consumer_build_uses_selected_cargo_without_a_rustup_toolchain_argument()
    -> googletest::Result<()> {
        let temporary = tempfile::tempdir().or_fail()?;
        let mut command = cargo_command(
            temporary.path(),
            Path::new("/opt/quest"),
            Path::new("/opt/clang++"),
        );
        command.arg("build");
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        expect_that!(command.get_program(), eq(OsStr::new("cargo")));
        verify_that!(arguments, elements_are![eq("build")])
    }

    #[gtest]
    fn explicit_nonempty_work_directory_is_rejected_without_overwriting_it()
    -> googletest::Result<()> {
        let temporary = tempfile::tempdir().or_fail()?;
        let marker = temporary.path().join("keep.txt");
        fs::write(&marker, "owned by caller").or_fail()?;

        let result = prepare_work_directory(Some(temporary.path().to_owned()));

        expect_that!(result, err(anything()));
        verify_that!(fs::read_to_string(marker).or_fail()?, eq("owned by caller"))
    }
}
