#!/usr/bin/env bash
set -uo pipefail
export PATH="/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:$PATH"
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
printf '%s\n' 'nix shell github:NixOS/nixpkgs/6774f7bc253789b113a4f39285dc0fa100abeacc#mdbook --command mdbook build docs/book' > ../receipts/final-book.command
date -u +%FT%TZ > ../receipts/final-book.started
nix shell github:NixOS/nixpkgs/6774f7bc253789b113a4f39285dc0fa100abeacc#mdbook --command mdbook build docs/book > ../receipts/final-book.log 2>&1
check_exit=$?
printf '%s\n' "$check_exit" > ../receipts/final-book.exit
date -u +%FT%TZ > ../receipts/final-book.finished
exit "$check_exit"
