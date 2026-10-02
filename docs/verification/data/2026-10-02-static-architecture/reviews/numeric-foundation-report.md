# Numerical foundation implementation evidence

Date: 2026-10-02. Checkout: `/path/to/quest-rs`.
Owner: numeric-foundation subagent. Baseline supplied by parent: `001a2b6`.
No commits, workspace-wide formatting, QSP verifier edits, or native changes.

## Delivered interfaces

- `arithmetic::Backend`, separate `PointBackend` / `EnclosureBackend`, and sealed
  `CertifyingBackend`; backend errors are independent of polynomial and QSP.
- Concrete `F64Backend`, Maryada `Interval64Backend`, Astro-float 0.9.6
  `MpBackend`, and directed `MpIntervalBackend`. Direct exact endpoint bridge
  pairs f64/interval64 and BigFloat/MP interval. Non-Copy scalars are supported.
- Captured exact binary64, signed integer, rational, decimal, and arbitrary-size
  string ratio constants. Decimal/rational enclosure imports are directed.
  Adaptive enclosure-to-f64 rounding avoids fixed-intermediate double rounding.
  Hot-path small integers bypass MP allocation in binary64 arithmetic.
- `Precision` admits word-rounded precision, finite exponents and modeled backend
  operation count. Arithmetic validates backend errors before comparisons and
  shortcuts. Exact MP endpoint storage and actual mantissa bytes are accounted.
- `Budget` / `BudgetedBackend` counts application-level arithmetic calls, works
  with custom backend error types, and preserves sealed arithmetic admission
  only when its wrapped backend is admitted. Storage queries are forwarded.
- `FirstBackend`, `JetBackend`, `GradientBackend<B,N>` and `ad::jacobian<B,F,N,M>`
  supply value-only (ordinary backend), first, second, and rectangular static
  Jacobian evaluation. `Gradient` rows and Jacobian seeds/results are heap-owned.
  Explicit `JacobianLimits` admits dimensions and actual scalar-owned bytes before
  seeding; callbacks receive seed slices and return checked output vectors.
  `shapes::Matrix<T,R,C>` owns matrix entries on the heap.
- Generic scalar Newton, Krawczyk, Hansen–Sengupta, static-vector
  Hansen–Sengupta, and deterministic scalar root cover. Pole division preserves
  split branches. Coverage, existence, at-most-one, and uniqueness are distinct.
- `RootCover<T,E>` retains `covered`, `excluded`, `unresolved`, iteration count,
  callback premise, and backend failure. Covered boxes may contain no roots;
  exact-zero continua are marked separately. Unresolved boxes retain budget,
  arithmetic-failure and resolution-stall coverage. `complete()` also requires
  absence of a backend failure. Repeated roots can have complete coverage
  without uniqueness evidence.
- Removed the old binary64-only contractors module and its public compatibility
  exports; migrated its meaningful linear/coupled/split/singular/budget tests.
- Expanded crate README with mathematical premises, exact interchange and
  accounting limitations. Kept independent QSP verifier arithmetic untouched.

## Observed red/green evidence

Initial arithmetic and generic-root test gates failed because the new APIs were
absent; static vector and rectangular Jacobian API tests likewise failed before
implementation. Subsequent behavioral tests exposed and then verified fixes for:

1. Decimal midpoint + 1e-100 rounding incorrectly through a fixed 256-bit
   nearest intermediary before binary64 export.
2. Smallest positive binary64 subnormal midpoint rounding outside its input.
3. Extreme nonzero decimal underflow being returned by Astro-float as zero.

Additional regressions cover MP exp underflow; endpoint and derivative domains;
backend errors hidden behind zero operands; higher-precision input midpoint and
storage; arbitrary-size ratio distinction; shared operation budgets; branch and
byte admission; callback failure retaining partial coverage; scalar and coupled
vector roots; zero/singular preconditioners; constant zero/nonzero functions;
repeated roots; and below-binary64 MP root coverage.

## Verified commands

All commands used the project `devenv shell`.

```
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=target/mpfr-oracle cargo test -p quest-numerics
CARGO_TARGET_DIR=target/mpfr-oracle cargo clippy -p quest-numerics --all-targets --message-format short
rustfmt --check --edition 2024 <the ten changed numerics Rust entry/test files>
```

Package test result: **47 runtime tests passed**, plus **3 doctests passed**
(including sealed-admission and zero-dimension compile-fail checks).

- arithmetic: 16
- generic roots: 12
- independent MPFR/Rug oracle: 3
- retained kernels: 11
- retained observers: 5

Independent oracle uses Rug 1.30.0 / MPFR at 1024 bits and exact dyadic conversion
from Astro-float mantissas. It checks directed exp, ln, sqrt, sin, cos, division,
decimal inputs, cancellation, pi extrema, huge-argument enclosure, and first/second
sine derivatives. The root proof fixture covers `1e-1000` at `1e-1100` resolution
with 512-bit arithmetic. MPFR is a dev dependency only.

The initial bundled GMP/MPFR build was interrupted in the shared target to release
other workers, then successfully built with two jobs in `target/mpfr-oracle`.
This isolated target was reused for final tests and linting.

## Resource and proof boundaries

