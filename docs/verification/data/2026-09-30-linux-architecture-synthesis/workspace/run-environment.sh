#!/usr/bin/env bash
set -uo pipefail
export PATH="/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
devenv --no-tui shell -- bash -c 'set -eu; rustc -Vv; cargo -V; cargo nextest --version; cmake --version; printf "QUEST_ROOT=%s\nHDF5_DIR=%s\n" "$QUEST_ROOT" "$HDF5_DIR"; uname -a; lscpu; free -h' > ../receipts/environment.log 2>&1
status=$?
printf '%s\n' "$status" > ../receipts/environment.exit
exit "$status"
