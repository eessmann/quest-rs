# Large-polynomial workspace acceptance — 2026-10-08

The final resource redesign passed the default and all-feature workspace test,
documentation and strict lint matrix. The default tests were repeated against the
final source after the feature-specific import correction: **1,556 passed**.
The all-feature run passed **1,753 tests**, including local MPI integration tests.

This record covers the [approved resource/workspace migration](../plans/2026-10-08-large-polynomial-benchmarks.md).
The [API migration guide](../resource-api-migration.md) describes ownership and
budget boundaries. [Large-polynomial capacity verification](2026-10-08-large-polynomial.md)
records the separate native inverse and independent forward capacity checks.
Earlier workspace-quality and migration records remain unchanged.

## Executed matrix

All commands completed with exit code zero. Native builds and tests ran locally
on `x86_64-unknown-linux-gnu`, using installed QuEST 4.3.0 and matching MPICH.
The all-feature build explicitly verified that the Rust MPI compiler wrapper
matched QuEST's MPI ABI and loaded library. The compiler was
`rustc 1.101.0-nightly (8d1a76430 2026-10-06)`, with LLVM 23.1.3.

| Command | Observed result |
| --- | --- |
| `cargo nextest run -j2 --workspace --build-jobs 2` | Final repeat: 1,556 passed, 1 explicitly skipped; initial run also passed |
| `cargo nextest run -j2 --workspace --all-features --build-jobs 2` | 1,753 passed, 6 explicitly skipped; one slow MPI matching test passed |
| `cargo test -j2 --workspace --doc` | 57 passed, 0 failed, 1 ignored example |
| `cargo test -j2 --workspace --all-features --doc` | 67 passed, 0 failed, 1 ignored example |
| `cargo clippy -j2 --workspace --all-targets -- -D warnings` | Exit 0 |
| `cargo clippy -j2 --workspace --all-features --all-targets -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Fresh final check: exit 0 |
| `git diff --check` | Exit 0 |

Heavy commands used the following envelope; path placeholders identify the
installed dependencies and workspace without publishing personal machine paths:

```sh
systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  flock /tmp/quest-quality-build.lock \
  env TMPDIR="<workspace>/target/quality-tmp" \
      QUEST_ROOT="<installed-system-quest>" \
      MPICC="<matching-mpich-wrapper>" \
      CARGO_TARGET_DIR="<workspace>/target/large-polynomial" \
  cargo <command-and-options-from-the-table>
```

Cargo build jobs and nextest test concurrency were both limited to two. Temporary
storage was disk-backed. Every check wrote separate command, start, finish, exit
and full-log receipts. Start timestamps include time queued behind the shared
build lock. Existing acceptance directories were preserved. The compiler's
existing generic-const-expression/next-solver compatibility notices remain in
the logs; both strict Clippy commands returned zero.

## Portable receipts and tested source

The [compact check receipt](data/2026-10-08-large-polynomial/workspace/checks.json)
contains all nine successful captures, exact Cargo/Git arguments, exit codes,
result lines, timestamps and SHA-256 hashes of the original complete logs.
Complete local captures remain under
`target/large-polynomial-resource-verification/`. The compact receipt replaces
personal dependency/workspace paths with placeholders; the original log hashes
identify those retained captures.

The [source index](data/2026-10-08-large-polynomial/workspace/source-index.json)
records hashes for 772 workspace Rust sources, crate Cargo manifests, root
Cargo manifests/lock, toolchain and nextest configuration files. Its aggregate
is the SHA-256 of the sorted path-to-hash map serialized as compact JSON. Its
scope excludes binary fixtures and installed native dependencies; their separate
identities belong to the capacity and benchmark records.

| Receipt identity | SHA-256 |
| --- | --- |
| Compact check receipt | `e58a169bbaa096d530b312644e70fec62297acac4030f2d721bde948c2eea7a6` |
| Source-index file | `d7d1c3f9ad60612c3ddc038389b89c365a8dba1396a79fb061462f00651d86df` |
| Aggregate source-file map | `e3c54c852e4785e7fb1dcf8a6d29102c291af8c6cfa410be4ceadd90294a33dd` |

Initial compilation failures are preserved in separate local rejection receipts.
They exposed missed QSVT/QSVT-IO/CFD flat-policy callers and an overbroad change
to three unrelated domain limits. The common-policy callers now use independent
shape/storage/work fields; the three domain-limit changes were restored to their
pre-task bytes. Independent review found those final caller migrations clean.

Default compilation also exposed an unused `HalfEven` import when its offline
and test-only operations were absent. The matching import cfg was applied only
after the staged pilot-v1 atomic completion confirmed all 171 receipts. No
numerical expression changed. The portable check receipt retains the correction's
before/after source hashes and original pilot-completion hash. The staged pilot
was preserved in full, and the default workspace tests were then repeated on the
corrected source. No later Rust source change was needed for this matrix.

## Explicit exclusions

The default run skipped the degree-8192 cold interval-FFT certification fixture.
The all-feature run also skipped these separately invoked scale campaigns:

- QSP offline degree-8105 catalog export/certification.
- QSP parallel degree-8105 catalog bit-preservation acceptance.
- CFD bounded 243-node nonlinear history inverse.
- CFD fixed-budget resolved nonlinear history inverse at one/two ranks.
- CFD separately approved higher-budget nonlinear history inverse at one/two ranks.

These six ignored tests were not executed by this matrix. The ignored doctest is
the QuEST crate README's `build.rs` runtime-path-emission excerpt. Executed
documentation tests include compile-fail and no-run contracts as marked; their
passing count is not a count of native runtime experiments.

Resource regressions cover ownership release, failed admission and allocation,
overflow, cumulative work across calls/stages/retries, shared sequence lifetimes,
plan/scratch reuse and deterministic parallel batch admission. Modelled bytes
remain distinct from process RSS. Generic MathCore contexts and Remez/root-proof
drivers retain separate counters/storage contracts, as disclosed in the migration
guide; a single resource report does not aggregate them with NLFT.

This matrix establishes the recorded local regression and lint results. It does
not establish certified million-degree completion, arbitrary input capacity,
GPU execution, other operating systems, multi-host scaling or performance
speedups. Matched benchmark adapter checks, measurement identities and publication
observations have their own receipts.
