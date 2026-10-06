#!/bin/bash -l
# One allocation coordinator; capacity.py owns the individual srun steps.
set -euo pipefail
: "${QUEST_CAMPAIGN_ROOT:?}"
: "${QUEST_SNAPSHOT:?}"
: "${QUEST_COMPILER:?gnu or cray}"
: "${QUEST_BUILD_RECEIPT:?set the successful matching build receipt directory}"
: "${SLURM_JOB_ID:?run as the allocation coordinator, outside srun}"
: "${SLURM_CPUS_PER_TASK:?}"
[[ "$SLURM_JOB_ID" =~ ^[0-9]+$ ]] || exit 2
case "$QUEST_COMPILER" in gnu|cray) ;; *) exit 2;; esac
quest_nodes=${SLURM_JOB_NUM_NODES:-${SLURM_NNODES:-}}
[[ "$quest_nodes" =~ ^[1248]$ ]] || exit 2
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
        printf 'Capacity coordinator requires an allocation or verified rank-zero batch context\n' >&2
        exit 2
    fi
fi
for quest_rank_variable in QUEST_MPI_SUPERVISED_CHILD OMPI_COMM_WORLD_RANK \
        PMI_RANK PMIX_RANK MV2_COMM_WORLD_RANK; do
    if [[ -v "$quest_rank_variable" ]]; then
        printf 'Run one capacity coordinator outside MPI ranks: %s is set\n' "$quest_rank_variable" >&2
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
quest_executable="$CARGO_TARGET_DIR/debug/examples/sparse_capacity"
test -x "$quest_executable"
quest_receipt_parent="$QUEST_CAMPAIGN_ROOT/capacity-receipts/$QUEST_COMPILER"
quest_tmp_parent="$QUEST_CAMPAIGN_ROOT/tmp/$QUEST_COMPILER"
mkdir -p "$quest_receipt_parent" "$quest_tmp_parent"
quest_receipts=$(mktemp -d "$quest_receipt_parent/job-$SLURM_JOB_ID.XXXXXXXX")
export TMPDIR
TMPDIR=$(mktemp -d "$quest_tmp_parent/capacity-$SLURM_JOB_ID.XXXXXXXX")
export PYTHONDONTWRITEBYTECODE=1
unset SLURM_NTASKS_PER_NODE SLURM_TASKS_PER_NODE
printf 'Capacity receipts: %s\nShared temporary files: %s\n' "$quest_receipts" "$TMPDIR"
printf '%s\n' "$quest_digest" > "$quest_receipts/source-digest"
printf '%s\n' "$QUEST_ROOT" > "$quest_receipts/native-prefix"
printf '%s\n' "$CARGO_TARGET_DIR" > "$quest_receipts/target-directory"
printf '%s\n' "$QUEST_BUILD_RECEIPT" > "$quest_receipts/build-receipt-path"
cp "$CARGO_TARGET_DIR/.native-build-profile.json" "$quest_receipts/build-profile.json"
cp "$QUEST_SNAPSHOT/source-snapshot.json" "$quest_receipts/source-snapshot.json"
sha256sum < "$quest_script" > "$quest_receipts/capacity-job.sha256"
sha256sum < "$quest_helpers/capacity.py" > "$quest_receipts/capacity-runner.sha256"
sha256sum < "$quest_helpers/runtime-env.sh" > "$quest_receipts/runtime-env.sha256"
sha256sum < "$quest_verifier" > "$quest_receipts/snapshot-verifier.sha256"
sha256sum < "$quest_executable" > "$quest_receipts/executable.sha256"
module -t list > "$quest_receipts/modules.txt" 2>&1
quest_command=(python3 -B "$quest_helpers/capacity.py"
    --executable "$quest_executable" --output "$quest_receipts/artifacts"
    --launcher srun --nodes "$quest_nodes" --ranks-per-node 1
    --threads "$SLURM_CPUS_PER_TASK"
    --start-dimension "${QUEST_CAPACITY_START_DIMENSION:-1024}"
    --max-dimension "${QUEST_CAPACITY_MAX_DIMENSION:-536870912}"
    --max-cases "${QUEST_CAPACITY_MAX_CASES:-20}"
    --repetitions "${QUEST_CAPACITY_REPETITIONS:-2}"
    --process-as-mib "${QUEST_CAPACITY_PROCESS_AS_MIB:-8192}"
    --model-rank-mib "${QUEST_CAPACITY_MODEL_RANK_MIB:-4096}"
    --omp-stack-mib "${QUEST_CAPACITY_OMP_STACK_MIB:-8}"
    --timeout "${QUEST_CAPACITY_TIMEOUT_SECONDS:-3600}")
python3 - "${quest_command[@]}" <<'PY' > "$quest_receipts/invocation.json"
import json
import os
import sys
keys = ('QUEST_SOURCE_DIGEST', 'CARGO_TARGET_DIR', 'QUEST_ROOT', 'RUSTFLAGS',
        'CARGO_ENCODED_RUSTFLAGS', 'SLURM_JOB_ID', 'SLURM_JOB_NUM_NODES',
        'SLURM_CPUS_PER_TASK', 'SRUN_CPUS_PER_TASK', 'OMP_NUM_THREADS',
        'OMP_PLACES', 'OMP_PROC_BIND', 'TMPDIR')
print(json.dumps(dict(command=sys.argv[1:], environment={key: os.environ.get(key) for key in keys}), indent=2))
PY
quest_stage=capacity
trap 'quest_status=$?; printf "stage=%s\nexit_status=%s\n" "$quest_stage" "$quest_status" > "$quest_receipts/status"' EXIT
cd "$QUEST_SNAPSHOT"
"${quest_command[@]}" > "$quest_receipts/capacity.log" 2>&1
quest_stage=completion-verification
python3 - "$quest_receipts/artifacts/completion.json" "$quest_receipts/executable.sha256" <<'PY'
import json
from pathlib import Path
import sys
completion = json.loads(Path(sys.argv[1]).read_text())
if (completion.get('complete') is not True
        or type(completion.get('capacity_closed')) is not bool
        or completion.get('executable_sha256') != Path(sys.argv[2]).read_text().split()[0]):
    raise SystemExit('Capacity runner did not complete with the admitted executable identity')
PY
quest_stage=source-verification
quest_final_digest=$(python3 "$quest_verifier" --verify --root . --digest-only)
test "$quest_final_digest" = "$quest_digest"
printf '%s\n' "$quest_digest" > "$quest_receipts/capacity-run-complete"
quest_stage=complete
