#!/bin/bash
# Sourced only by native-validation.sbatch's scaling stage after profile admission.
# Parent-owned variables are read and updated by this sourced stage fragment.
# shellcheck disable=SC2154,SC2034
test "$QUEST_STAGE" = scaling
printf 'release\n' > "$quest_receipts/build-profile"
cmp "$QUEST_BUILD_RECEIPT/build-profile" "$quest_receipts/build-profile"
quest_executable="$CARGO_TARGET_DIR/release/examples/sparse_capacity"
test -x "$quest_executable"
cp "$QUEST_BUILD_RECEIPT/executable.sha256" "$quest_receipts/executable.sha256"
run_stage executable-before sha256sum --check --quiet "$quest_receipts/executable.sha256"
quest_executable_sha=$(cut -d ' ' -f 1 "$quest_receipts/executable.sha256")
export OMP_DYNAMIC=FALSE OMP_STACKSIZE=8388608B
mkdir "$quest_receipts/scaling"
printf '%s\n' "$quest_nodes" > "$quest_receipts/allocated-nodes"
verify_scaling_inputs() {
    local directory="$1" ranks="$2" rank expected
    local manifest="$directory/input.sha256"
    : > "$manifest" || return
    for ((rank = 0; rank < ranks; rank++)); do
        expected=$(jq --exit-status --raw-output \
            '.local_input_sha256 | select(type == "string" and test("^[a-f0-9]{64}$"))' \
            "$directory/rank-$rank.json") || return
        printf '%s  %s\n' "$expected" "$directory/input-rank-$rank.coo32" >> "$manifest" || return
    done
    sha256sum --check --quiet "$manifest"
}
if [[ "$(cat "$quest_receipts/executable-before.status")" = 0 ]]; then
    run_stage scaling-placement srun --nodes=8 --ntasks=8 --ntasks-per-node=1 \
        --cpus-per-task=288 --hint=nomultithread --distribution=block:block \
        --cpu-bind=cores --kill-on-bad-exit=1 --time=00:01:00 hostname
    quest_placement_log="$quest_receipts/scaling-placement.log"
    quest_placement=1
    if [[ "$(cat "$quest_receipts/scaling-placement.status")" = 0 ]] \
        && [[ "$(wc -l < "$quest_placement_log")" -eq 8 ]] \
        && [[ "$(sort -u "$quest_placement_log" | wc -l)" -eq 8 ]]; then
        quest_placement=0
    fi
    printf '%s\n' "$quest_placement" > "$quest_receipts/scaling-placement-check.status"
    if (( quest_placement != 0 )); then quest_failed=1; fi
    for quest_active_nodes in 2 4 8; do
        if (( quest_placement != 0 )); then break; fi
        for quest_series in strong weak; do
            quest_dimension=65536
            if [[ "$quest_series" = weak ]]; then quest_dimension=$((8192 * quest_active_nodes)); fi
            quest_case="$quest_receipts/scaling/$quest_series-$quest_active_nodes"
            mkdir "$quest_case"
            quest_command=(srun --nodes="$quest_active_nodes" --ntasks="$quest_active_nodes" \
                --ntasks-per-node=1 --cpus-per-task=288 --hint=nomultithread \
                --distribution=block:block --cpu-bind=cores --kill-on-bad-exit=1 --time=00:08:00 \
                "$quest_executable" --scaling "$quest_case" "$quest_dimension" 3 \
                4294967296 4294967296 "$quest_active_nodes" 1 288 8388608)
            jq --null-input --arg source "$QUEST_SOURCE_DIGEST" --arg native "$QUEST_NATIVE_SHA256" \
                --arg executable "$quest_executable_sha" --arg compiler "$QUEST_COMPILER" \
                --arg series "$quest_series" --argjson allocated_nodes "$quest_nodes" \
                --argjson active_nodes "$quest_active_nodes" --argjson dimension "$quest_dimension" \
                --args '{schema_version: 1, evidence_kind: "scaling-only", capacity_closed: false,
                    source_manifest_sha256: $source, native_library_sha256: $native,
                    executable_sha256: $executable, compiler: $compiler, build_profile: "release",
                    series: $series, allocated_nodes: $allocated_nodes, active_nodes: $active_nodes,
                    dimension: $dimension, repetitions: 3, ranks_per_node: 1, threads: 288,
                    managed_rank_bytes: 4294967296, managed_node_bytes: 4294967296,
                    stack_bytes_per_worker: 8388608, argv: $ARGS.positional}' \
                -- "${quest_command[@]}" > "$quest_case/invocation.json"
            run_stage "scaling-$quest_series-$quest_active_nodes" "${quest_command[@]}"
            run_stage "scaling-$quest_series-$quest_active_nodes-reports" jq --exit-status --slurp \
                --slurpfile invocation "$quest_case/invocation.json" \
                --rawfile placement "$quest_placement_log" \
                --arg source "$QUEST_SOURCE_DIGEST" --arg native "$QUEST_NATIVE_SHA256" \
                --arg executable "$quest_executable_sha" \
                -f "$QUEST_SOURCE/docs/verification/fixtures/cirrus/scaling-report.jq" \
                "$quest_case"/rank-*.json
            run_stage "scaling-$quest_series-$quest_active_nodes-inputs" verify_scaling_inputs \
                "$quest_case" "$quest_active_nodes"
        done
    done
fi
