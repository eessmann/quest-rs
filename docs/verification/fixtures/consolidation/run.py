#!/usr/bin/env python3
"""Run identical pure-crate allocation probes against a selected repository tree."""
import argparse
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--repository", type=Path, required=True)
parser.add_argument("--work", type=Path, required=True)
args = parser.parse_args()
repo = args.repository.resolve()
work = args.work.resolve()
work.mkdir(parents=True, exist_ok=True)
if (work / "Cargo.toml").exists():
    raise SystemExit("Choose a new work directory to retain prior evidence")
(work / "src").mkdir()
(work / "src/main.rs").write_bytes(Path(__file__).with_name("main.rs").read_bytes())
quote = lambda value: json.dumps(str(value))
(work / "Cargo.toml").write_text(f'''[package]
name = "quest-consolidation-measurement"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
quest-circuit = {{ path = {quote(repo / "crates/quest-circuit")}, default-features = false }}
quest-language = {{ path = {quote(repo / "crates/quest-language")} }}
''')
(work / "rust-toolchain.toml").write_bytes((repo / "rust-toolchain.toml").read_bytes())
env = dict(os.environ, CARGO_BUILD_JOBS="4")
with (work / "build.log").open("w") as log:
    subprocess.run(["cargo", "build", "--offline", "--manifest-path", str(work / "Cargo.toml"), "--target-dir", str(work / "target")], cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
with (work / "results.jsonl").open("w") as out, (work / "diagnostics.log").open("w") as err:
    subprocess.run([str(work / "target/debug/quest-consolidation-measurement")], cwd=work, stdout=out, stderr=err, check=True)
print((work / "results.jsonl").read_text(), end="")
print((work / "diagnostics.log").read_text(), end="")
