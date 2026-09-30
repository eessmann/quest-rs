#!/usr/bin/env bash
set -uo pipefail
export PATH=/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/mpi
nix build --impure --file ./build.nix quest mpi mpiDev --no-link --print-out-paths --max-jobs 1 --cores 3 > paths.txt 2> build.log
status=$?
printf '%s\n' "$status" > build.exit
exit "$status"
