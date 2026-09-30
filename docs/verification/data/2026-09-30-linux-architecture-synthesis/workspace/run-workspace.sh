#!/usr/bin/env bash
set -uo pipefail
export PATH="/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
devenv --no-tui shell -- bash ../run-workspace-inside.sh
