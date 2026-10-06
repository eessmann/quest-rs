#!/bin/bash
set -euo pipefail

# MpiTest adds --ntasks separately for each child. Each MPI process owns one
# node and uses the allocated physical cores for its enabled OpenMP work.
# This mode must execute before module setup and must not start another shell.
if [[ "${1:-}" = --mpi-step ]]; then
    shift
    [[ "${1:-}" =~ ^--ntasks=([1-9][0-9]*)$ ]] || exit 2
    quest_step_ranks=${BASH_REMATCH[1]}
    quest_step_nodes=${SLURM_JOB_NUM_NODES:-${SLURM_NNODES:-}}
    [[ "$quest_step_nodes" =~ ^[1-8]$ ]] || exit 2
    if (( quest_step_ranks > quest_step_nodes )); then
        printf 'Requested %s MPI ranks require %s exclusive nodes; allocation has %s\n' \
            "$quest_step_ranks" "$quest_step_ranks" "$quest_step_nodes" >&2
        exit 2
    fi
    [[ "${SLURM_CPUS_PER_TASK:-}" =~ ^[1-9][0-9]*$ ]] || exit 2
    export SRUN_CPUS_PER_TASK="$SLURM_CPUS_PER_TASK"
    export OMP_NUM_THREADS="$SLURM_CPUS_PER_TASK" OMP_PLACES=cores OMP_PROC_BIND=close
    unset SLURM_NTASKS_PER_NODE SLURM_TASKS_PER_NODE
    exec srun --nodes="$quest_step_ranks" --ntasks-per-node=1 --distribution=block:block \
        --kill-on-bad-exit=1 --hint=nomultithread --cpu-bind=cores \
        --cpus-per-task="$SLURM_CPUS_PER_TASK" --exclusive --exact "$@"
fi

: "${QUEST_CAMPAIGN_ROOT:?}"
: "${QUEST_SNAPSHOT:?}"
: "${QUEST_COMPILER:?gnu or cray}"
: "${QUEST_BUILD_RECEIPT:?set the successful matching build receipt directory}"
: "${SLURM_JOB_ID:?run as the allocation coordinator, outside srun}"
: "${SLURM_CPUS_PER_TASK:?}"
[[ "$SLURM_JOB_ID" =~ ^[0-9]+$ ]] || exit 2
case "$QUEST_COMPILER" in gnu|cray) ;; *) exit 2;; esac
quest_allocation_nodes=${SLURM_JOB_NUM_NODES:-${SLURM_NNODES:-}}
[[ "$quest_allocation_nodes" =~ ^[1-8]$ ]] || exit 2
export QUEST_TEST_STAGE=${QUEST_TEST_STAGE:-workspace}
case "$QUEST_TEST_STAGE" in
    workspace)
        if (( quest_allocation_nodes != 8 )); then
            printf 'The full rank matrix requires eight exclusive nodes; select QUEST_TEST_STAGE=smoke for two nodes\n' >&2
            exit 2
        fi;;
    smoke)
        if (( quest_allocation_nodes < 2 )); then
            printf 'The 1/2-rank smoke stage requires two exclusive nodes\n' >&2
            exit 2
        fi;;
    native-ui) ;;
    *) printf 'QUEST_TEST_STAGE must be smoke, workspace, or native-ui\n' >&2; exit 2;;
esac
# Cirrus sbatch exports rank-zero task markers even outside an srun step.
# Admit that batch context without removing markers inherited by child tools.
quest_slurm_task_context=0
for quest_slurm_variable in SLURM_PROCID SLURM_LOCALID SLURM_NODEID SLURM_STEP_ID SLURM_STEPID; do
    if [[ -v "$quest_slurm_variable" ]]; then quest_slurm_task_context=1; fi
