#!/usr/bin/env bash
set -euo pipefail
# Run inside `devenv shell -- bash ...`; dependency caches are intentionally warm.
receipt_dir="${1:-docs/verification/data/2026-10-01-mathematical-audit/functions}"
mkdir -p "$receipt_dir"
{
  date -u
  uname -sm
  rustc --version --verbose
  cargo --version
  git rev-parse HEAD
} > "$receipt_dir/environment.txt"
# Prewarm transitive dependencies and both example configurations. Each timed
# command then rebuilds one leaf after an mtime change, without changing bytes.
cargo build --locked --release -p quest-polynomial --examples \
  > "$receipt_dir/prewarm.log" 2>&1
for representation in dynamic typed; do
  name="function_${representation}"
  touch "crates/quest-polynomial/examples/$name.rs"
  /usr/bin/time -p cargo build --locked --release -p quest-polynomial --example "$name" \
    > "$receipt_dir/${name}-build.log" 2> "$receipt_dir/${name}-build.time.txt"
  executable="${CARGO_TARGET_DIR:-target}/release/examples/$name"
  wc -c < "$executable" > "$receipt_dir/${name}-bytes.txt"
  size "$executable" > "$receipt_dir/${name}-sections.txt"
  shasum -a 256 "$executable" > "$receipt_dir/${name}.sha256"
  for trial in 1 2 3; do
    "$executable" > "$receipt_dir/${name}-trial${trial}.csv"
  done
done
shasum -a 256 Cargo.lock rust-toolchain.toml \
  crates/quest-polynomial/examples/function_dynamic.rs \
  crates/quest-polynomial/examples/function_typed.rs \
  crates/quest-polynomial/examples/support/function_measurement.rs \
  crates/quest-polynomial/src/{function,backend,typed,lib}.rs \
  > "$receipt_dir/sources.sha256"
