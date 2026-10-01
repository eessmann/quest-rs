#!/usr/bin/env bash
set -uo pipefail
export PATH="/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
export CARGO_BUILD_JOBS=6
cd /path/to/validation/architecture-synthesis/source
printf '%s\n' 'devenv shell -- cargo nextest run --locked -p quest-circuit --all-features --test-threads=1' > ../receipts/facade-linux.command
devenv --no-tui shell -- cargo nextest run --locked -p quest-circuit --all-features --test-threads=1 > ../receipts/facade-linux.log 2>&1
printf '%s\n' "$?" > ../receipts/facade-linux.exit
