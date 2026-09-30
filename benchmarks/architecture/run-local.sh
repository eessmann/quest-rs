#!/usr/bin/env bash
set -euo pipefail
# Run from the repository root inside `devenv shell -- bash ...`.
receipt_dir="${1:-benchmarks/architecture/2026-09-29}"
mkdir -p "$receipt_dir"
{
  date -u
  uname -a
  sw_vers
  sysctl -n machdep.cpu.brand_string hw.memsize hw.ncpu
  rustc --version --verbose
  cargo --version
  git rev-parse HEAD
} > "$receipt_dir/environment.txt"
cargo build --release -p quest-rs --example architecture_runtime -p quest-synthesis --example architecture_synthesis > "$receipt_dir/build.log" 2>&1
shasum -a 256 target/release/examples/architecture_runtime target/release/examples/architecture_synthesis > "$receipt_dir/executables.sha256"
shasum -a 256 crates/quest/examples/architecture_runtime.rs crates/quest-synthesis/examples/architecture_synthesis.rs Cargo.lock > "$receipt_dir/inputs.sha256"
for trial in 1 2 3; do
  for qubits in 4 8 12; do
    /usr/bin/time -l ./target/release/examples/architecture_runtime "$qubits" > "$receipt_dir/runtime-q${qubits}-trial${trial}.csv" 2> "$receipt_dir/runtime-q${qubits}-trial${trial}.time.txt"
  done
  for digits in 3 6 9 12; do
    /usr/bin/time -l ./target/release/examples/architecture_synthesis "$digits" > "$receipt_dir/synthesis-d${digits}-trial${trial}.csv" 2> "$receipt_dir/synthesis-d${digits}-trial${trial}.time.txt"
  done
done
