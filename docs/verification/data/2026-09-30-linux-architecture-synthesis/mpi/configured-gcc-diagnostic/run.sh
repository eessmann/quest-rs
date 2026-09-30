#!/usr/bin/env bash
set -uo pipefail
export PATH=/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
devenv --no-tui shell -- bash ../mpi/run-inside.sh > ../mpi/run-driver.log 2>&1
status=$?; printf '%s\n' "$status" > ../mpi/run.exit
exit "$status"
