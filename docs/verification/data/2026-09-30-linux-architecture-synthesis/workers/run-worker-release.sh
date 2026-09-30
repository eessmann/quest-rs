#!/usr/bin/bash
export PATH=/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=6
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
devenv --no-tui shell -- cargo test --release --locked -p quest-optimizer-worker --features synthesis,zx,mitm --test synthesis_process fixed_exact_phase_sequence_survives_the_bounded_process -- --exact --nocapture --test-threads=1 > ../receipts/worker-release-phase.log 2>&1
printf '%s\n' "$?" > ../receipts/worker-release-phase.exit
