# Workspace quality and consolidation — 2026-10-08

Implementation follows the [approved plan](../plans/2026-10-08-workspace-quality.md).
The [23-package review ledger](2026-10-08-workspace-review.md) identifies the
inspected boundaries and independent verification retained. The
[migration notes](2026-10-08-workspace-migration.md) describe caller-visible changes.

## Changes and review

MathCore now owns neutral identities, errors and finite dyadic import. Lowered
kernels capture arithmetic profiles and support reusable RAII execution scratch.
Polynomial evaluators share support/work admission; negative Laurent powers use
scaled reciprocals, while positive zero padding stays in Horner order. Finite
QASM conditions use explicit bool conversions.

Matching execution admits an owned native scratch register on every rank and
uses documented QuEST operations. First-party Rust inherits a safety lint across
all targets; necessary audited CXX/MPI FFI remains in `quest-sys`. Safe external
instrumentation preserves allocation tests, including cross-thread measurements.

Native requests and evaluated configurations are separate. Clap, Cargo metadata
and Rust compiler metadata replace generic handwritten parsing. Explicit Cargo
emission retains native file and pkg-config environment invalidation. QuEST and
HDF5 discovery remain quiet; `native-doctor` isolates upstream MPI probe output.
HDF5 continues to use `hdf5-metno`. The narrow fallible CMake
launcher remains for the limitations documented in the plan.

The gate registry generates mechanical adapters. VM and compiler responsibilities
have separate modules, and CFD shares checked storage/normalization helpers and
reuses RK4 intermediate storage. Independent mathematical oracles and scientific
reference calculations remain separate.

Independent source review found and resolved three production regressions:
positive-power factoring lost representable results, quiet HDF5 discovery lost
target-qualified environment watches, and request round trips lost native input
watches. Executable regressions failed before each correction. Benchmark review
also corrected completion identity checks, final source guards, raw-sample
integrity, interruption cleanup and timeout phase reporting. Neither review is
a substitute for the execution results below.

## Local execution

The [acceptance receipt](data/2026-10-08-workspace-quality/acceptance.json)
links sanitized result excerpts and failing-before-fix regression evidence. The
[source manifest](data/2026-10-08-workspace-quality/acceptance-source.json)
identifies the final workspace crate/configuration files. The receipt records
the small lint-only delta between full-suite execution and final focused verification.

The selected platform is native Linux x86-64, Rust nightly 1.101.0
(`8d1a76430`), Cargo nightly 1.101.0, nextest 0.9.143, CMake 4.3.0,
QuEST 4.3.0, MPICH 4.2.2 and serial HDF5 1.14.6. HDF5 uses the supported
standard Linux fallback on this installation; no `hdf5.pc` is available in the
default pkg-config search path.

All heavy builds/tests/measurements run locally with a shared serialization lock
inside `systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G`.
Temporary compiler files and targets use disk-backed workspace storage. An
initial baseline build exhausted the separate tmpfs quota; its failed output was
retained, and later builds use disk-backed targets. This was an infrastructure
failure, not a passing baseline test run.

| Check | Result |
| --- | --- |
| Final default workspace nextest | 1,538 passed; one explicitly ignored test |
| All-feature doctests | 67 passed; one ignored example |
| Final all-feature workspace nextest | 1,733 passed; six explicitly ignored tests |
| Strict all-target/all-feature Clippy | Passed with `-D warnings` |
| Explicit release-scale tests | Five passed; one failed at the documented 64 MiB certificate limit (exit 100) |
| Generated binding freshness | Passed |
| Installed CPU/OpenMP consumers with isolated loader | Blocked before build by fresh offline dependency resolution (exit 1) |
| Formatting | Passed |
| Final native failure/owned-scratch regressions | Two passed after lint-only corrections |

The default and all-feature suites include the new mathematical and build
regressions, allocation contracts, ownership compile-fail tests, compiler and
language contracts, and local native execution. Matching's owned scratch passed
whole-unitary, adjoint, budget and failure-path checks including 1/2/4/8 ranks
and split communicators. This does not establish multi-node capacity closure.

The fixed 64 MiB CFD certificate trial retains its documented capacity rejection:
`certification resource budget: modeled verification memory`. The same failure
and admission formula appear in the
[earlier fixed-budget receipt](2026-10-06-resolved-nonlinear-box.md); its fixture,
distributed-history gate and QSP certification implementation are unchanged by
this pass. The separate, already-defined higher-budget trial passed at one and
two ranks. No limits were raised to turn the fixed-budget rejection into a pass.

The independent consumer fixture could not generate a fresh offline lockfile:
the cached resolver rejects yanked `private-gemm-x86` 0.1.20, required through
`faer` 0.24.4. Existing workspace-lockfile builds and native tests remain separate
passing evidence. This fixture reached neither consumer compilation nor isolated
loader execution. Its failed lock log is retained; dependency versions, caches
and fixture locking semantics were not changed to bypass that outcome.

