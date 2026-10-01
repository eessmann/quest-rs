#!/usr/bin/env bash
set -uo pipefail
export PATH="/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
cd /path/to/validation/architecture-synthesis/source
devenv --no-tui shell -- bash ../run-workspace-inside.sh