- Byte admission counts retained scalar-owned limbs, vector capacities and proof
  metadata plus conservative local scratch. Jacobian seeds and gradient rows are
  admitted heap storage; only O(N)/O(M) value input/output arrays remain static.
  Arbitrary callback temporaries remain caller-owned. Backend constant/transcendental
  caches, internal allocation metadata and internal arithmetic iteration counts
  are opaque; the public budgets are modeled, not an allocator-enforced cap.
- Initial invalid root requests return errors; failures after a cover step begins
  retain original current and pending boxes. Unrecoverable allocator failure is
  not a guarantee that an additional error artifact can itself be allocated.
- Arbitrary callbacks retain the continuously-differentiable enclosure premise.
  Sealed backend admission establishes arithmetic provenance; a higher-level
  function admission is still required for unconditional claims.
- Tests provide independent numerical evidence, not a formal proof of upstream
  Maryada/Astro-float implementations. Independent architecture/mathematics review
  belongs to the parent integration workflow.
- No Linux/MPI/accelerator or whole-workspace feature/performance claim is made.

## Read-only integration review supplied to parent

Reviewed newly written polynomial `function.rs`, `typed.rs`, `remez.rs`, and
`remez/{proof,exchange}.rs` independently of this owner's numerical implementation.
The parent accepted these concrete findings and owns their fixes:

1. **P1 certified export identity:** custom point arithmetic may be used with an
   audited enclosure backend. Re-running its `to_f64` during later export can
   change the exported polynomial after certification, and its `point` import
   need not match the first export. Freeze the exact binary64 payload, check the
   coefficient enclosure against audited `I.point(payload)`, and export only the
   retained payload. Parent confirmed this fix design.
2. **P2 live memory:** candidate-precision zero storage undercounts proof-precision
   endpoints transferred to point buffers; previous root covers remain live
   while replacements are built; precision history retains multiple full covers.
   Account actual scalar limbs and simultaneous containers, reserve cover work
   after retained evidence, and cap cumulative retained history.
3. **P2 fallible sorting:** replacing failed comparator results with `Equal` inside
   standard sorting changes the comparison relation midway through a budgeted
   sort. Replace with a deterministic fallible sort that exits immediately.
4. **P2 schedule admission:** reject an oversized precision schedule before
   cloning it into the retained request.

No additional false-positive bound issue was found in the centered residual
intersection, endpoint/critical-point uniform bound, inner-domain ordered strict
alternation, or compile-type function/enclosure evidence separation on this pass.
This review did not execute the parent integration tests and is not a claim that
subsequent parent revisions have been independently revalidated.

## Performance-driven follow-up

The first matched-accuracy measurement exposed a 512 KiB pending-queue allocation
in even tiny scalar root covers. A focused global-allocator test first failed
because a one-box continuum reserved the configured 10,000-box limit. The queue
now starts with one slot and grows fallibly only before committing admitted
children. Its capacity always accommodates pending plus previously unresolved
boxes, so popping the current parent leaves its recovery slot. Failure recovery
still needs no allocation. Growth and actual capacities are charged before a
step is committed. This replaces the previous worst-case preallocation strategy.

Binary64 point imports now have direct checked fast paths. The adaptive exact
rational/decimal importer is separate from thin inline binary64/integer constant
branches, allowing AD seed zeros/ones to remain visible to optimization. Exact
rounding, nonfinite rejection and overflow/domain checks are unchanged. New
bit-preservation/nonfinite regression coverage includes signed zero, subnormal
and maximal finite input. Timing evidence is retained separately in performance
receipts; no unmeasured improvement is claimed here.

The private MP monotone interval helper now takes statically selected closures
instead of string dispatch. It preserves input validation, one modeled work tick,
directed endpoints, logarithm/square-root domains and validation before the
exponential lower-endpoint underflow check.

After these changes and the independent review owner's admission/premise fixes:
`devenv shell -- cargo test -p quest-numerics` passes **57 runtime tests and four
doctests**; strict all-target package Clippy passes. Logs are
`performance/numerics-fixes-green.log` and `performance/numerics-fixes-clippy.log`.
The focused allocation red gate is `performance/root-allocation-red.log`.

### Final modeled-work and inline follow-up

`Backend::charge(work)` provides bulk opaque-kernel work admission. Its default
calls `visit()` in order until the first error, preserving custom side effects
and partial progress. The binary64 point/interval backends have constant-time
no-op implementations because they carry no local work meter. AD wrappers forward
the reservation. `BudgetedBackend` reserves against its own budget before
forwarding to the inner backend, so nested budgets all participate. A failed
inner reservation may leave the outer reservation charged; this is conservative
modeled work, not an assertion that every reserved operation executed. Checked
addition rejects overflow without changing the failing budget.

The nested-budget/overflow/default-visit exhaustion test first failed to compile
without the new method (`performance/bulk-charge-red.log`); the parent's final
workspace feature suite subsequently passed all 989 tests. The parent migrated
faer QR from its cubic visit loop to one bulk reservation. No numerical arithmetic
or domain checks were removed. Narrow plain inline hints on JetBackend add/mul
and the typed-expression forwarding boundary followed the recorded release
assembly diagnosis; final measurements are reported in `performance-report.md`.
