#!/usr/bin/env bash
set -uo pipefail
export CARGO_BUILD_JOBS=6
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
failed=0
run_check() {
    label="acceptance-$1"
    shift
    printf '%q ' "$@" > "../receipts/${label}.command"
    printf '\n' >> "../receipts/${label}.command"
    date -u +%FT%TZ > "../receipts/${label}.started"
    "$@" > "../receipts/${label}.log" 2>&1
    result=$?
    date -u +%FT%TZ > "../receipts/${label}.finished"
    printf '%s\n' "$result" > "../receipts/${label}.exit"
    printf '%s: exit %s\n' "$label" "$result"
    if test "$result" -ne 0; then failed=1; fi
    return "$result"
}
run_check workspace-build cargo build --workspace --locked || exit 1
run_check workspace-nextest cargo nextest run --workspace --locked --no-fail-fast --test-threads=1
run_check workspace-doctests cargo test --doc --workspace --locked
run_check workspace-clippy cargo clippy --workspace --all-targets --locked -- -D warnings
run_check format cargo fmt --all -- --check
run_check bindings cargo run --locked -p xtask -- generate-quest-bindings --check
run_check native-consumers cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp
printf '%s\n' "$failed" > ../receipts/acceptance-workspace-driver.exit
exit "$failed"