done
if (( quest_slurm_task_context )); then
    quest_slurm_batch=1
    if [[ "${SLURM_PROCID-}" != 0 ]]; then quest_slurm_batch=0; fi
    if [[ -v SLURM_JOBID && "$SLURM_JOBID" != "$SLURM_JOB_ID" ]]; then quest_slurm_batch=0; fi
    for quest_slurm_variable in SLURM_LOCALID SLURM_NODEID; do
        if [[ -v "$quest_slurm_variable" && "${!quest_slurm_variable}" != 0 ]]; then quest_slurm_batch=0; fi
    done
    for quest_slurm_variable in SLURM_STEP_ID SLURM_STEPID; do
        if [[ -v "$quest_slurm_variable" && "${!quest_slurm_variable}" != batch ]]; then quest_slurm_batch=0; fi
    done
    if (( ! quest_slurm_batch )); then
        printf 'Test coordinator requires an allocation or verified rank-zero batch context\n' >&2
        exit 2
    fi
fi
for quest_rank_variable in QUEST_MPI_SUPERVISED_CHILD OMPI_COMM_WORLD_RANK \
        PMI_RANK PMIX_RANK MV2_COMM_WORLD_RANK; do
    if [[ -v "$quest_rank_variable" ]]; then
        printf 'Run one test coordinator outside MPI ranks: %s is set\n' "$quest_rank_variable" >&2
        exit 2
    fi
done
quest_script=$(realpath -- "${BASH_SOURCE[0]}")
QUEST_CAMPAIGN_ROOT=$(cd -- "$QUEST_CAMPAIGN_ROOT" && pwd)
QUEST_SNAPSHOT=$(cd -- "$QUEST_SNAPSHOT" && pwd)
QUEST_BUILD_RECEIPT=$(cd -- "$QUEST_BUILD_RECEIPT" && pwd)
quest_helpers="$QUEST_SNAPSHOT/docs/verification/fixtures/cirrus"
quest_verifier=${QUEST_SNAPSHOT_VERIFIER:-"$quest_helpers/snapshot.py"}
quest_digest=$(python3 "$quest_verifier" --verify --root "$QUEST_SNAPSHOT" --digest-only)
export QUEST_SOURCE_DIGEST="$quest_digest"
case "$QUEST_BUILD_RECEIPT" in
    "$QUEST_CAMPAIGN_ROOT/receipts/$QUEST_COMPILER/"job-*) ;;
    *) printf 'Build receipt must belong to this compiler campaign\n' >&2; exit 2;;
