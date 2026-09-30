#!/usr/bin/bash
export PATH=/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=6
export CARGO_TARGET_DIR=/home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/worker-target
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
run() {
 name="$1"
 shift
 printf '%q ' "$@" > "../receipts/$name.command"
 printf '\n' >> "../receipts/$name.command"
 devenv --no-tui shell -- "$@" > "../receipts/$name.log" 2>&1
 printf '%s\n' "$?" > "../receipts/$name.exit"
}
run worker-isolated-engines-final cargo test --locked -p quest-optimizer-worker --features synthesis,zx,mitm -- --test-threads=1
run worker-isolated-client-final cargo test --locked -p quest-optimizer-client -- --test-threads=1
run worker-isolated-compiler-final cargo test --locked -p quest-circuit --features workers --test worker_contract -- --test-threads=1
run worker-isolated-clippy-final cargo clippy --locked -p quest-optimizer-client -p quest-optimizer-worker --features quest-optimizer-worker/synthesis,quest-optimizer-worker/zx,quest-optimizer-worker/mitm --all-targets
run worker-isolated-release-phase-final cargo test --release --locked -p quest-optimizer-worker --features synthesis,zx,mitm --test synthesis_process fixed_exact_phase_sequence_survives_the_bounded_process -- --exact --nocapture --test-threads=1
run worker-isolated-native-build-final cargo build --locked -p quest-optimizer-worker --features synthesis,zx,mitm
run worker-isolated-native-final env QUEST_TUTORIAL_WORKER=/home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/worker-target/debug/quest-optimizer-worker cargo test --locked -p quest-rs --features workers --test structured_workers -- --test-threads=1
printf 'complete\n' > ../receipts/worker-isolated-final.done
