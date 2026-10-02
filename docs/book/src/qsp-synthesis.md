# QSP conventions, algorithms and stages

The examples use the [shared numerical prelude](numerical-polynomials.md).

`quest-qsp` separates numerical input admission, outer-factor completion, control synthesis and independent certification. Each construction stage consumes its input and publishes its result only after that stage's checks. A completed polynomial and a frozen candidate are not interchangeable with an independently certified export.

## Explicit algorithm choice

`Policy::algorithm` defaults to `InverseNlftDivideConquer` in binary64 and
offline construction. `RhwHalfCholesky` remains an explicit selection. Both
support real-parity Wx phases and complex unit-circle generalized QSP controls.
Recorded algorithms in compiled artifacts retain their original meaning;
loading an RHW artifact does not select or run a different solver. A bounded dense RHW factorization is retained as an internal test oracle.

`complete()` and `complete_with()` honor the selected algorithm. NLFT completion
computes the outer complement without constructing or retaining the Weiss ratio;
RHW completion includes that ratio. `CompletedPolynomial::algorithm()` identifies
the retained payload, and `weiss_ratio()` returns `None` for NLFT or
`Some(&WeissRatio)` for RHW. Callers that need the ratio must select RHW before
completion. Offline synthesis makes the same internal distinction.

RHW completion publishes Fourier coefficients of `b/a` with target identity,
positive-real-constant gauge, grid and contractivity evidence. The structured
Half-Cholesky recurrence uses complex rank-two generator rotations and shifted
rows; it is distinct from dense factorization. Algorithm, precision, FFT backend
and execution policy are separate choices with no silent fallback. See
[Laneve §5](https://arxiv.org/html/2503.03026v2) and
[Ni–Ying](https://arxiv.org/html/2410.06409v2).

## Real-parity Wx response

A `RealParityWx` target is a real Chebyshev polynomial with definite parity and an established strict contractivity margin. The example uses the odd polynomial (p(x)=0.6x):

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:canonical_synthesis}}
```

The frozen candidate exposes immutable binary64 phases in the stated Wx convention. `response(x)` evaluates that exported sequence. Native circuit construction and execution are separate stages; synthesizing phases does not allocate a native QuEST environment.

Use `SynthesisBuilder::policy` to choose response tolerance, contractivity margin, backend and numerical limits. Limits are checked before admitting workspaces. Byte accounting includes retained payloads, working buffers and a declared allowance for opaque FFT plans; it is not an allocator-enforced physical-memory cap. A failed budget or enclosure is reported rather than repaired by an automatic precision change.

## Generalized unit-circle response

The `unit_circle_response` builder accepts complex power coefficients with nonnegative support. Signed Laurent polynomials remain available in the polynomial layer, but negative support requires a separate explicit mathematical transformation before this synthesis interface can accept it.

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:generalized_synthesis}}
```

The exported product is `C0 D(z) C1 ... D(z) Cd`, with `D(z) = diag(z, 1)`. The final control already incorporates the right convention matrix `K = [[0, -1], [1, 0]]`; do not append it again. The complete target quartet is:

```text
[ b(z)       -reverseconj(a_star)(z) ]
[ a_star(z)   reverseconj(b)(z)      ]
```

Here `reverseconj` reverses the coefficient order and conjugates each coefficient, using the full admitted degree.

Scaled normalization avoids squaring unscaled large reflections. Completion and reconstruction residuals in the ordinary construction pipeline are binary64 numerical diagnostics. Use the independent certification stage for outward error guarantees on actual exported matrices or phases.

## Caller-owned synthesis workers

Enable `quest-qsp/rayon` to borrow a caller-owned pool for each consuming numerical stage:

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:parallel_synthesis}}
```

The ordinary `complete()` and `synthesize()` methods select sequential execution. For inverse NLFT, explicit pool execution preserves each reduction and the inverse dependency order: first half, midpoint, second half. Independent forward FFTs use separate prepared scratch, pointwise operations use fixed index semantics, and sufficiently large forward reconstruction subtrees can run concurrently. Small work stays serial. No global pool or pool lifetime enters the frozen payload, and failures never dispatch to another precision or backend.

Parallel subtrees receive deterministic partitions of the remaining byte and work budgets; their work is counted back into the parent before reconstruction continues. This can reject a tight budget that fits serial execution because concurrent branches require separate FFT plans and scratch. Errors are selected in the original left-before-right order. Opaque plan storage remains an explicitly modeled allowance rather than an allocator hard cap. Certification and offline Dashu stages remain separate from this binary64 execution policy.

## Observe the complete stage

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:stage_observation}}
```

The observer and clock belong to the caller. Wrap an entire `certify()` call to include all verifier retries, or an entire offline `solve()` to include all offline attempts and their certification. `NoopObserver` skips clock reads and event construction. Scoped spans record completed, failed or aborted operations; traces expose capacity loss and clock regressions. Optional `quest-numerics/trace-json` exports Chrome/Perfetto trace JSON.

The provided clock measures elapsed wall time on the calling CPU. GPU completion, process CPU usage and cross-host timing require a suitable explicit clock/synchronization contract.
