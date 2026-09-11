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
    let mut lock = cargo_command(&toolchain, &work, &package);
    lock.args(["generate-lockfile", "--offline", "--manifest-path"])
        .arg(&manifest);
    run_logged(
        &mut lock,
        &work.join("lock.log"),
        "consumer dependency locking",
    )?;

    let mut build = cargo_command(&toolchain, &work, &package);
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
        inspect_elf(&work, fixture, &executable)?;
        let mut execute = Command::new(&executable);
        execute.current_dir(&work);
        clean_loader_environment(&mut execute);
        run_logged(
            &mut execute,
            &work.join(format!("{fixture}-run.log")),
            &format!("{fixture} consumer execution"),
        )?;
        println!("{fixture}: RUNPATH, complete native closure, and numerical check passed");
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

fn cargo_command(toolchain: &str, work: &Path, package: &quest_build::NativePackage) -> Command {
    let mut command = Command::new("cargo");
    command.arg(format!("+{toolchain}")).current_dir(work);
    pin_native_environment(&mut command, &package.prefix, &package.compiler);
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
    command
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_PRELOAD")
        .env_remove("LD_AUDIT")
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
            .env("LD_AUDIT", "/wrong-audit.so");

        clean_loader_environment(&mut command);

        let entries = command.get_envs().collect::<Vec<_>>();
        for variable in ["LD_LIBRARY_PATH", "LD_PRELOAD", "LD_AUDIT"] {
            expect_that!(
                entries
                    .iter()
                    .any(|(name, value)| { *name == OsStr::new(variable) && value.is_none() }),
                eq(true)
            );
        }
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
