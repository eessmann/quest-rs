#!/usr/bin/bash
set -u
export PATH=/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=6
cd /path/to/validation/architecture-synthesis/source
run() {
 name="$1"
 shift
 printf '%q ' "$@" > "../receipts/$name.command"
 printf '\n' >> "../receipts/$name.command"
 devenv --no-tui shell -- "$@" > "../receipts/$name.log" 2>&1
 printf '%s\n' "$?" > "../receipts/$name.exit"
}
run worker-client cargo test --locked -p quest-optimizer-client -- --test-threads=1
run worker-engines cargo test --locked -p quest-optimizer-worker --features synthesis,zx,mitm -- --test-threads=1
run worker-compiler cargo test --locked -p quest-circuit --features workers --test worker_contract -- --test-threads=1
run worker-clippy cargo clippy --locked -p quest-optimizer-client -p quest-optimizer-worker --features quest-optimizer-worker/synthesis,quest-optimizer-worker/zx,quest-optimizer-worker/mitm --all-targets
run worker-native-build cargo build --locked -p quest-optimizer-worker --features synthesis,zx,mitm
run worker-native env QUEST_TUTORIAL_WORKER=/path/to/validation/architecture-synthesis/source/target/debug/quest-optimizer-worker cargo test --locked -p quest-rs --features workers --test structured_workers -- --test-threads=1
run worker-disabled cargo test --locked -p quest-optimizer-worker --no-default-features
printf 'complete\n' > ../receipts/worker-validation.done
