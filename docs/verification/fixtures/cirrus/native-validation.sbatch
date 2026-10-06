#!/bin/bash -l
# Submit directly with documented sbatch resource options. Run one coordinator.
set -euo pipefail
: "${QUEST_VALIDATION_ROOT:?}"
: "${QUEST_NATIVE_CAMPAIGN:?}"
: "${QUEST_SOURCE:?}"
: "${QUEST_SOURCE_MANIFEST:?}"
: "${QUEST_SOURCE_DIGEST:?}"
: "${QUEST_COMPILER:?gnu or cray}"
: "${QUEST_NATIVE_SHA256:?expected installed library identity}"
: "${QUEST_STAGE:?build, discovery, scaling-build, smoke, verify, or scaling}"
: "${SLURM_JOB_ID:?}"
: "${SLURM_CPUS_PER_TASK:?}"
[[ "$QUEST_SOURCE_DIGEST" =~ ^[a-f0-9]{64}$ ]]
[[ "$SLURM_JOB_ID" =~ ^[0-9]+$ ]]
[[ "$SLURM_CPUS_PER_TASK" =~ ^[1-9][0-9]*$ ]]
quest_nodes=${SLURM_JOB_NUM_NODES:?}
case "$QUEST_STAGE:$quest_nodes:$SLURM_CPUS_PER_TASK" in
    build:1:8|discovery:1:8|scaling-build:1:8|smoke:2:288|verify:8:288|scaling:8:288) ;;
    *) printf 'Unexpected stage or allocation\n' >&2; exit 2;;
esac
quest_receipts="$QUEST_VALIDATION_ROOT/receipts/$QUEST_COMPILER/$SLURM_JOB_ID"
mkdir -p "$quest_receipts"
record_exit() {
    local status=$?
    printf '%s\n' "$status" > "$quest_receipts/job-exit.status"
}
trap record_exit EXIT
module restore
case "$QUEST_COMPILER" in
    gnu) module switch PrgEnv-cray PrgEnv-gnu;;
    cray) ;;
    *) exit 2;;
esac
module load cmake cray-hdf5
if [[ "$QUEST_COMPILER" = cray ]]; then module unload cray-libsci; fi
export CC=cc CXX=CC
export QUEST_ROOT="$QUEST_NATIVE_CAMPAIGN/native/$QUEST_COMPILER"
export CARGO_TARGET_DIR="$QUEST_VALIDATION_ROOT/targets/$QUEST_COMPILER/$QUEST_SOURCE_DIGEST"
if [[ "$QUEST_STAGE" = discovery ]]; then
    export CARGO_TARGET_DIR="$QUEST_VALIDATION_ROOT/targets/discovery/$QUEST_COMPILER/$QUEST_SOURCE_DIGEST"
elif [[ "$QUEST_STAGE" = scaling-build || "$QUEST_STAGE" = scaling ]]; then
    export CARGO_TARGET_DIR="$QUEST_VALIDATION_ROOT/targets/scaling/$QUEST_COMPILER/$QUEST_SOURCE_DIGEST"
    export CARGO_PROFILE_RELEASE_DEBUG=0
fi
export CARGO_BUILD_JOBS=8 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc
export LIBCLANG_PATH=/usr/lib64
export CLANG=/opt/cray/pe/cce/19.0.0/cce-clang/x86_64/bin/clang
export OMP_NUM_THREADS="$SLURM_CPUS_PER_TASK" OMP_PLACES=cores OMP_PROC_BIND=close
export SRUN_CPUS_PER_TASK="$SLURM_CPUS_PER_TASK"
if [[ "$QUEST_COMPILER" = cray ]]; then
    # Cargo separates host artifacts when --target is supplied (including by
    # trybuild). Configure both domains through Cargo's documented nightly API.
    export CARGO_UNSTABLE_HOST_CONFIG=true
    export CARGO_UNSTABLE_TARGET_APPLIES_TO_HOST=true
    export CARGO_TARGET_APPLIES_TO_HOST=false
    export CARGO_HOST_LINKER=cc CARGO_HOST_X86_64_UNKNOWN_LINUX_GNU_LINKER=cc
    for quest_family in CARGO_HOST_RUSTFLAGS \
        CARGO_HOST_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS; do
        quest_flags=${!quest_family-}
        printf -v "$quest_family" '%s' "${quest_flags:+$quest_flags }-C linker-features=-lld -C link-arg=-mno-daz-ftz"
        export "${quest_family?}"
    done
    for quest_family in RUSTFLAGS RUSTDOCFLAGS; do
        quest_encoded=CARGO_ENCODED_$quest_family
        if [[ -v "$quest_encoded" ]]; then
            quest_flags=${!quest_encoded}
            if [[ -n "$quest_flags" ]]; then quest_flags+=$'\x1f'; fi
            quest_flags+=$'-C\x1flinker-features=-lld\x1f-C\x1flink-arg=-mno-daz-ftz'
            printf -v "$quest_encoded" '%s' "$quest_flags"
            export "${quest_encoded?}"
        else
            quest_flags=${!quest_family-}
            printf -v "$quest_family" '%s' "${quest_flags:+$quest_flags }-C linker-features=-lld -C link-arg=-mno-daz-ftz"
            export "${quest_family?}"
        fi
    done
