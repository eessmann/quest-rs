#!/bin/bash -l
# Submit with one exclusive node, eight CPUs, lowpriority, and two hours.
set -euo pipefail
: "${QUEST_VALIDATION_ROOT:?}"
: "${QUEST_SOURCE:?}"
: "${QUEST_COMPILER:?}"
: "${QUEST_TORC_BIN:?absolute installed torc path}"
: "${QUEST_TORC_SERVER_BIN:?absolute installed torc-server path}"
: "${TMPDIR:?Cirrus job-local temporary storage}"
: "${SLURM_JOB_ID:?}"
[[ "${SLURM_JOB_NUM_NODES:?}" = 1 && "${SLURM_CPUS_PER_TASK:?}" = 8 ]]
export QUEST_STAGE=${QUEST_STAGE:-build}
case "$QUEST_STAGE" in
    build|discovery|scaling-build) ;;
    *) printf 'Torc standalone requires build, discovery, or scaling-build stage\n' >&2; exit 2;;
esac
quest_torc_job="native-$QUEST_STAGE"
quest_torc_receipts="$QUEST_VALIDATION_ROOT/torc/$QUEST_COMPILER/$SLURM_JOB_ID"
mkdir -p "$(dirname "$quest_torc_receipts")"
mkdir "$quest_torc_receipts"
quest_torc_local=$(mktemp -d "$TMPDIR/quest-torc.XXXXXXXX")
archive_torc() {
    local status=$?
    trap - EXIT
    # All standalone commands have returned: copy closed databases and log bytes.
    if ! tar -czf "$quest_torc_receipts/local-artifacts.tar.gz" -C "$quest_torc_local" .; then
        status=1
    fi
    printf '%s\n' "$status" > "$quest_torc_receipts/job-exit.status"
    exit "$status"
}
trap archive_torc EXIT
cd "$quest_torc_local"
quest_torc=("$QUEST_TORC_BIN" --standalone --db "$quest_torc_local/torc.db"
    --torc-server-bin "$QUEST_TORC_SERVER_BIN")
"$QUEST_TORC_BIN" --version > "$quest_torc_receipts/version.txt"
sha256sum "$QUEST_TORC_BIN" "$QUEST_TORC_SERVER_BIN" > "$quest_torc_receipts/binaries.sha256"
cp "$QUEST_SOURCE/docs/verification/fixtures/cirrus/torc-$quest_torc_job.yaml" "$quest_torc_receipts/workflow.yaml"
"${quest_torc[@]}" -f json create "$quest_torc_receipts/workflow.yaml" \
    > "$quest_torc_receipts/create.json" 2> "$quest_torc_receipts/create.stderr"
quest_workflow_id=$(jq -er '.workflow_id | select(type == "number" and . > 0 and floor == .)' \
    "$quest_torc_receipts/create.json")
quest_run_status=0
"${quest_torc[@]}" run "$quest_workflow_id" --num-cpus 8 --time-limit PT1H50M \
    --output-dir "$quest_torc_local/output" \
    > "$quest_torc_receipts/run.log" 2>&1 || quest_run_status=$?
printf '%s\n' "$quest_run_status" > "$quest_torc_receipts/run.status"
"${quest_torc[@]}" workflows export "$quest_workflow_id" --include-results --include-events \
    --output "$quest_torc_receipts/workflow-export.json" \
    > "$quest_torc_receipts/export.log" 2>&1
# Torc's CLI return code alone does not establish successful job execution.
jq -e --argjson expected_id "$quest_workflow_id" --arg expected_job "$quest_torc_job" '
    .workflow.id == $expected_id and
    (.jobs | length) == 1 and (.results | length) == 1 and
    .jobs[0].name == $expected_job and .jobs[0].status == "completed" and
    .results[0].status == "completed" and .results[0].return_code == 0 and
    .results[0].job_id == .jobs[0].id and
    .results[0].workflow_id == .workflow.id and
    .results[0].run_id == .workflow.run_id and
    .results[0].attempt_id == .jobs[0].attempt_id
' "$quest_torc_receipts/workflow-export.json" > "$quest_torc_receipts/acceptance.json"
test "$quest_run_status" = 0
test "$(cat "$QUEST_VALIDATION_ROOT/receipts/$QUEST_COMPILER/$SLURM_JOB_ID/stage")" = "$QUEST_STAGE"
test "$(cat "$QUEST_VALIDATION_ROOT/receipts/$QUEST_COMPILER/$SLURM_JOB_ID/completed.status")" = 0
