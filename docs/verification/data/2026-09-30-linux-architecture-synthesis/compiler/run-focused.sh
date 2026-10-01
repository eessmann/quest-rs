#!/usr/bin/env bash
set -uo pipefail
export PATH=/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=3
cd /path/to/validation/architecture-synthesis/source
devenv --no-tui shell -- cargo nextest run --locked -p quest-circuit --all-features --no-fail-fast --test-threads 1 -E 'binary(beam_contract) | binary(beam_approx_contract) | binary(beam_mitm_contract)' > ../compiler/focused-red.log 2>&1
printf '%s\n' "$?" > ../compiler/focused-red.exit