fi
mkdir -p "$(dirname "$CARGO_TARGET_DIR")" "$QUEST_VALIDATION_ROOT/tmp"
if [[ "$QUEST_STAGE" = build || "$QUEST_STAGE" = discovery || "$QUEST_STAGE" = scaling-build ]]; then
    mkdir "$CARGO_TARGET_DIR"
else
    : "${QUEST_BUILD_RECEIPT:?matching successful build receipt}"
    test -d "$CARGO_TARGET_DIR"
    test "$(cat "$QUEST_BUILD_RECEIPT/completed.status")" = 0
    quest_expected_build_stage=build
    if [[ "$QUEST_STAGE" = scaling ]]; then quest_expected_build_stage=scaling-build; fi
    test "$(cat "$QUEST_BUILD_RECEIPT/stage")" = "$quest_expected_build_stage"
    test "$(cat "$QUEST_BUILD_RECEIPT/source-digest")" = "$QUEST_SOURCE_DIGEST"
fi
export TMPDIR
TMPDIR=$(mktemp -d "$QUEST_VALIDATION_ROOT/tmp/$SLURM_JOB_ID.XXXXXXXX")
cd "$QUEST_SOURCE"
test "$(sha256sum "$QUEST_SOURCE_MANIFEST" | cut -d ' ' -f 1)" = "$QUEST_SOURCE_DIGEST"
sha256sum --check --quiet "$QUEST_SOURCE_MANIFEST" > "$quest_receipts/source-before.log" 2>&1
printf '%s  %s\n' "$QUEST_NATIVE_SHA256" "$QUEST_ROOT/lib64/libQuEST.so.4.3.0" \
    | sha256sum --check --quiet > "$quest_receipts/native-identity.log" 2>&1
printf '%s\n' "$QUEST_SOURCE_DIGEST" > "$quest_receipts/source-digest"
printf '%s\n' "$QUEST_STAGE" > "$quest_receipts/stage"
module -t list > "$quest_receipts/modules.txt" 2>&1
rustc -vV > "$quest_receipts/rustc.txt"
CC --version > "$quest_receipts/cxx.txt" 2>&1
cargo nextest --version > "$quest_receipts/nextest.txt"
cat "$QUEST_ROOT/source-revision.txt" > "$quest_receipts/native-revision.txt"
for quest_key in QUEST_ROOT CARGO_TARGET_DIR CARGO_BUILD_JOBS CARGO_INCREMENTAL \
    CARGO_PROFILE_DEV_DEBUG CARGO_PROFILE_TEST_DEBUG CC CXX CLANG LIBCLANG_PATH \
    CARGO_PROFILE_RELEASE_DEBUG CARGO_PROFILE_RELEASE_OPT_LEVEL CARGO_PROFILE_RELEASE_LTO \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS \
    CARGO_UNSTABLE_HOST_CONFIG CARGO_UNSTABLE_TARGET_APPLIES_TO_HOST \
    CARGO_TARGET_APPLIES_TO_HOST CARGO_HOST_LINKER CARGO_HOST_RUSTFLAGS \
    CARGO_HOST_X86_64_UNKNOWN_LINUX_GNU_LINKER CARGO_HOST_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS \
    RUSTDOCFLAGS CARGO_ENCODED_RUSTDOCFLAGS HDF5_DIR CRAY_MPICH_DIR \
    SLURM_JOB_ID SLURM_JOB_NUM_NODES SLURM_CPUS_PER_TASK OMP_NUM_THREADS \
    OMP_PLACES OMP_PROC_BIND SRUN_CPUS_PER_TASK TMPDIR; do
    if [[ -v "$quest_key" ]]; then declare -p "$quest_key"; fi
