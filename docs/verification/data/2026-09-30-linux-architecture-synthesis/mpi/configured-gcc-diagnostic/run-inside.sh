#!/usr/bin/env bash
set -uo pipefail
receipt=/path/to/validation/architecture-synthesis/mpi
export QUEST_ROOT=$(sed -n '/-quest-mpi-validation-4.3.0$/p' "$receipt/paths.txt" | head -1)
mpi_dev=$(sed -n '/-openmpi-5.0.10-dev$/p' "$receipt/paths.txt" | head -1)
mpi_bin=$(sed -n '/-openmpi-5.0.10$/p' "$receipt/paths.txt" | head -1)
export MPICC="$mpi_dev/bin/mpicc"
export CARGO_TARGET_DIR=../target-mpi CARGO_BUILD_JOBS=3
export CXX=/nix/store/z4c6k0mrlkwl3s4w9ysxc8vq1wylm3ms-gcc-wrapper-15.3.0/bin/c++
export CMAKE_PREFIX_PATH="$mpi_dev" PKG_CONFIG_PATH="$mpi_dev/lib/pkgconfig"
export PATH="$mpi_dev/bin:$mpi_bin/bin:$PATH"
{
 date -u; uname -a; rustc -Vv; cargo -V; mpiexec --version; "$MPICC" -show
 printf "CXX=%s\n" "$CXX"
 printf 'QUEST_ROOT=%s\nMPICC=%s\nCARGO_TARGET_DIR=%s\nCARGO_BUILD_JOBS=%s\n' "$QUEST_ROOT" "$MPICC" "$CARGO_TARGET_DIR" "$CARGO_BUILD_JOBS"
 env | sort | grep -E '^(OMPI|HWLOC|PRTE|MPI|QUEST|CMAKE|CARGO_BUILD|CARGO_TARGET)' || true
 sha256sum ../source-manifest.json Cargo.toml Cargo.lock nix/quest.nix crates/quest/tests/collective_runtime.rs crates/quest/src/collective.rs crates/quest-build/src/rsmpi.rs crates/quest-build/native/mpi_abi.c
} > "$receipt/inventory.log" 2>&1
run() {
 label=$1; shift
 printf '%q ' "$@" >> "$receipt/commands.log"; printf '\n' >> "$receipt/commands.log"
 "$@" > "$receipt/$label.log" 2>&1
 status=$?; printf '%s\n' "$status" > "$receipt/$label.exit"
}
run launcher mpiexec -n 2 hostname
run collective cargo test --locked -p quest-rs --features mpi --test collective_runtime -- --test-threads=1
run lint cargo clippy --locked -p quest-rs --features mpi --all-targets -- -D warnings
find ../target-mpi -type f \( -name quest_mpi_abi -o -name rsmpi-abi \) -print > "$receipt/abi-executables.txt"
while IFS= read -r witness; do printf '%s\n' "$witness"; "$witness"; done < "$receipt/abi-executables.txt" > "$receipt/abi-witnesses.log" 2>&1
printf 'complete\n' > "$receipt/run.complete"
