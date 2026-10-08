# Shared numerical resources and NLFT workspace migration

The [large-polynomial plan](plans/2026-10-08-large-polynomial-benchmarks.md)
separates numerical accuracy, representable shapes, modelled storage and cumulative
work. The [capacity verification record](verification/2026-10-08-large-polynomial.md)
records executed checks; an admitted configuration alone is not a capacity result.

## Replace flat limits with one operation policy

`OperationLimits` contains `ShapeLimits` and `ResourceLimits`. Numerical
accuracy remains separate in `quest_qsp::Policy::accuracy`:

| Type | Fields | Meaning |
| --- | --- | --- |
| `ShapeLimits` | `max_coefficients`, `max_fft_len`, `max_completion_grid` | Coefficient/reflection slots, padded transform length and completion grid |
| `ResourceLimits` | `max_peak_bytes`, `max_work_units` | Peak modelled owned storage and cumulative admitted work |
| `AccuracyPolicy` | `response_tolerance`, `contractivity_margin` | Numerical acceptance in `quest_qsp::Policy::accuracy` |

The transitional `Limits` name aliases `OperationLimits`; it does not preserve the
old flat struct fields. Update struct literals and field access through `shapes`
and `resources`. `Policy::limits` uses this shared policy, while accuracy settings
live in `Policy::accuracy`. Increasing capacity limits does not raise a tolerance.

The defaults admit 1,048,576 coefficient slots, FFT entries and completion-grid
entries, 512 MiB of modelled peak storage, and 2^33 cumulative work units on a
64-bit target. The explicit 64-bit `OperationLimits::million_degree()` profile
admits 1,000,001 slots, FFT length 2^21, completion grid 2^22, 8 GiB of modelled
peak storage and 2^37 cumulative work units. The completion grid is a separate
gate; these values do not demonstrate completed or certified million-degree
completion. Larger limits do not remove quadratic work or conditioning.

## Share the operation ledger across stages

`OperationResources::from_limits(limits)` creates a ledger. Cloning it shares
the same counters and ownership reservations; it does not reset work. Pass that
ledger through polynomial preparation, completion refinements, inverse synthesis
and reconstruction when those stages belong to one operation.

`Polynomial::new_with_resources` and `Polynomial::from_scalars_with_resources`
accept an existing ledger. Cloned polynomials share immutable coefficient storage
and its reservation. Derived polynomials retain the originating ledger; evaluation
and transformation work accumulate there. Obtain it with `operation_resources()`.

To include synthesis in that operation, use
`SynthesisBuilder::new().resources(polynomial.operation_resources())` **before
target selection**. The supplied ledger's limits take precedence over conflicting
`Policy::limits`. Without a supplied ledger, the builder starts a new operation
for its owned target/source copies; the caller's borrowed polynomial remains
outside that new owned-buffer model. Offline attempts share work, and artifact
loading uses the loader's own admission limits rather than the producer's limits.

## Keep results and reservations together

`Accounted<T>` owns a value and its reservation tokens. Storage remains charged
when a result outlives its workspace. Shared reservations are released with their
last owner. `Accounted::map` transfers existing tokens during an ownership-preserving
representation change; it does not automatically account for unrelated new
allocations. When using `into_parts`, keep the returned tokens with the storage.

Retained FFT plans and scratch remain charged between calls. Work is monotonic
across calls, retries and failures after admission. Ordered parallel batches are
admitted before dispatch. `ResourceReport` separates live buffer bytes and opaque
planner estimates, and records configured limits, requested peaks/work, admitted
peaks/work and the last structured rejection. Allocation, overflow, shape, byte
and work rejections retain evidence.

The byte model is not process RSS, an allocator quota or an OOM guarantee.
Allocator metadata, scheduler state and opaque backend internals are not fully
represented; RustFFT plan charges are conservative estimates.

Generic MathCore contexts and the Remez approximation, Remez precision-attempt,
and root-proof drivers retain separate arithmetic counters and storage contracts. Where applicable their
limits use the shared field vocabulary, but their internal budgets do not join a
caller's `OperationResources`. A single `ResourceReport` therefore does not
aggregate these drivers or arbitrary callbacks together with the NLFT pipeline.
The shared ledger covers the owned polynomial and Binary64 operations described
above and the production FFT/completion/inverse/reconstruction pipeline.

## Reuse public NLFT workspaces

`InverseNlftWorkspace::new(backend, resources, execution)` and
`ForwardNlftWorkspace::new(backend, resources, execution)` accept the same ledger.
Plans and scratch are reused across calls. The convenience paths use temporary
workspaces around these implementations.

| Operation | Input convention | Owned result |
| --- | --- | --- |
| `inverse(a_star, b)` | Canonical reversed, conjugated complement and target | `Accounted<Vec<Complex64>>` |
| `inverse_physical(a, b)` | Ascending physical coefficients, supports `[-d, 0]` and `[0, d]` | Same result; reverse/conjugate preparation is charged |
| `forward(gamma)` | `d + 1` independent reflection slots | `Accounted<ScatteringPair>` with canonical complement and target |

The following sequence assumes valid physical input arrays `a` and `b` and an
explicitly chosen backend. Borrowed input arrays remain caller-owned:

```rust
use quest_numerics::{ExecutionPolicy, FftBackend, OperationLimits, OperationResources};
use quest_qsp::{ForwardNlftWorkspace, InverseNlftWorkspace};

let resources = OperationResources::from_limits(OperationLimits::million_degree());
let mut inverse = InverseNlftWorkspace::new(
    FftBackend::Simd, resources.clone(), ExecutionPolicy::Sequential,
);
let mut forward = ForwardNlftWorkspace::new(
    FftBackend::Simd, resources.clone(), ExecutionPolicy::Sequential,
);
let gamma = inverse.inverse_physical(&a, &b)?;
let pair = forward.forward(&gamma)?;
let observed_resources = resources.report();

drop(pair);
drop(gamma);
drop(inverse);
drop(forward);
assert_eq!(resources.report().live_bytes, 0);
```

`cached_plan_count()` and `cached_buffer_bytes()` expose retained workspace
capacity; `resource_report()` exposes shared accounting. Repeating a call can
reuse both plans and scratch while still spending additional cumulative work.
Zero live bytes after all owners drop does not reset the operation's work total.

The crate-level guides provide further context:
[numerics](../crates/quest-numerics/README.md#shared-operation-resources),
[polynomials](../crates/quest-polynomial/README.md#sharing-resources), and
[QSP](../crates/quest-qsp/README.md#resource-migration-and-reusable-nlft-workspaces).
