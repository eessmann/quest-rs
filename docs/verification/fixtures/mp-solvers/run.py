#!/usr/bin/env python3
"""Compare complete MP solvers against a frozen pre-migration checkout."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tomli_w


def render_manifest(root):
    dependencies = {"csv": "1.4"}
    for name in ["quest-numerics", "quest-polynomial", "quest-qsp", "quest-math", "quest-synthesis"]:
        dependency = {"path": str(root / "crates" / name)}
        if name == "quest-qsp":
            dependency["features"] = ["offline-synthesis"]
        dependencies[name] = dependency
    return tomli_w.dumps({
        "package": {"name": "mp-solvers", "version": "0.0.0", "edition": "2024"},
        "workspace": {}, "dependencies": dependencies, "profile": {"release": {"lto": "thin"}},
    })


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--repository", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    fixture = Path(__file__).resolve().parent
    output = args.output.resolve()
    if output.exists() and any(output.iterdir()):
        parser.error("output must be empty; receipts are not overwritten")
    receipts = output / "receipts"
    receipts.mkdir(parents=True)
    roots = [("astro", args.baseline.resolve()), ("dashu", args.repository.resolve())]
    env = dict(os.environ, CARGO_BUILD_JOBS="2")

    def fingerprints():
        result = {"fixture/" + p.name: sha(p) for p in fixture.iterdir() if p.is_file()}
        for label, root in roots:
            for name in ["Cargo.toml", "Cargo.lock"]:
                result[label + "/" + name] = sha(root / name)
            for crate in ["quest-numerics", "quest-polynomial", "quest-qsp", "quest-math", "quest-synthesis", "quest-language", "quest-symbolic"]:
                for path in (root / "crates" / crate).rglob("*"):
                    if path.is_file() and path.suffix in [".rs", ".toml"]:
                        result[label + "/" + str(path.relative_to(root))] = sha(path)
        return result

    before = fingerprints()
    (receipts / "sources-before.json").write_text(json.dumps(before, indent=2) + "\n")
    with (receipts / "toolchain.txt").open("w") as stream:
        subprocess.run(["rustc", "-Vv"], stdout=stream, check=True)
        stream.write(platform.platform() + "\n")
    binaries = []
    for label, root in roots:
        package = output / label
        (package / "src").mkdir(parents=True)
        shutil.copyfile(fixture / "main.rs", package / "src/main.rs")
        shutil.copyfile(fixture / "allocator.rs", package / "src/allocator.rs")
        (package / "Cargo.toml").write_text(render_manifest(root))
        shutil.copyfile(root / "Cargo.lock", package / "Cargo.lock")
        shutil.copyfile(root / "Cargo.lock", receipts / (label + "-input.lock"))
        env["CARGO_TARGET_DIR"] = str(output / "build" / label)
        with (receipts / (label + "-build.log")).open("w") as stream:
            subprocess.run(["/usr/bin/time", "-l" if platform.system() == "Darwin" else "-v", "cargo", "build", "--release", "--offline", "--manifest-path", str(package / "Cargo.toml")],
                           env=env, stdout=stream, stderr=subprocess.STDOUT, check=True)
        shutil.copyfile(package / "Cargo.lock", receipts / (label + "-resolved.lock"))
        binaries.append((label, output / "build" / label / "release/mp-solvers"))
    for trial in range(1, 4):
        order = binaries if trial % 2 else binaries[::-1]
        for label, binary in order:
            with (receipts / f"{label}-{trial}.csv").open("w") as stdout, \
                 (receipts / f"{label}-{trial}.time").open("w") as stderr:
                subprocess.run(["/usr/bin/time", "-l" if platform.system() == "Darwin" else "-v", str(binary)],
                               stdout=stdout, stderr=stderr, check=True)
    after = fingerprints()
    (receipts / "sources-after.json").write_text(json.dumps(after, indent=2) + "\n")
    if before != after:
        raise SystemExit("Sources changed during comparison; reject these timings")
    (receipts / "binaries.json").write_text(json.dumps({label: {
        "sha256": sha(binary), "bytes": binary.stat().st_size,
    } for label, binary in binaries}, indent=2) + "\n")
    print(receipts)


if __name__ == "__main__":
    main()
