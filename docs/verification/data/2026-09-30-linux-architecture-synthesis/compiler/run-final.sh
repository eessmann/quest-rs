#!/usr/bin/env bash
set -uo pipefail
export PATH=/home/user/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=3
cd /path/to/validation/architecture-synthesis/source
run() {
 label=$1; shift
 printf '%q ' "$@" > "../compiler/$label.command"; printf '\n' >> "../compiler/$label.command"
 "$@" > "../compiler/$label.log" 2>&1
 printf '%s\n' "$?" > "../compiler/$label.exit"
}
sha256sum Cargo.toml Cargo.lock devenv.nix crates/quest-circuit/Cargo.toml crates/quest-circuit/tests/generator_error_contract.rs crates/quest-compile/src/workers.rs crates/quest-compile/src/generator.rs crates/quest-compile/src/beam.rs > ../compiler/final-source.sha256
run focused-green devenv --no-tui shell -- cargo nextest run --locked -p quest-circuit --all-features --no-fail-fast --test-threads 1 -E 'binary(beam_contract) | binary(beam_approx_contract) | binary(beam_mitm_contract) | binary(generator_error_contract)'
run facade-final devenv --no-tui shell -- cargo nextest run --locked -p quest-circuit --all-features --no-fail-fast --test-threads 1
run clippy-final devenv --no-tui shell -- cargo clippy --locked -p quest-circuit -p quest-compile --all-features --all-targets -- -D warnings
printf 'complete\n' > ../compiler/run.complete
