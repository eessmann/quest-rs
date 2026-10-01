#!/usr/bin/bash
set -u
export PATH=/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=3
cd /path/to/validation/architecture-synthesis/source
receipt_dir=../receipts
trap 'printf "%s\n" "$?" > "$receipt_dir/qsp-driver.exit"' EXIT
{
 date -u --iso-8601=seconds
 uname -a
 lscpu
 free -b
 printf 'CARGO_BUILD_JOBS=%s\n' "$CARGO_BUILD_JOBS"
 printf 'PATH=%s\n' "$PATH"
} > "$receipt_dir/qsp-machine.log" 2>&1
run_check() {
 local name="$1"
 shift
 printf '%q ' "$@" > "$receipt_dir/$name.command"
 printf '\n' >> "$receipt_dir/$name.command"
 date -u --iso-8601=seconds > "$receipt_dir/$name.started"
 "$@" > "$receipt_dir/$name.log" 2>&1
 local result=$?
 printf '%s\n' "$result" > "$receipt_dir/$name.exit"
 date -u --iso-8601=seconds > "$receipt_dir/$name.finished"
 return "$result"
}
run_check qsp-environment devenv shell -- /usr/bin/bash -c 'rustc -vV; cargo -V; env | sort | grep -E "^(OMP_|QUEST_|CARGO_BUILD_JOBS|CC=|CXX=|CMAKE_PREFIX_PATH=)"' || exit "$?"
run_check qsp-all-features devenv shell -- cargo test -p quest-qsp --all-features --locked || exit "$?"
run_check qsp-large-release devenv shell -- cargo test -p quest-qsp --all-features --release --lib --tests --locked -- --ignored --nocapture || exit "$?"
run_check qsp-native-cli devenv shell -- cargo test -p quest-qsvt-cli --features offline-synthesis,rayon --locked || exit "$?"
run_check qsp-all-features-lint devenv shell -- cargo clippy -p quest-qsp --all-features --all-targets --locked -- -D warnings || exit "$?"
run_check qsp-native-cli-lint devenv shell -- cargo clippy -p quest-qsvt-cli -p quest-qsvt-io --features quest-qsvt-cli/offline-synthesis,quest-qsvt-cli/rayon --all-targets --locked -- -D warnings || exit "$?"
