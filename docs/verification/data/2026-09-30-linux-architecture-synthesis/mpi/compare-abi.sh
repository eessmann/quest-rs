#!/usr/bin/env bash
set -uo pipefail
base=/path/to/validation/architecture-synthesis
cd "$base/source"
receipt="$base/mpi"
: > "$receipt/abi-comparison.log"
while IFS= read -r native; do
 wrapper="$(dirname "$native")/rsmpi-abi"
 label="$(printf '%s' "$native" | sha256sum | cut -c1-12)"
 "$native" > "$receipt/abi-$label-native.bin" 2>&1; native_status=$?
 "$wrapper" > "$receipt/abi-$label-wrapper.bin" 2>&1; wrapper_status=$?
 cmp -s "$receipt/abi-$label-native.bin" "$receipt/abi-$label-wrapper.bin"; comparison=$?
 printf '%s native=%s wrapper=%s byte-comparison=%s\n' "$label" "$native_status" "$wrapper_status" "$comparison" >> "$receipt/abi-comparison.log"
done < <(find ../target-mpi -type f -name quest_mpi_abi)
sha256sum devenv.nix Cargo.lock crates/quest-compile/src/workers.rs crates/quest-build/src/rsmpi.rs crates/quest-build/native/mpi_abi.c crates/quest/src/collective.rs crates/quest/tests/collective_runtime.rs > "$receipt/final-source.sha256"
