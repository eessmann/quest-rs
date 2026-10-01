#!/usr/bin/bash
export PATH=/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=6
cd /path/to/validation/architecture-synthesis/source
run() {
 name="$1"
 shift
 printf '%q ' "$@" > "../receipts/$name.command"
 printf '\n' >> "../receipts/$name.command"
 devenv --no-tui shell -- "$@" > "../receipts/$name.log" 2>&1
 printf '%s\n' "$?" > "../receipts/$name.exit"
}
run worker-engines2 cargo test --locked -p quest-optimizer-worker --features synthesis,zx,mitm -- --test-threads=1
run worker-clippy2 cargo clippy --locked -p quest-optimizer-client -p quest-optimizer-worker --features quest-optimizer-worker/synthesis,quest-optimizer-worker/zx,quest-optimizer-worker/mitm --all-targets
