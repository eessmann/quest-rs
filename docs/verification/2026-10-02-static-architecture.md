# Static numerical architecture and workspace consolidation

This breaking pass replaces the numerical expression interpreter and the two
Remez implementations with one statically dispatched architecture. Baseline:
`001a2b656a5a80a60659a408f87f57670309a09b`. The
[migration guide](2026-10-02-static-architecture-migration.md) describes caller
changes. The [per-layer ledger](data/2026-10-02-static-architecture/removal-retention-ledger.md)
explains what was removed and why remaining boundaries exist.

The acceptance criteria are mathematical correctness, numerical performance,
clear ownership and reproducibility. Custom backends are ordinary scientific
extensions expected to implement documented arithmetic laws. Independent checks
protect against numerical and implementation errors; hostile certificate forging
is not a new design requirement.

## Numerical architecture

- `Function<E>` and canonical `function!` retain concrete expressions. Value,
  first derivative, second derivative and vector Jacobians come from one generic
  AD implementation. Binary64, outward binary64 intervals, MP points and MP
  intervals share capabilities in `quest-numerics`; non-Copy scalars and intervals
  without total ordering are supported.
- Exact captured binary64 values, integers, rationals and decimal-source inputs
  enter the chosen backend directly. Directed Astro-float intervals are separate
  from the QSP verifier, with explicit domains, enclosed trigonometric extrema,
  exponent/precision limits and conservative phase widening. Independent Rug/MPFR
  tests corroborate endpoint inclusion on cancellation, subnormal and
  transcendental cases.
- One owning Remez request retains the original target and exact domain through
  exchange, proof, export, retries and failure. Uniform error and minimax gap are
  separate certificates selected independently of precision. Pivoted faer QR and
  scaled pivoted MP Householder QR share orchestration and residual/rank contracts.
- Generic Newton, Krawczyk and scalar/vector Hansen–Sengupta are provided alongside
  a deterministic scalar root-cover driver. Exclusion, existence, at-most-one, uniqueness, continua and
  unresolved boxes are distinct. Exhaustion preserves partial coverage and cannot
  grant a complete certificate.
- Static degree and vector dimensions are checked in primary types. Const
  structural metadata supports admission; adaptive precision, runtime dimensions,
  convergence and resource budgets remain runtime obligations. Large workspaces
  stay on bounded heap storage.

Two useful precision witnesses passed: exact `1/3` can be approximated and
certified in MP at uniform tolerance `1e-50`, while its binary64 export fails that
tolerance; an MP cubic approximation to `exp` establishes a minimax gap at
`1e-35`. Higher proof precision does not conceal coefficient-export error.

Independent review corrected branch-budget admission, partial-cover preservation,
MP operand admission, simultaneous vector-workspace accounting, AD input admission,
retained retry-history storage and candidate/coverage lifetime alignment. Early
performance measurements exposed unnecessary eager queue allocation and slow
binary64 AD constant paths; these were corrected before final measurement.
Polynomial support is admitted once, cached, and reused. Input validation uses
`Backend::validate` without clone-and-add-zero arithmetic.
The QR storage model includes faer's padded layouts and published scratch sizes;
factorization scratch is released before solve scratch. Its conservative cubic
work estimate is reserved in a batch instead of an additional cubic counting
loop. Nested limits and custom visit behavior are preserved. Small AD operations
and the typed expression entrypoint have ordinary inlining hints, retaining all
arithmetic checks.

## Compiler and native boundaries

`quest-compile` replaces the removed `quest-circuit` facade and owns macro exports.
Forwarding compiler modules, redundant aliases, erased errors, JSON-valued
compiler evidence, native-synthesis relay and generator bootstrap compatibility
were removed. Direct finite semantic/SSA insertion replaces reconstructed ASTs
and placeholder captures. Typed capture banks retain exact angles, effects,
source order and capture-once behavior.

Native preparation uses one dispatch-derived matrix resource pool per prepared
owner. Separate owners keep separate lifetimes and reservations. `QUEST_ROOT`
is the sole explicit installation selector; conventional CMake discovery,
foreign ABI checks, RAII and actual runtime dimensions remain necessary.
Only changed publication/template representations advance to version 2. Worker,
scientific and QSP interchange versions remain unchanged.

Inverse NLFT remains the QSP/GQSP default; RHW remains explicitly selectable.
The independent QSP verifier arithmetic, exact-angle symbolic semantics and
exact circuit algorithms were not merged into the generic numerical core.
A documentation-only bracket correction in `quest-math` does not change its
exact-ring operations. The C++ checkout and Zotero library were not modified;
the [previous mathematical audit](2026-10-01-mathematical-audit.md) retains their
paper/capability comparison.

## Validation

On aarch64 Darwin with project-local QuEST 4.3.0 CPU/OpenMP and rustc revision
`6bb1652a020e80cef79332741d89e996d71933c9`:

| Check | Result |
| --- | --- |
| Supported-feature workspace | 989 passed; 8 skipped |
| Workspace doctests, including compile-fail contracts | 56 passed; 1 preexisting ignored example |
| Large QSP release regressions | 3 passed: degree 8192 and degree 8105 offline/serial/parallel cases |
| Final polynomial integration after small review fixes | 26 passed |
| Scalar kernels/observers without default features | 16 passed |
| Strict all-target workspace Clippy, rustdoc, formatting, book | Passed |
| Pure CLI, compiler feature isolation, renamed macro consumer | Passed |
| Binding freshness and native CPU/OpenMP/installed consumers | Passed |

