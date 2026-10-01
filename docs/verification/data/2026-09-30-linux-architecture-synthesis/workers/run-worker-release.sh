#!/usr/bin/bash
export PATH=/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=6
cd /path/to/validation/architecture-synthesis/source
devenv --no-tui shell -- cargo test --release --locked -p quest-optimizer-worker --features synthesis,zx,mitm --test synthesis_process fixed_exact_phase_sequence_survives_the_bounded_process -- --exact --nocapture --test-threads=1 > ../receipts/worker-release-phase.log 2>&1
printf '%s\n' "$?" > ../receipts/worker-release-phase.exit