esac
test "$(cat "$QUEST_BUILD_RECEIPT/build-complete")" = "$quest_digest"
test "$(cat "$QUEST_BUILD_RECEIPT/source-digest")" = "$quest_digest"
cmp "$QUEST_BUILD_RECEIPT/source-snapshot.json" "$QUEST_SNAPSHOT/source-snapshot.json"
source "$quest_helpers/runtime-env.sh"
test "$(cat "$QUEST_BUILD_RECEIPT/native-prefix")" = "$QUEST_ROOT"
test "$(cat "$QUEST_BUILD_RECEIPT/target-directory")" = "$CARGO_TARGET_DIR"
test -d "$QUEST_ROOT"
test -d "$CARGO_TARGET_DIR"
quest_receipt_parent="$QUEST_CAMPAIGN_ROOT/test-receipts/$QUEST_COMPILER"
quest_tmp_parent="$QUEST_CAMPAIGN_ROOT/tmp/$QUEST_COMPILER"
mkdir -p "$quest_receipt_parent" "$quest_tmp_parent"
quest_receipts=$(mktemp -d "$quest_receipt_parent/job-$SLURM_JOB_ID.XXXXXXXX")
export TMPDIR
TMPDIR=$(mktemp -d "$quest_tmp_parent/job-$SLURM_JOB_ID.XXXXXXXX")
printf 'Test receipts: %s\nShared temporary files: %s\n' "$quest_receipts" "$TMPDIR"
printf '%s\n' "$quest_digest" > "$quest_receipts/source-digest"
printf '%s\n' "$QUEST_TEST_STAGE" > "$quest_receipts/test-stage"
printf '%s\n' "$QUEST_ROOT" > "$quest_receipts/native-prefix"
printf '%s\n' "$CARGO_TARGET_DIR" > "$quest_receipts/target-directory"
cp "$CARGO_TARGET_DIR/.native-build-profile.json" "$quest_receipts/build-profile.json"
printf '%s\n' "$QUEST_BUILD_RECEIPT" > "$quest_receipts/build-receipt-path"
cp "$QUEST_SNAPSHOT/source-snapshot.json" "$quest_receipts/source-snapshot.json"
sha256sum < "$quest_script" > "$quest_receipts/test-script.sha256"
sha256sum < "$quest_helpers/runtime-env.sh" > "$quest_receipts/runtime-env.sha256"
sha256sum < "$quest_verifier" > "$quest_receipts/snapshot-verifier.sha256"
module -t list > "$quest_receipts/modules.txt" 2>&1
export QUEST_MPI_LAUNCHER=slurm
export QUEST_MPI_LAUNCHER_EXECUTABLE="$quest_helpers/test-rust.sh"
export QUEST_MPI_LAUNCHER_ARGS='["--mpi-step"]'
test -x "$QUEST_MPI_LAUNCHER_EXECUTABLE"
sha256sum < "$QUEST_MPI_LAUNCHER_EXECUTABLE" > "$quest_receipts/mpi-launcher.sha256"
python3 - <<'PY' > "$quest_receipts/runtime-environment.json"
import json
import os
keys = ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_TARGET_DIR', 'QUEST_ROOT',
        'HDF5_DIR', 'SLURM_JOB_ID', 'SLURM_JOB_NUM_NODES', 'SLURM_NNODES',
        'SLURM_CPUS_PER_TASK', 'SRUN_CPUS_PER_TASK', 'OMP_NUM_THREADS', 'OMP_PLACES',
        'OMP_PROC_BIND', 'CARGO_BUILD_JOBS', 'QUEST_TEST_STAGE', 'TMPDIR', 'QUEST_MPI_LAUNCHER',
        'QUEST_MPI_LAUNCHER_EXECUTABLE', 'QUEST_MPI_LAUNCHER_ARGS')
print(json.dumps({key: os.environ.get(key) for key in keys}, indent=2))
PY
quest_ui_mode=ordinary
quest_ui_helper=${QUEST_NATIVE_UI_HELPER:-"$quest_helpers/native_ui.py"}
quest_ui_features=${QUEST_UI_FEATURES:-all}
quest_stage=tests
trap 'quest_status=$?; printf "stage=%s\nexit_status=%s\n" "$quest_stage" "$quest_status" > "$quest_receipts/status"' EXIT
cd "$QUEST_SNAPSHOT"
if [[ "$QUEST_TEST_STAGE" = workspace || "$QUEST_TEST_STAGE" = native-ui ]]; then
    if [[ "$QUEST_TEST_STAGE" = workspace ]]; then quest_ui_features=all; fi
    if [[ "$QUEST_COMPILER" = cray ]]; then
        sha256sum < "$quest_ui_helper" > "$quest_receipts/native-ui-helper.sha256"
        quest_ui_mode=$(python3 "$quest_ui_helper" prepare "$quest_receipts" "$quest_ui_features")
    fi
    if [[ "$QUEST_TEST_STAGE" = native-ui && "$quest_ui_mode" != native ]]; then
        printf 'Native UI requires the Cray native host profile without configured build.target; see native-ui-admission.json\n' >&2
        exit 2
    fi
