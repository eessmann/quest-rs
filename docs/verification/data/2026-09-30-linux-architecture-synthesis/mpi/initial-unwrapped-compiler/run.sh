#!/usr/bin/env bash
set -uo pipefail
export PATH=/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
cd /path/to/validation/architecture-synthesis/source
devenv --no-tui shell -- bash ../mpi/run-inside.sh > ../mpi/run-driver.log 2>&1
status=$?; printf '%s\n' "$status" > ../mpi/run.exit
exit "$status"
