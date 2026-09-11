# QSVT encodings and transform builders

`quest-qsp` produces typed phases or generalized controls for a scalar polynomial
response. `quest-qsvt` combines those values with a projected unitary encoding to
construct a circuit and its required projections. Both crates are pure Rust:
constructing either value does not initialize QuEST, MPI or a worker pool.

## Construct a numerical encoding and a standard transform

The crate's executable rustdoc example builds a small dense encoding and applies
canonical phases. It materializes the logical output block as a bounded numerical
reference; native state simulation is a later, separate stage.

{{#include ../../../crates/quest-qsvt/README.md:pure_transform}}

`DenseEncodingBuilder` copies a checked faer view and builds a numerical unitary
dilation using binary64 arithmetic. It preserves rectangular dimensions and
independent padding. Its normalization is an explicit positive scalar: the
encoded logical block represents the source matrix divided by that scalar.
Residual checks admit a numerical encoding; they do not prove exact unitarity.

For an existing circuit, use `EncodingBuilder` with an immutable
`OracleFragment`, separate `LogicalSpace<Left>` and `LogicalSpace<Right>` values,
and normalization. Required fields are consuming builder states. Left and right
are different types even when their dimensions happen to match. Projectors can
use coordinate indices, an admitted isometry, or a dense orthogonal projector.
An isometry describes its basis directly; a projector needs a decoded basis to
define logical coordinates.

Default oracle admission computes a bounded whole-oracle unitarity residual.
An explicit mathematical unitarity premise allows a caller to retain a scalable
oracle without dense materialization. The named premise remains an assumption;
it grants no exact inverse-cancellation property to numerical circuit payloads.
Neither admission path changes the source oracle or makes a resource native.

## Select the route explicitly

`TransformBuilder` requires an encoding and one supported recipe before `build`
is available. The standard recipe accepts a phase sequence carrying its convention
in its Rust type. Generalized recipes accept immutable controls and state their
structural route in the builder method.

| Recipe | Input and structural meaning |
| --- | --- |
| `standard(phases)` | Canonical QSVT with an explicit Wx phase convention |
| `direct(controls)` | Generalized transformation with Hermitian whole-oracle and agreeing-projector admission |
| `hermitianized_full(controls)` | Full structural Hermitian lift using the source and its adjoint |
| `hermitianized_even(controls)` | Reduced even route through that lift |
| `hermitianized_odd(controls)` | Reduced odd route through that lift |
| `multiplication_even(controls)` | Reduced even multiplication route |
| `multiplication_odd(controls)` | Reduced odd multiplication route with bridge projection and source continuation |

Route admission retains its fixed numerical requirements, auxiliary operand
layout, degree-zero behavior and readout corrections. Unsupported combinations
return structured errors. `OperandLayout` preserves caller operand order,
including nonsorted operands; it does not sort a matrix's local bit convention.

The generalized QSP product is documented in [synthesis](qsp-synthesis.md).
Its exported final control already contains the convention factor. Circuit
construction preserves that order and full scalar phase. Do not append a second
factor or erase a global phase before applying controls. Standard phase
conversion also changes floating values: its roundoff estimate is a diagnostic,
and an earlier QSP certificate does not automatically cover the converted
transform.

## Keep the complete transform

`ValidatedTransform` owns the immutable encoding, circuit, projectors, optional
continuation, route and query metadata. Its execution order is:

```text
input projection -> main circuit -> bridge projection -> continuation -> output projection
```

The bridge and continuation are present only when required by the route.
Extracting `main()` alone loses any surrounding projections and continuation.
A projected transform therefore cannot implicitly become a coherent oracle.
Reusable coherent source bodies remain ordinary `OracleFragment` values and
can be captured by the circuit DSL; projections stay outside that unitary DAG.

`query_counts()` distinguishes semantic degree, forward/adjoint source uses and
retained oracle instructions. These are circuit-model quantities. Native
dispatch counts belong to prepared execution and include the actual selected
decompositions; the two reports answer different cost questions.

## Choose the evidence you need

`materialize_block()` and `diagnostics()` form bounded dense references with
faer and explicit sequential execution. Diagnostics preserve complex phase and
subnormalized values. Sampled observations and numerical SVD residuals are not
certified operator bounds, and neither is a native execution measurement.

`analysis()` can produce a standard-QSVT bound conditional on named mathematical
premises tied to the same immutable transform: encoding semantics, actual phase
linkage, parity and completion hypotheses. An optional execution-error term must
be supplied before claiming a native-execution bound. Generalized routes retain
numerical observations but do not publish an established generalized-QSVT
robustness theorem. QSP/control certificates can be reported separately; the
transform builder accepts a sequence, does not retain its source certificate,
and does not automatically certify the constructed transform.

See the [analysis contract](qsvt-analysis.md)
for formulas, normalization composition and premise details. Continue with
[native QSVT](qsvt-runtime.md) to admit and prepare the transform, reuse its
resources, then release or consume the exclusive execution result to condition
the retained state.
