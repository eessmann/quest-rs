#!/bin/bash -l
# One exclusive node, eight CPUs, lowpriority, and twenty minutes.
set -euo pipefail
: "${QUEST_HOST_PROBE_ROOT:?}"
: "${QUEST_HOST_PROBE_SOURCE:?}"
: "${QUEST_HOST_PROBE_MANIFEST:?}"
: "${SLURM_JOB_ID:?}"
[[ "${SLURM_JOB_NUM_NODES:?}" = 1 && "${SLURM_CPUS_PER_TASK:?}" = 8 ]]
quest_receipts="$QUEST_HOST_PROBE_ROOT/receipts/$SLURM_JOB_ID"
mkdir -p "$quest_receipts"
record_exit() {
    local status=$?
    printf '%s\n' "$status" > "$quest_receipts/job-exit.status"
}
trap record_exit EXIT
module restore
module load cmake cray-hdf5
module unload cray-libsci
export CC=cc CXX=CC CARGO_BUILD_JOBS=8 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export OMP_NUM_THREADS="$SLURM_CPUS_PER_TASK" OMP_PLACES=cores
export SRUN_CPUS_PER_TASK="$SLURM_CPUS_PER_TASK"
export CARGO_UNSTABLE_HOST_CONFIG=true
export CARGO_UNSTABLE_TARGET_APPLIES_TO_HOST=true
export CARGO_TARGET_APPLIES_TO_HOST=false
export CARGO_HOST_LINKER=cc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS='-C linker-features=-lld -C link-arg=-mno-daz-ftz'
cd "$QUEST_HOST_PROBE_SOURCE"
sha256sum --check --quiet "$QUEST_HOST_PROBE_MANIFEST" > "$quest_receipts/source-before.log" 2>&1
module -t list > "$quest_receipts/modules.txt" 2>&1
rustc -vV > "$quest_receipts/rustc.txt"
cc --version > "$quest_receipts/compiler.txt" 2>&1
quest_failed=0
for quest_mode in control implicit explicit; do
    quest_args=()
    if [[ "$quest_mode" != implicit ]]; then quest_args+=(--target x86_64-unknown-linux-gnu); fi
    export CARGO_HOST_RUSTFLAGS='--cfg quest_host_policy'
    if [[ "$quest_mode" != control ]]; then
        CARGO_HOST_RUSTFLAGS+=' -C linker-features=-lld -C link-arg=-mno-daz-ftz'
    fi
    quest_target="$QUEST_HOST_PROBE_ROOT/targets/$SLURM_JOB_ID/$quest_mode"
    mkdir -p "$(dirname "$quest_target")"
    mkdir "$quest_target"
    quest_status=0
    # Controlled fixture inputs, independent of caller target-flag precedence.
    env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo run --locked --offline \
        --manifest-path Cargo.toml --target-dir "$quest_target" "${quest_args[@]}" \
        --config 'build.rustflags=["--cfg", "quest_target_policy"]' \
        --config 'target.x86_64-unknown-linux-gnu.rustflags=["--cfg", "quest_target_policy"]' \
        -vv > "$quest_receipts/$quest_mode.log" 2>&1 || quest_status=$?
    printf '%s\n' "$quest_status" > "$quest_receipts/$quest_mode.status"
    printf '%s: %s\n' "$quest_mode" "$quest_status"
    if [[ "$quest_mode" = control ]]; then
        if (( quest_status == 0 )); then quest_failed=1; fi
    elif (( quest_status != 0 )); then
        quest_failed=1
    fi
done
sha256sum --check --quiet "$QUEST_HOST_PROBE_MANIFEST" > "$quest_receipts/source-after.log" 2>&1
printf '%s\n' "$quest_failed" > "$quest_receipts/completed.status"
exit "$quest_failed"
