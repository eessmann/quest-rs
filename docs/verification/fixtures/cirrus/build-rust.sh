#!/bin/bash -l
set -euo pipefail
: "${QUEST_CAMPAIGN_ROOT:?}"
: "${QUEST_SNAPSHOT:?set the immutable snapshot directory}"
: "${QUEST_COMPILER:?gnu or cray}"
: "${SLURM_CPUS_PER_TASK:?}"
: "${SLURM_JOB_ID:?run inside the build allocation}"
[[ "$SLURM_JOB_ID" =~ ^[0-9]+$ ]] || exit 2
quest_script=$(realpath -- "${BASH_SOURCE[0]}")
QUEST_SNAPSHOT=$(cd -- "$QUEST_SNAPSHOT" && pwd)
quest_verifier=${QUEST_SNAPSHOT_VERIFIER:-"$QUEST_SNAPSHOT/docs/verification/fixtures/cirrus/snapshot.py"}
quest_digest=$(python3 "$quest_verifier" --verify --root "$QUEST_SNAPSHOT" --digest-only)
export QUEST_SOURCE_DIGEST="$quest_digest"
mkdir -p "$QUEST_CAMPAIGN_ROOT"
source "$QUEST_SNAPSHOT/docs/verification/fixtures/cirrus/runtime-env.sh"
quest_receipt_parent="$QUEST_CAMPAIGN_ROOT/receipts/$QUEST_COMPILER"
mkdir -p "$quest_receipt_parent"
quest_receipts=$(mktemp -d "$quest_receipt_parent/job-$SLURM_JOB_ID.XXXXXXXX")
printf 'Build receipts: %s\n' "$quest_receipts"
cd "$QUEST_SNAPSHOT"
printf '%s\n' "$quest_digest" > "$quest_receipts/source-digest"
printf '%s\n' "$QUEST_ROOT" > "$quest_receipts/native-prefix"
printf '%s\n' "$CARGO_TARGET_DIR" > "$quest_receipts/target-directory"
cp "$CARGO_TARGET_DIR/.native-build-profile.json" "$quest_receipts/build-profile.json"
sha256sum < "$quest_script" > "$quest_receipts/build-script.sha256"
sha256sum < "$quest_verifier" > "$quest_receipts/snapshot-verifier.sha256"
sha256sum < "$QUEST_SNAPSHOT/docs/verification/fixtures/cirrus/runtime-env.sh" > "$quest_receipts/runtime-env.sha256"
python3 - <<'PY' > "$quest_receipts/rustflags.json"
import json
import os
print(json.dumps({key: os.environ.get(key) for key in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')}, indent=2))
PY
module -t list > "$quest_receipts/modules.txt" 2>&1
rustc -vV > "$quest_receipts/rustc.txt"
CC --version > "$quest_receipts/cxx.txt"
cp source-snapshot.json "$quest_receipts/source-snapshot.json"
cargo build --locked --offline -p xtask -p quest-sys --features quest-sys/mpi
cargo run --locked --offline -p xtask -- native-doctor --json > "$quest_receipts/native-doctor.json"
cargo run --locked --offline -p xtask -- generate-quest-bindings --check
cargo test --locked --offline --workspace --all-features --no-run
cargo build --locked --offline -p quest-rs --features mpi,qsvt-io --examples
quest_final_digest=$(python3 "$quest_verifier" --verify --root . --digest-only)
test "$quest_final_digest" = "$quest_digest"
printf '%s\n' "$quest_digest" > "$quest_receipts/build-complete"
