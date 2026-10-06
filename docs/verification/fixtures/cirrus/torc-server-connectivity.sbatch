#!/bin/bash -l
# PROPOSED, NOT SUBMITTED: use the documented sbatch command in torc-server.md.
set -euo pipefail
: "${TORC_API_URL:?confirmed user-provided server endpoint required}"
: "${QUEST_CONNECTIVITY_RECEIPT:?new receipt directory required}"
mkdir "$QUEST_CONNECTIVITY_RECEIPT"
hostname -f > "$QUEST_CONNECTIVITY_RECEIPT/hostname.txt"
printf '%s\n' "$TORC_API_URL" > "$QUEST_CONNECTIVITY_RECEIPT/endpoint.txt"
"$WORK/.local/bin/torc" --version > "$QUEST_CONNECTIVITY_RECEIPT/client-version.txt"
status=0
# The documented two-minute Slurm allocation deadline bounds this check.
"$WORK/.local/bin/torc" --url "$TORC_API_URL" ping \
    > "$QUEST_CONNECTIVITY_RECEIPT/ping.log" 2>&1 || status=$?
printf '%s\n' "$status" > "$QUEST_CONNECTIVITY_RECEIPT/ping.status"
exit "$status"
