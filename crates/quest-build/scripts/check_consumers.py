#!/usr/bin/env python3
"""Build and directly execute independent direct/wrapped QuEST consumers."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--work-dir", type=Path)
    args = parser.parse_args()
    repository = Path(__file__).resolve().parents[3]
    work = (args.work_dir or Path(tempfile.mkdtemp(prefix="quest-native-consumers-"))).resolve()
    work.mkdir(parents=True, exist_ok=True)
    config = args.config.resolve(strict=True)
    environment = os.environ.copy()
    for name in ["LD_LIBRARY_PATH", "LD_PRELOAD", "LD_AUDIT", "QUEST_ROOT", "QUEST_DIR", "QuEST_ROOT", "QuEST_DIR", "QUEST_RUNTIME_LIBRARY_PATH"]:
        environment.pop(name, None)
    environment["QUEST_NATIVE_CONFIG"] = str(config)
    environment.setdefault("CARGO_BUILD_JOBS", "4")
    target = repository / "target"
    toolchain = tomllib.loads((repository / "rust-toolchain.toml").read_text())["toolchain"]["channel"]

    def write(path, content):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    quest_dependency = f'quest = {{ package = "quest-rs", path = {json.dumps(str(repository / "crates/quest"))} }}\n'
    build_dependency = f'\n[build-dependencies]\nquest-build = {{ path = {json.dumps(str(repository / "crates/quest-build"))} }}\n'
    build_script = 'fn main() -> quest_build::Result<()> { quest_build::emit_final_target_runtime_paths() }\n'
    computation = '''
    let plan = quest::circuit! { qubit[2] q; h q[0]; cx q[0], q[1]; }?
        .bind(&[])?.lower()?.plan()?;
    assert_eq!(plan.num_qubits(), 2);
    assert_eq!(plan.instructions().len(), 2);
    let environment = quest::Environment::builder().build()?;
    let mut register = environment.state_vector(quest::QubitCount::new(1)?)?;
    register.h(0)?;
    let state = register.snapshot()?;
    assert_eq!((state.nrows(), state.ncols()), (2, 1));
    for row in 0..2 {
        assert!((state[(row, 0)].re - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert!(state[(row, 0)].im.abs() < 1e-12);
    }
    drop(register);
    environment.close()?;
    Ok(())
'''
    write(work / "Cargo.toml", '[workspace]\nresolver = "3"\nmembers = ["direct", "wrapped", "wrapper", "renamed"]\n')
    for package in ["direct", "wrapped", "wrapper", "renamed"]:
        package_name = f"quest-consumer-{package}"
        manifest = f'[package]\nname = "{package_name}"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[dependencies]\n'
        if package == "wrapped":
            manifest += 'quest-consumer-wrapper = { path = "../wrapper" }\n'
        elif package == "renamed":
            manifest += quest_dependency.replace("quest =", "quantum =", 1)
        else:
            manifest += quest_dependency
        if package != "wrapper":
            manifest += build_dependency
            write(work / package / "build.rs", build_script)
        write(work / package / "Cargo.toml", manifest)
    write(work / "direct/src/main.rs", 'fn main() -> Result<(), Box<dyn std::error::Error>> {\n' + computation + '}\n')
    write(work / "wrapper/src/lib.rs", 'pub fn run() -> Result<(), Box<dyn std::error::Error>> {\n' + computation + '}\n')
    write(work / "wrapped/src/main.rs", 'fn main() -> Result<(), Box<dyn std::error::Error>> { quest_consumer_wrapper::run() }\n')
    write(work / "renamed/src/main.rs", 'fn main() -> Result<(), Box<dyn std::error::Error>> {\n' + computation.replace("quest::", "quantum::") + '}\n')
    command = ["cargo", f"+{toolchain}", "build", "--offline", "--manifest-path", str(work / "Cargo.toml"), "--target-dir", str(target)]
    with (work / "build.log").open("w") as log:
        result = subprocess.run(command, cwd=work, env=environment, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise SystemExit(f"Consumer build failed; inspect {work / 'build.log'}")
    for package in ["direct", "wrapped", "renamed"]:
        executable = target / "debug" / f"quest-consumer-{package}"
        dynamic = subprocess.run(["readelf", "-d", str(executable)], env=environment, cwd=work, check=True, capture_output=True, text=True).stdout
        write(work / f"{package}-readelf.log", dynamic)
        if "(RPATH)" not in dynamic or "(RUNPATH)" in dynamic:
            raise SystemExit(f"{package} has the wrong loader policy; inspect {work / (package + '-readelf.log')}")
        closure = subprocess.run(["ldd", str(executable)], env=environment, cwd=work, check=True, capture_output=True, text=True).stdout
        write(work / f"{package}-ldd.log", closure)
        if "not found" in closure or "libQuEST" not in closure:
            raise SystemExit(f"{package} has an unresolved native closure")
        subprocess.run([str(executable)], cwd=work, env=environment, check=True)
        print(f"{package}: DT_RPATH, complete native closure, numerical state check passed")
    print(f"Preserved consumer fixture and evidence: {work}")


if __name__ == "__main__":
    main()