The eight ordinary-suite skips are the three separately executed large-degree
cases and five Linux-only process cases. The initial default suite's two migration
maintenance failures were fixed and rerun before the clean supported-feature run.
The [evidence index](data/2026-10-02-static-architecture/README.md) supplies exact
commands, receipts, source hashes, review reports and limitations. The nightly
compiler's known incomplete `generic_const_exprs` solver warning remains visible.

Linux workers/native execution, MPI and accelerators were unavailable; no result
here establishes those platforms. Application resource budgets do not claim to
bound opaque transcendental-library internals or allocator metadata. Sampled
MPFR comparisons and code review supplement mathematical enclosure arguments;
they are not a formal proof of an upstream library.

## Performance

The checked-in [numerical fixture](fixtures/static-architecture/numerical/README.md)
compares baseline `001a2b6` with this source in three interleaved release trials.
The two baseline columns distinguish its default dynamic interface from its old
optional typed interface. Values below are per-operation medians; they include
allocator instrumentation. These are local observations, not confidence intervals.

| Workload | Old default | Old typed | Current static |
| --- | ---: | ---: | ---: |
| Scalar value | 14.76 ns | 3.025 ns | 3.015 ns |
| Second-order jet | 23.82 ns | 7.225 ns | 7.027 ns |
| Complete cubic Remez solve | 36.20 ms | 36.68 ms | 0.557 ms |
| Extended Newton contraction | 535 ns | 538 ns | 545 ns |

Every Remez solve meets minimax gap `1e-8` and uniform error `0.006`, and produces
the same complex64 polynomial output boundary. Actual bounds are retained in the
receipts; the engines' richer evidence/ownership representations differ. The
approximately 65-fold time difference applies to this complete fixture, not all
Remez degrees or functions. MP-only `1/3` certification is reported separately
without a binary64 speedup ratio. The small Newton timing difference is within
the limits of this three-trial local comparison.

Remez allocations increase from 75 (73 old typed) to 376 per solve, while peak
extra live Rust allocation falls from 3,024 bytes (2,784 old typed) to 2,112.
Newton uses two allocations and 64 peak extra bytes, versus three and 160.
Small owning buffers and retained proof/coverage data remain; reduced elapsed
time and peak storage do not imply fewer allocation calls.

The numerical caller's cold build is 66.75 s versus 63.78 s; the touched-source
rebuild is 5.36 s versus 4.45 s. Its executable is 1,206,944 bytes versus 1,016,528
(old default), an 18.7% increase; it is 23.5% larger than the old typed fixture.
Builds use empty Cargo artifact directories for the cold observation, shared
system caches, two jobs and thin LTO. These are caller/dependency-closure costs,
not clean whole-workspace build costs. Single build timings are not distributions.

Assembly inspection identified outlined AD operations in the initial comparison.
Plain inlining hints removed that measured jet regression without removing
finite/domain checks. QR work reservation is now constant time on the audited
binary64 path; the actual factorization keeps its normal numerical cost.

The separate [project fixture](fixtures/static-architecture/project/README.md)
measures complete QSP synthesis, compiler stages and native execution. It checks
unit-circle responses, retained exact captures, and all native amplitudes/full
phase before timing. All 18 trial processes passed and source hashes remained
unchanged.

| Project workload | Baseline median | Current median |
| --- | ---: | ---: |
| QSP completion/NLFT, degree 256 | 1.079 ms | 1.050 ms |
| QSP completion/NLFT, degree 1024 | 5.013 ms | 5.053 ms |
| Compiler construction/admission | 3.527 µs | 3.442 µs |
| Compiler verify/lower/plan | 5.417 µs | 5.531 µs |
| Finite insertion and pipeline, 256 operations | 1.206 ms | 1.160 ms |
| Native shared matrix/oracle preparation | 99.917 µs | 78.426 µs |
| Native zeroed execution | 1.094 ms | 1.098 ms |

Native preparation improves 21.5% in this fixture; native execution and QSP
allocation/storage profiles remain unchanged. Finite insertion removes 466
allocations per call and lowers peak extra Rust storage from 686,326 to 597,930
bytes. QSP and execution timing differences are small relative to the limited
three-trial evidence. The native measurements here use serial CPU execution;
they are separate from the CPU/OpenMP consumer correctness checks.

The combined project executable grows 2.83%, from 4,233,952 to 4,353,792 bytes.
Cold build observations are 131.96 s baseline and 127.52 s current. Unchanged
invocations take 30.27/29.27 s, and touched-source invocations 30.95/29.47 s:
both checkouts rebuild native-dependent crates. These are observed rebuild
costs, not no-op or pure caller incremental timings. That existing invalidation
behavior is not attributed to this migration without a Cargo fingerprint trace.

Full ranges, build/RSS receipts, allocation counts, mathematical witnesses and
scope limits are in the [numerical report](data/2026-10-02-static-architecture/numerical-report.md)
and [project report](data/2026-10-02-static-architecture/project-report.md).

## Dependency follow-up

The subsequent multiprecision dependency review retained Astro-float as the sole
production floating-point backend and Rug/MPFR as an independent test oracle.
Rug's unused `rational` feature and `quest-compile`'s unused direct `num-rational`
dependency were removed. Exact integer/rational arithmetic remains in its
semantic owners. The [numerics dependency policy](../../crates/quest-numerics/README.md#multiprecision-dependencies)
explains the distinction.

After these changes, all three `mpfr_oracle` tests passed, and
`cargo check --offline -p quest-compile --all-targets --all-features` passed in the
project environment. `cargo tree --locked --workspace --all-features --target all
-e normal,build -i rug` found no dependency path. No arithmetic algorithm changed;
the full workspace and performance receipts above precede this manifest cleanup.
Their source hashes retain the measured snapshot, including the prior manifests
and lockfile, rather than being rewritten to imply another measurement.