done > "$quest_receipts/selected-environment.txt"
for quest_key in QUEST_ROOT CC CXX CLANG LIBCLANG_PATH CFLAGS CXXFLAGS CPPFLAGS \
    LDFLAGS CMAKE_TOOLCHAIN_FILE CPATH LIBRARY_PATH LD_LIBRARY_PATH PKG_CONFIG_PATH \
    CMAKE_PREFIX_PATH HDF5_DIR CRAY_MPICH_DIR MPICC MPI_PKG_CONFIG RUSTFLAGS \
    CARGO_ENCODED_RUSTFLAGS RUSTDOCFLAGS CARGO_ENCODED_RUSTDOCFLAGS \
    CARGO_UNSTABLE_HOST_CONFIG CARGO_UNSTABLE_TARGET_APPLIES_TO_HOST \
    CARGO_TARGET_APPLIES_TO_HOST CARGO_HOST_LINKER CARGO_HOST_RUSTFLAGS \
    CARGO_HOST_X86_64_UNKNOWN_LINUX_GNU_LINKER CARGO_HOST_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER CARGO_PROFILE_DEV_DEBUG \
    CARGO_PROFILE_TEST_DEBUG CARGO_PROFILE_RELEASE_DEBUG CARGO_PROFILE_RELEASE_OPT_LEVEL \
    CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_INCREMENTAL; do
    if [[ -v "$quest_key" ]]; then declare -p "$quest_key"; fi
done > "$quest_receipts/compiler-inputs.txt"
if [[ "$QUEST_STAGE" = smoke || "$QUEST_STAGE" = verify || "$QUEST_STAGE" = scaling ]]; then
    for quest_profile in modules.txt cxx.txt rustc.txt native-revision.txt compiler-inputs.txt; do
        cmp "$QUEST_BUILD_RECEIPT/$quest_profile" "$quest_receipts/$quest_profile"
    done
fi
if [[ "$QUEST_STAGE" = scaling-build || "$QUEST_STAGE" = scaling ]]; then
    printf '%s  %s\n' "$QUEST_NATIVE_SHA256" "$QUEST_ROOT/lib64/libQuEST.so.4.3.0" \
        > "$quest_receipts/native.sha256"
    if [[ "$QUEST_STAGE" = scaling ]]; then
        cmp "$QUEST_BUILD_RECEIPT/native.sha256" "$quest_receipts/native.sha256"
    fi
fi
quest_failed=0
run_stage() {
    local name="$1"
    shift
    local status=0
    printf '%q ' "$@" > "$quest_receipts/$name.command"
    printf '\n' >> "$quest_receipts/$name.command"
    "$@" > "$quest_receipts/$name.log" 2>&1 || status=$?
    printf '%s\n' "$status" > "$quest_receipts/$name.status"
    printf '%s: %s\n' "$name" "$status"
    if (( status != 0 )); then quest_failed=1; fi
}
if [[ "$QUEST_STAGE" = build ]]; then
    run_stage doctor cargo run --locked --offline -p xtask -- native-doctor --json
    run_stage bindings cargo run --locked --offline -p xtask -- generate-quest-bindings --check
    run_stage default-build cargo test --locked --offline --workspace --no-run
    run_stage all-feature-build cargo test --locked --offline --workspace --all-features --no-run
    run_stage mpi-consumer-build cargo build --locked --offline --manifest-path docs/verification/fixtures/native-mpi-consumer/Cargo.toml
elif [[ "$QUEST_STAGE" = discovery ]]; then
    run_stage discovery-tests cargo test --locked --offline -p quest-build --all-features -- --test-threads=1
    run_stage discovery-clippy cargo clippy --locked --offline -p quest-build --all-features --all-targets --no-deps -- -D warnings
    run_stage doctor cargo run --locked --offline -p xtask -- native-doctor --json
    run_stage bindings cargo run --locked --offline -p xtask -- generate-quest-bindings --check
elif [[ "$QUEST_STAGE" = scaling-build ]]; then
    printf 'release\n' > "$quest_receipts/build-profile"
    run_stage scaling-build cargo build --release --locked --offline -p quest-rs \
        --features mpi,qsvt-io --example sparse_capacity
    if [[ "$(cat "$quest_receipts/scaling-build.status")" = 0 ]]; then
        sha256sum "$CARGO_TARGET_DIR/release/examples/sparse_capacity" > "$quest_receipts/executable.sha256"
    fi
elif [[ "$QUEST_STAGE" = scaling ]]; then
    source "$QUEST_SOURCE/docs/verification/fixtures/cirrus/native-scaling.sh"