fi
quest_run_native_ui() (
    export CARGO_TARGET_DIR="$quest_receipts/native-ui-target"
    export CARGO_ENCODED_RUSTFLAGS
    CARGO_ENCODED_RUSTFLAGS=$(cat "$quest_receipts/native-ui-rustflags")
    # The parent admission verifies this target is the host before removing the
    # explicit target request which would suppress flags on host dependencies.
    unset CARGO_BUILD_TARGET
    quest_ui_options=()
    if [[ "$quest_ui_features" = all ]]; then quest_ui_options+=(--all-features); fi
    cargo nextest run --locked --offline -p quest-compile -p quest-rs \
        "${quest_ui_options[@]}" --test compile_fail --test-threads=1 --no-fail-fast \
        --success-output=immediate
)
{
    if [[ "$QUEST_TEST_STAGE" = workspace ]]; then
        quest_nextest_status=0
        quest_base_filter=()
        if [[ "$quest_ui_mode" = native ]]; then
            quest_base_filter=(-E 'not (binary(=compile_fail) and (package(=quest-compile) or package(=quest-rs)))')
        fi
        cargo nextest run --locked --offline --workspace --all-features \
            --test-threads=1 --no-fail-fast --success-output=immediate \
            "${quest_base_filter[@]}" || quest_nextest_status=$?
        printf '%s\n' "$quest_nextest_status" > "$quest_receipts/nextest-status"
        quest_ui_status=0
        if [[ "$quest_ui_mode" = native ]]; then
            quest_stage=native-ui
            quest_run_native_ui > "$quest_receipts/native-ui.log" 2>&1 || quest_ui_status=$?
            printf '%s\n' "$quest_ui_status" > "$quest_receipts/native-ui-status"
        fi
        quest_stage=doctests
        quest_doctest_status=0
        cargo test --locked --offline --workspace --all-features --doc -- --test-threads=1 \
            > "$quest_receipts/doctests.log" 2>&1 || quest_doctest_status=$?
        printf '%s\n' "$quest_doctest_status" > "$quest_receipts/doctest-status"
        if [[ "$quest_ui_mode" = native ]]; then
            python3 "$quest_ui_helper" summarize "$quest_receipts"
        fi
        if (( quest_nextest_status != 0 )); then exit "$quest_nextest_status"; fi
        if (( quest_ui_status != 0 )); then exit "$quest_ui_status"; fi
        if (( quest_doctest_status != 0 )); then exit "$quest_doctest_status"; fi
    elif [[ "$QUEST_TEST_STAGE" = native-ui ]]; then
        quest_ui_status=0
        quest_run_native_ui > "$quest_receipts/native-ui.log" 2>&1 || quest_ui_status=$?
        printf '%s\n' "$quest_ui_status" > "$quest_receipts/native-ui-status"
        python3 "$quest_ui_helper" summarize "$quest_receipts"
        if (( quest_ui_status != 0 )); then exit "$quest_ui_status"; fi
    else
        quest_smoke_status=0
        for quest_smoke_test in \
            mpi_survives_quest_drop_and_reinitialization_is_rejected \
            mpi_threaded_views_exchange_and_collectives_agree \
            mpi_collective_preflight_rejects_mismatch_on_every_rank \
            mpi_distributed_drop_with_live_resource_aborts_job; do
            quest_case_status=0
            cargo nextest run --locked --offline -p quest-sys --features mpi --test mpi \
                -E "test(=$quest_smoke_test)" --test-threads=1 --no-fail-fast \
                --success-output=immediate || quest_case_status=$?
            printf '%s %s\n' "$quest_smoke_test" "$quest_case_status" >> "$quest_receipts/nextest-status"
            if (( quest_case_status != 0 )); then quest_smoke_status=$quest_case_status; fi
        done
        quest_case_status=0
        cargo nextest run --locked --offline -p quest-rs --all-features --test collective_runtime \
            -E 'test(=collective_rejects_different_classical_results_and_step_counts)' \
            --test-threads=1 --no-fail-fast --success-output=immediate || quest_case_status=$?
        printf '%s %s\n' collective_rejects_different_classical_results_and_step_counts "$quest_case_status" >> "$quest_receipts/nextest-status"
        if (( quest_case_status != 0 )); then quest_smoke_status=$quest_case_status; fi
        if (( quest_smoke_status != 0 )); then exit "$quest_smoke_status"; fi
    fi
} > "$quest_receipts/tests.log" 2>&1
quest_stage=source-verification
quest_final_digest=$(python3 "$quest_verifier" --verify --root . --digest-only)
test "$quest_final_digest" = "$quest_digest"
printf '%s\n' "$quest_digest" > "$quest_receipts/tests-complete"
quest_stage=complete
