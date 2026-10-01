# Reproduction and evidence

Run from the Rust checkout through `devenv shell -- ...`. Compiler and dependency
revision details are in `functions/environment.txt`, `sources.sha256`, Cargo.lock
and the audit ledger. These are local Darwin CPU/OpenMP records. No Linux/MPI/GPU
run is implied. Review reports are retained in `reviews/`, including original
findings and their subsequent resolutions. Task reports provide narrower checks.
The [audit ledger](../../2026-10-01-mathematical-audit.md) connects mathematical
claims to source material and records limitations. `validation-summary.json`
provides a machine-readable result index; the compressed final logs are the
execution receipts. Verify source manifests from the repository root with
`shasum -a 256 -c docs/verification/data/2026-10-01-mathematical-audit/sources.sha256`.

## Integrated commands

The shared feature selection was:

```text
quest-rs/qsvt,quest-qsvt-cli/offline-synthesis,quest-qsvt-cli/rayon,quest-qsp/simd,quest-optimizer-worker/synthesis
```

```sh
cargo nextest run --workspace --locked --features "$FEATURES" --profile ci
cargo clippy --workspace --all-targets --locked --features "$FEATURES" -- -D warnings
cargo test --doc --workspace --locked --features "$FEATURES"
cargo doc --workspace --no-deps --locked --features "$FEATURES"
cargo fmt --all -- --check
cargo test --release --locked -p quest-qsp --features offline-synthesis,rayon,simd,artifact -- --ignored --nocapture --test-threads=1
cargo run --locked -p xtask -- check-native-consumers --work-dir target/math-audit-native-consumers --backends cpu,omp
cargo run --locked -p xtask -- generate-qsvt-catalog --source /Users/erich/Projects/quest-qsvt --check
bash benchmarks/architecture/run-functions.sh
```

Set `FEATURES` to the comma-separated selection above. Native-consumer fixtures
require a new empty output directory; change that path when reproducing.
Book compilation used mdBook0.5.4 from the project's pinned nixpkgs revision:
`nix run github:NixOS/nixpkgs/6774f7bc253789b113a4f39285dc0fa100abeacc#mdbook -- build docs/book`.

## Scope and failed intermediate checks

`workspace-nextest-final.log.gz` is the final integrated result. Earlier logs
retain failed storage-boundary/admission assertions and the allocation finding;
they are not represented as successful gates. Allocation backtraces identified
Rayon external-injection blocks and lazy macOS worker condition variables.
The trace harness disabled allocation tracking recursively while collecting
backtraces, so these diagnostic counts identify origins and are not exact total
allocation or maximum-memory certificates. Temporary trace code was removed.
The final test asserts single-worker in-pool zero allocation and verifies
four-worker output values and storage reuse, without a scheduler-zero promise.

`bindings-check.log.gz` records a coverage-manifest mismatch: local headers omit
the conditional MPI declaration and discovery is through QUEST_ROOT, while the
checked-in manifest records it and CMAKE_PREFIX_PATH. Regeneration produced
identical Rust/C++ bindings, names and adapter registry. The coverage manifest
was restored so this CPU run did not discard its MPI record. This full manifest
freshness check remains an explicit platform-specific gap.

The catalog coefficients are unchanged; only its source revision string was
refreshed. `catalog-all-inverse-nlft.json` predates that string-only refresh;
`catalog-check-final.log.gz` verifies all21 payloads against the requested current
C++ revision. Construction success is not a proof of every family's reciprocal
approximation error or a native solve for each family.

## Function measurements

`functions/` records three release trials of the same log(1+x²) expression,
counting allocations and wall-clock nanoseconds for construction, scalar value,
scalar jet and interval jet. `run-functions.sh` prewarms dependencies and both
example configurations, then rebuilds each leaf after an mtime-only change.
Compilation times are warm leaf rebuilds, not clean whole-workspace builds.
Binary sizes include the executable wrapper and counting allocator. One
expression cannot establish aggregate monomorphization/code-size costs for a
large application. Source and executable hashes accompany the measurements.