The Cargo checks use `--offline --locked`. The main invocations are
`cargo nextest run --workspace`, the same command with `--all-features`,
`cargo test --workspace --all-features --doc`, and
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.
The release gate explicitly runs ignored tests with
`cargo nextest run --release --workspace --all-features --run-ignored only`.
Native acceptance uses `cargo run -p xtask -- generate-quest-bindings --check`
and `cargo run -p xtask -- check-native-consumers --backends cpu,omp --loader-isolated`.
Set `QUEST_ROOT` to the installed QuEST prefix and `MPICC` to its matching MPI
wrapper; execute these commands inside the memory-limited scope above.

## Initial benchmark accounting

The [replication harness](../../benchmarks/replication/README.md) uses actual
`cargo nextest bench`, exact IEEE754 fixture transport, correctness preflight,
separate baseline/current targets and explicit terminal outcomes. The pinned
62-case SoftwareX corpus is verified before extraction. Rust stage timings,
native source solver/kernel boundaries, and source capability diagnostics remain
distinct; numerical unitary admission does not stand in for a certified proof.

The completed combined inventory contains 811 terminal rows: 183 native source workloads,
310 original and 310 current Rust stage measurements, and four original plus
four current numerical unitary measurements. The 33 extra-suite capability
diagnostics add no workloads. The
[completion receipt](data/2026-10-08-workspace-quality/benchmark-completion.json)
records complete accounting and incomplete successful performance coverage.

| Outcome, derived from verified raw logs | Rows |
| --- | ---: |
| Successful measurements | 560 |
| Native dependency/configuration failures | 183 |
| Rust modeled resource-limit rejections | 50 |
| Numerical accuracy/preparation failures | 14 |
| Strict contractivity admission not established | 4 |

Each Rust stage lane has 276 successful measurements and the same 34 unsuccessful
outcomes. Both unitary lanes pass all four dimensions with zero Gram residual.
Raw status buckets remain unchanged; the receipt separately classifies numerical
admission and structured budget errors from their hashed diagnostic logs. A
contractivity upper bound above one does not establish a violation, and modeled
byte-budget rejection is not evidence of an operating-system OOM.

The [comparison CSV](data/2026-10-08-workspace-quality/benchmark-comparison.csv)
contains 314 matched case records, including 280 pairs with timing samples.
Medians are recomputed from hashed raw Criterion samples. Observed original/current
median ratios span 0.9413–1.0522 across successful cases: measured medians include
both faster and slower current results. These descriptive ratios establish
neither aggregate speedup nor statistical significance. For example, the degree-127
industrial filter full pipeline measured 269.205 µs originally and 268.217 µs
currently, with the same approximately 1.186e-15 preflight residual.

The [provenance receipt](data/2026-10-08-workspace-quality/benchmark-provenance.json)
pins sources, fixtures, protocols, executed controller snapshots and artifact
hashes. The final unitary lanes were rerun from captured controller/helper files
and verified unchanged afterward. Earlier attempts remain separate and excluded.
Raw samples and logs remain under `target/quality-campaign/`; peak RSS denotes the
largest child process across build, discovery, preflight and measurement, not
aggregate cgroup memory or steady kernel allocation. Allocation tests are separate
evidence. The historical 1,488-row campaign contributes no rows to this result.

Final independent artifact review revalidated all 811 terminal identities,
sample/log/RSS hashes, source and controller identities, and every published
paired median and ratio. All 22 controller tests passed. Production and benchmark
reviews have no unresolved P1/P2 findings; the execution limits above remain
separate from those review dispositions.

The initial installed-dependency attempt was blocked by the missing required
`autodiff` 1.1.2 package. Its 183 native rows retain that prerequisite failure,
with the actual configuration log. The subsequently authorized
[devenv follow-up](2026-10-08-native-devenv.md) uses the existing locked native
environment and records its results separately. Upstream sources are unchanged; missing native fixtures and
contracts are not replaced by approximate Rust workloads. Criterion distributions
remain separate from the original coordinate benchmark's three-sample protocol.

## Limits

Explicit GPU backend validation, macOS and multi-host execution were not
performed in the initial workspace acceptance. The native devenv follow-up
records compiled capabilities without inferring automatic backend deployment.
Matching now needs the admitted second native register, so historical borrowed
communication-array capacity measurements do not apply. The source-package
unit tests in `quest-build` require the workspace's `quest-sys` FFI fixture
assets; ordinary installed library consumers have a separate acceptance gate.
Arbitrary-precision scalar operations and RK4 drift callbacks may allocate.
The initial failure accounting alone establishes neither successful native
measurements nor whole-node capacity. The separate devenv follow-up records
actual native benchmark observations and the remaining limits.
