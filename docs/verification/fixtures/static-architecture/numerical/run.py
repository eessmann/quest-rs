#!/usr/bin/env python3
"""Portable matched-accuracy numerical measurements, run inside project devenv."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tomli_w


def render_manifest(source, label, extra_bin):
    return tomli_w.dumps({
        "package": {"name": "numeric-performance-" + label, "version": "0.0.0", "edition": "2024"},
        "workspace": {},
        "dependencies": {"quest-polynomial": {"path": str(source / "crates/quest-polynomial")},
                         "quest-numerics": {"path": str(source / "crates/quest-numerics")}, "csv": "1.4"},
        "profile": {"release": {"lto": "thin"}},
        "bin": [{"name": extra_bin, "path": "src/extra.rs"}],
    })


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--baseline-repository", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--trials", type=int, default=3)
    args = parser.parse_args()
    if args.trials < 3:
        parser.error("at least three runtime trials are required")
    root = args.repository.resolve()
    baseline = args.baseline_repository.resolve()
    output = args.output.resolve()
    fixture = Path(__file__).resolve().parent
    for source in [root, baseline]:
        for name in ["Cargo.lock", "crates/quest-polynomial/Cargo.toml", "crates/quest-numerics/Cargo.toml"]:
            if not (source / name).is_file():
                parser.error(f"missing numerical repository input: {source / name}")
    if output.exists() and any(output.iterdir()):
        parser.error("output must be absent or empty; existing receipts are never overwritten")
    receipts = output / "receipts"
    packages = output / "packages"
    receipts.mkdir(parents=True)
    packages.mkdir()
    shutil.copyfile(fixture.parent / "allocator.rs", packages / "allocator.rs")
    sources = [("baseline", baseline), ("current", root)]
    for label, source in sources:
        package = packages / label
        (package / "src").mkdir(parents=True)
        shutil.copyfile(fixture / f"{label}.rs", package / "src/main.rs")
        extra_source = "baseline_typed.rs" if label == "baseline" else "mp.rs"
        extra_bin = "numeric-performance-baseline-typed" if label == "baseline" else "numeric-performance-mp"
        shutil.copyfile(fixture / extra_source, package / "src/extra.rs")
        (package / "Cargo.toml").write_text(render_manifest(source, label, extra_bin))
        shutil.copyfile(source / "Cargo.lock", package / "Cargo.lock")
        shutil.copyfile(source / "Cargo.lock", receipts / f"{label}-input-Cargo.lock")

    env = os.environ.copy()
    env["CARGO_BUILD_JOBS"] = "2"
    darwin = platform.system() == "Darwin"
    timer = ["/usr/bin/time", "-l" if darwin else "-v"]

    def run(command, logfile, environment=None):
        with (receipts / logfile).open("w") as stream:
            subprocess.run(command, cwd=root, env=environment, stdout=stream,
                           stderr=subprocess.STDOUT, check=True)

    with (receipts / "environment.txt").open("w") as stream:
        stream.write(datetime.datetime.now(datetime.timezone.utc).isoformat() + "\n")
        stream.flush()
        for command in [["uname", "-a"], ["rustc", "-Vv"], ["cargo", "-V"]]:
            subprocess.run(command, cwd=root, stdout=stream, stderr=stream, check=True)
        if darwin:
            subprocess.run(["sysctl", "machdep.cpu.brand_string", "hw.memsize", "hw.ncpu"],
                           cwd=root, stdout=stream, stderr=stream, check=True)
        elif shutil.which("lscpu"):
            subprocess.run(["lscpu"], cwd=root, stdout=stream, stderr=stream, check=True)
        stream.write("release_lto=thin\njobs=2\n")
        for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]:
            stream.write(f"{key}={env.get(key, '')!r}\n")
    (receipts / "inputs.json").write_text(json.dumps({
        "repository": str(root), "baseline_repository": str(baseline),
        "trials": args.trials, "allocator": "Rust global System wrapper",
        "clean_build": "empty Cargo artifact directory; operating-system caches not flushed",
    }, indent=2) + "\n")

    # Portable labels identify source paths without embedding machine paths.
    fingerprinted = [("fixture/allocator.rs", fixture.parent / "allocator.rs")]
    fingerprinted.extend(("fixture/numerical/" + p.name, p) for p in fixture.iterdir() if p.is_file())
    for label, source in sources:
        for name in ["Cargo.toml", "Cargo.lock", "devenv.nix", "devenv.lock", "rust-toolchain.toml"]:
            if (source / name).is_file():
                fingerprinted.append((f"{label}/{name}", source / name))
        for crate in ["quest-numerics", "quest-polynomial"]:
            directory = source / "crates" / crate
            for path in directory.rglob("*.rs"):
                fingerprinted.append((f"{label}/" + str(path.relative_to(source)), path))
            fingerprinted.append((f"{label}/crates/{crate}/Cargo.toml", directory / "Cargo.toml"))

    def source_hashes():
        return "".join(f"{sha(path)}  {name}\n" for name, path in sorted(fingerprinted))

    before = source_hashes()
    (receipts / "sources.sha256").write_text(before)
    binaries = []
    for label, _ in sources:
        package = packages / label
        target = output / "build" / label
        env["CARGO_TARGET_DIR"] = str(target)
        binary = f"numeric-performance-{label}"
        cargo = ["cargo", "build", "--release", "--manifest-path", str(package / "Cargo.toml"), "--bin", binary]
        run([*timer, *cargo], f"{label}-cold-build.log", env)
        run([*timer, *cargo], f"{label}-unchanged-build.log", env)
        os.utime(package / "src/main.rs", None)
        run([*timer, *cargo], f"{label}-incremental-build.log", env)
        extra = "numeric-performance-baseline-typed" if label == "baseline" else "numeric-performance-mp"
        run([*timer, "cargo", "build", "--release", "--manifest-path", str(package / "Cargo.toml"), "--bin", extra],
            f"{label}-extra-build.log", env)
        binaries.append((label, target / "release" / binary))
        binaries.append(("baseline-typed" if label == "baseline" else "mp", target / "release" / extra))
        shutil.copyfile(package / "Cargo.lock", receipts / f"{label}-resolved-Cargo.lock")
    (receipts / "executables.json").write_text(json.dumps({
        name: {"bytes": path.stat().st_size, "sha256": sha(path)}
        for name, path in binaries
    }, indent=2) + "\n")
    for name, path in binaries:
        run(["size", "-m" if darwin else "-A", str(path)], f"{name}-sections.txt")
    for trial in range(1, args.trials + 1):
        shift = (trial - 1) % len(binaries)
        for name, path in binaries[shift:] + binaries[:shift]:
            with (receipts / f"{name}-trial{trial}.csv").open("w") as stdout, \
                 (receipts / f"{name}-trial{trial}.details").open("w") as stderr:
                subprocess.run([str(path)], cwd=root, stdout=stdout, stderr=stderr, check=True)
    after = source_hashes()
    (receipts / "post-run-sources.sha256").write_text(after)
    if before != after:
        raise SystemExit("Sources changed during measurement; receipts retained but comparison is invalid")
    print(f"Verified receipts: {receipts}")


if __name__ == "__main__":
    main()
