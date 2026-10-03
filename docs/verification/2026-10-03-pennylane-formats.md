# PennyLane catalog and standard formats, 2026-10-03

This record covers the runtime catalog migration and the maintained format audit.
The [audit inventory](../book/src/format-handling.md) distinguishes common-format
replacements from retained domain grammars and historical evidence.

## Source and numerical contracts

The unchanged PennyLane `inverse.h5` is 970,233 bytes with SHA-256
`dccb518a24395d73af9ab701a922431f4600f553e904cc57507b49863a6d30a3`.
The explicit fetch command downloaded and validated that snapshot successfully;
the offline check admitted all 21 families. Every original coefficient-array
hash matches, including the independent degree-8105 QSP fixture. The manifest
records source URLs, retrieval date, interpretation, author and dataset license.

Catalog tests cover exact lookup, sorting, binary64 words, odd support, malformed
rank/type, missing groups, corrupt input, nonfinite values, per-family and
aggregate limits, and continued operation after failure. Regression tests reject
symbolic/external links, external raw storage and virtual datasets before reads,
and reject oversized family storage before allocation. Owned coefficients remain
usable after the source file is removed.

JSON tests cover duplicate fields, conflicting payloads, matrix provenance,
complex pairs, fixed shapes, signed zero, subnormals, full-width integer words,
metadata and nesting limits. Existing artifact digests and independent
recertification tests pass. Bounded output tests verify exact byte boundaries,
early serializer termination and bounded amortized buffer growth.

## Rust validation

Using installed QuEST 4.3.0 and system serial HDF5, with `HDF5_DIR` unset:

```sh
export QUEST_ROOT=/path/to/installed/quest
cargo test --offline --workspace
cargo test --offline -p quest-qsvt-cli --no-default-features
cargo test --offline -p quest-qsvt-cli --no-default-features --features rayon
cargo test --offline -p quest-qsvt-io --all-features
cargo run --offline -p xtask -- check-qsvt-catalog
cargo run --offline -p xtask -- fetch-qsvt-catalog
cargo fmt --all --check
git diff --check
```

The workspace suite passed 1,118 tests with two existing ignores, including
native tests and doctests. The initial sandbox run could not initialize an MPI
OFI endpoint; the native lifecycle test and full workspace rerun passed outside
that restriction. The no-default-feature and Rayon CLI suites passed separately.
Cargo's `--offline` prevents registry access; the explicit fetch subcommand still
performs its intended dataset download.

Strict workspace Clippy reached an existing `clippy::too_many_lines` diagnostic
in the unchanged `quest-macros/src/frontend.rs` `emit` function (106/100 lines).
The complete check passed with only that lint excepted:

```sh
cargo clippy --offline --workspace --all-targets -- -D warnings -A clippy::too_many_lines
```

Changed IO and tooling targets also passed their focused Clippy checks without
that exception. The nightly compiler still emits its existing
`generic_const_exprs`/next-solver compatibility notice.

## Tooling and measurement boundaries

Native-consumer TOML tests passed for table selection, comments, literal strings,
escapes, duplicate keys and dependency paths containing spaces, quotes,
backslashes and non-BMP Unicode. Generated Cargo manifests passed metadata and
consumer checks. CSV correctness checks parsed quoted fields, commas, newlines,
Unicode, full-width integers and preserved column order/LF termination. Formatting
occurs after timing and allocation snapshots.

The repeat audit also migrated the remaining generalized-catalog and QSVT
benchmark JSON reports. Two report regressions passed for escaped text, numeric
bound bits and nullable degree; the benchmark compiled and its serialization
remains outside Criterion timing. No further maintained handwritten common-format
syntax was found within the documented audit boundaries.

An isolated Python environment installed the declared fixture requirements. The
new manifest round-trip test and eight existing QSP controller tests passed.
The active static-architecture project launcher completed `--prepare-only`, and
the standalone multiprecision fixture compiled with its updated lockfile. These
are format checks, not new benchmark measurements.

The Glaze report serializer compiled against Glaze 6.0.0 and its CTest smoke test
passed, covering report schemas, escaping, optional fields, complex pairs,
nonfinite rejection and failed writes. Full C++ adapters could not configure:
the available upstream revision `4fc35983138d07a990862a4d83ad16f2b737c98f` differs
from the required `7fe7f740579b03c52a8cf48be6a31268b029c19f`. No full native adapter
execution is claimed. The existing revision pin remains enforced.

## Packaged offline consumer

Packaged `quest-build`, `quest-numerics`, `quest-polynomial`, `quest-qsp` and
`quest-qsvt-io` together with `cargo package --offline --allow-dirty --no-verify`.
The `--no-verify` phase only creates archives: independent verification then
extracted those archives and built a new consumer outside the checkout, patching
unpublished workspace dependencies only to the extracted packages. The archive
included the unchanged HDF5 file, typed manifest and attribution notice; final
catalog/JSON/artifact sources matched the workspace.

Both the ordinary and no-default-feature consumer builds passed offline with
`HDF5_DIR` unset. Copied binaries then ran with the repository and extracted
sources unavailable, `HDF5_DIR` and `LD_LIBRARY_PATH` unset, and network syscalls
denied through a seccomp filter. A socket probe confirmed the network denial.
Both runs checked all 21 coefficient hashes, 38,124 coefficients, converted
polynomial bits, degree 8105, exact and near-miss lookups and source SHA-256.
This verifies bundled-data inclusion and native HDF5 runtime discovery.

An isolated unprivileged run with an unwritable `TMPDIR` returned the expected
IO permission error. Successful loads left no staged catalog file. This is
explicit catalog-loading behavior, not a build-time download requirement.