else
    export QUEST_MPI_LAUNCHER=slurm QUEST_MPI_LAUNCHER_EXECUTABLE=srun
    # Serialized JSON for the Rust supervisor, never expanded as shell arguments.
    # shellcheck disable=SC2089
    printf -v QUEST_MPI_LAUNCHER_ARGS '["--nodes=1-%s","--ntasks-per-node=1","--cpus-per-task=%s","--hint=nomultithread","--distribution=block:block","--cpu-bind=cores","--kill-on-bad-exit=1"]' "$quest_nodes" "$SLURM_CPUS_PER_TASK"
    # shellcheck disable=SC2090
    export QUEST_MPI_LAUNCHER_ARGS
    printf '%s\n' "$QUEST_MPI_LAUNCHER_ARGS" > "$quest_receipts/mpi-launcher-args.json"
    for quest_ranks in 1 2 4 8; do
        if (( quest_ranks > quest_nodes )); then break; fi
        run_stage "placement-$quest_ranks" srun --nodes="1-$quest_nodes" --ntasks="$quest_ranks" --ntasks-per-node=1 \
            --cpus-per-task="$SLURM_CPUS_PER_TASK" --hint=nomultithread \
            --distribution=block:block --cpu-bind=cores --kill-on-bad-exit=1 \
            hostname
        quest_placement=0
        test "$(cat "$quest_receipts/placement-$quest_ranks.status")" = 0 \
            && test "$(wc -l < "$quest_receipts/placement-$quest_ranks.log")" -eq "$quest_ranks" \
            && test "$(sort -u "$quest_receipts/placement-$quest_ranks.log" | wc -l)" -eq "$quest_ranks" \
            || quest_placement=1
        printf '%s\n' "$quest_placement" > "$quest_receipts/placement-$quest_ranks-check.status"
        if (( quest_placement != 0 )); then exit "$quest_placement"; fi
        run_stage "mpi-consumer-$quest_ranks" srun --nodes="1-$quest_nodes" \
            --ntasks="$quest_ranks" --ntasks-per-node=1 --time=00:02:00 \
            --cpus-per-task="$SLURM_CPUS_PER_TASK" --hint=nomultithread \
            --distribution=block:block --cpu-bind=cores --kill-on-bad-exit=1 \
            "$CARGO_TARGET_DIR/debug/quest-independent-mpi-consumer" --worker "$quest_ranks"
    done
    run_stage abi cargo nextest run --locked --offline -p quest-build -p quest-sys \
        --features quest-sys/mpi --lib --test-threads=1 --no-fail-fast --success-output=immediate \
        -E 'test(generated_abi_tests::) | test(rsmpi::tests::)'
    if [[ "$QUEST_STAGE" = smoke ]]; then
        run_stage mpi-smoke cargo nextest run --locked --offline -p quest-sys --features mpi \
            --test mpi --test-threads=1 --no-fail-fast --success-output=immediate \
            -E 'test(=mpi_survives_quest_drop_and_reinitialization_is_rejected) | test(=mpi_threaded_views_exchange_and_collectives_agree) | test(=mpi_collective_preflight_rejects_mismatch_on_every_rank) | test(=mpi_distributed_drop_with_live_resource_aborts_job)'
    else
        run_stage all-features cargo nextest run --locked --offline --workspace --all-features \
            --test-threads=1 --no-fail-fast --success-output=immediate
        run_stage default-tests cargo nextest run --locked --offline --workspace \
            --test-threads=1 --no-fail-fast --success-output=immediate
        run_stage doctests cargo test --locked --offline --workspace --all-features --doc -- --test-threads=1
        run_stage default-doctests cargo test --locked --offline --workspace --doc -- --test-threads=1
        run_stage default-check cargo check --locked --offline --workspace
        run_stage all-feature-check cargo check --locked --offline --workspace --all-features
        run_stage clippy cargo clippy --locked --offline --workspace --all-features --all-targets -- -D warnings
        run_stage fmt cargo fmt --all -- --check
        run_stage consumers cargo run --locked --offline -p xtask -- check-native-consumers \
            --backends cpu,omp --work-dir "$quest_receipts/consumers"
    fi
fi
if [[ "$QUEST_STAGE" = scaling-build || "$QUEST_STAGE" = scaling ]]; then
    run_stage native-after sha256sum --check --quiet "$quest_receipts/native.sha256"
    if [[ -f "$quest_receipts/executable.sha256" ]]; then
        run_stage executable-after sha256sum --check --quiet "$quest_receipts/executable.sha256"
    fi
fi
run_stage source-after sha256sum --check --quiet "$QUEST_SOURCE_MANIFEST"
printf '%s\n' "$quest_failed" > "$quest_receipts/completed.status"
exit "$quest_failed"
