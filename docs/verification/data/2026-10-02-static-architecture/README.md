# Static architecture evidence

Baseline: `001a2b656a5a80a60659a408f87f57670309a09b`. The
[migration guide](../../2026-10-02-static-architecture-migration.md) describes API
and representation changes; the [removal/retention ledger](removal-retention-ledger.md)
records each workspace layer's purpose. This pass builds on the previous
[paper and capability audit](../2026-10-01-mathematical-audit/README.md).
The [numerical report](numerical-report.md) and [project report](project-report.md)
give final measurements, full observed ranges, accuracy checks and retained costs.
Their raw receipts are under `numerical/` and `project/`; source hashes in each
record distinguish the archived baseline from the uncommitted current source.

These are aarch64 Darwin CPU/OpenMP observations. Linux process workers, MPI and
accelerator execution were not available. The C++ checkout and Zotero library
remain read-only. Independent verification is retained to detect mathematical and
numerical mistakes; a hostile environment is not the design premise.

## Reproduction

Use the repository's locked `devenv shell`, `CARGO_BUILD_JOBS=2`, and this supported
feature selection:

```sh
FEATURES='quest-rs/qsvt,quest-rs/workers,quest-rs/serde,quest-rs/codespan-reporting,quest-rs/ndarray,quest-optimizer-worker/synthesis,quest-optimizer-worker/zx,quest-optimizer-worker/mitm,quest-qsp/offline-synthesis,quest-qsp/rayon,quest-qsp/simd,quest-qsvt-cli/offline-synthesis,quest-qsvt-cli/rayon'
cargo nextest run --workspace --locked --features "$FEATURES" --profile ci
cargo test --doc --workspace --locked --features "$FEATURES"
cargo clippy --workspace --all-targets --locked --features "$FEATURES" -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked --features "$FEATURES"
cargo check -p quest-qsvt-cli --no-default-features --all-targets --locked
cargo test -p quest-numerics --no-default-features --test kernels --test observer --locked
cargo fmt --all -- --check
mdbook build docs/book
cargo run --locked -p xtask -- generate-quest-bindings --check
cargo test --release -p quest-qsp --all-features --lib --test offline --test parallel --locked -- --ignored --nocapture --test-threads=1
cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp --work-dir target/static-native-consumers
```

Use a fresh native-consumer work directory. The numerical and project performance
fixtures have separate runners and explicit mathematical checks; see
[numerical](../../fixtures/static-architecture/numerical/README.md) and
[project](../../fixtures/static-architecture/project/README.md) instructions.
Run each in a quiet window without other builds, and retain all three interleaved
trials. Performance evidence does not replace correctness verification.

The exact compiler revision is in `toolchain-final.txt`. The compiler emits its
known `generic_const_exprs`/next-trait-solver compatibility warning and uses the
coherence solver for affected crates; this incomplete nightly feature is retained
explicitly and that compatibility warning remains visible in the receipts.
The development shell now includes mdBook from the existing pinned nixpkgs input.

## Interpreting receipts

`validation-summary.json` indexes completed checks. `capture-manifest.json` records
hashes before and after local path redaction; `.log.gz` files are compressed
execution receipts. Source hashes describe the final production source and
reproduction fixtures, not generated build output. The declaration inventory is
a line-based review aid, not a proof that every macro-generated type was examined.

Independent review reports retain findings and their resolutions. In particular,
the Remez review explicitly supersedes its initial hostile-custom-backend framing
with ordinary documented arithmetic laws, one-time candidate support admission,
and cached hot-path evaluation. The final source does not rescan support on every
evaluation. The final additional polynomial cleanup replaces clone-and-add-zero
input validation with `Backend::validate`.

The first complete default workspace run recorded two migration maintenance
failures: the numerical arithmetic module needed admission in the dependency
boundary test, and a non-Send compile-fail snapshot needed its changed native
matrix ownership trace. Both were corrected and independently rerun. The later
supported-feature workspace run passed all selected tests. Earlier source edits
and concurrent-build failures are not represented as passing final gates.

One preexisting rustdoc bracket was marked as code in `quest-math`; exact-ring
algorithms were unchanged. Book generation initially lacked its executable in the
project environment; the pinned package addition resolves that reproducibility
gap. Binding freshness reflects this host's actual serial header inventory;
it is not a statement about MPI declaration availability elsewhere.

From the repository root, verify the final source snapshot with:

```sh
shasum -a 256 -c docs/verification/data/2026-10-02-static-architecture/sources.sha256
```

These hashes describe the measured snapshot. The later
[dependency follow-up](../../2026-10-02-static-architecture.md#dependency-follow-up)
changes two manifests and the lockfile, so those entries intentionally differ
from the present checkout. The original benchmark receipts remain unchanged.

The numerical report's intermediate assembly diagnosis is retained under
`numerical/intermediate-diagnostic/`; it is not a final-source timing claim.
The project fixture's initial compilation mismatch was corrected before its
fresh accepted run. The unchanged project build really recompiles native-dependent
crates in both revisions; those receipts are not labeled no-op builds. Exact
Cargo invalidation causes were not established by these measurements.
