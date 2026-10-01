# Numerical polynomials and function expressions

The numerical crates work without a native QuEST installation. `quest-numerics` supplies reusable binary64 FFT workspaces and finite real intervals; `quest-polynomial` adds immutable typed polynomials, mathematical functions and approximation. The ordinary build uses sequential scalar binary64 kernels. SIMD and caller-owned Rayon execution are explicit opt-ins; production failure never selects Astro Float.

The following imports are shared by the executable examples in these chapters:

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:numerical_prelude}}
```

## Bases, coefficients and intervals

`Polynomial<B>` stores immutable complex coefficients in increasing basis order. Choose `Monomial`, `Chebyshev`, `Hermite`, `Laguerre`, `Jacobi` or `Laurent` explicitly. Hermite explicitly selects `physicists()` or `probabilists()`. Laguerre requires finite alpha > -1; Jacobi requires finite alpha and beta > -1 through checked constructors. `Laurent::new(offset)` preserves signed exponent support; nonzero negative effective support has a pole at zero. Real interval evaluation requires real coefficients and a defined finite domain.

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:polynomial_interval}}
```

This evaluates (0.1+T_2(x)) at (x=0.3), giving approximately (-0.72), and encloses its values throughout a neighboring interval. The derivative remains represented in the selected basis with its original parameters. Conversion is a separate cold operation: `to_monomial()` and `to_basis()` return an immutable `Conversion<Destination, Source>` retaining both payloads and an outward coefficient-error bound. Unsupported support transformations return an error. A conversion bound in an output basis is not automatically a uniform bound on every real domain; the magnitude of that basis on the domain matters.

`stored_support()` includes retained zero coefficients; `effective_support()`
includes only nonzero terms. `degree()` is the highest nonzero basis order or
`None` for the zero polynomial. `stored_order()` is a storage/recurrence quantity.

`Even` and `Odd` admission checks exact forbidden coefficients and basis symmetry. Admission consumes the polynomial and returns a parity-bearing type. Tiny forbidden coefficients are not silently discarded. The exact cosine-circle conversion preserves complex coefficients and explicitly rejects inexact subnormal halving.

## One expression for values and derivatives

`function!` constructs one mathematical expression. Scalar values, scalar derivatives and interval derivatives all interpret that same expression; callers do not supply unrelated derivative callbacks. Expressions support arithmetic, exponential, logarithm, sine, cosine and square root. Ordinary Rust ownership applies, so use `x.clone()` when an expression uses its variable more than once.

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:function_remez}}
```

The binary64 Remez builder needs a function and closed domain before it can run. Its deterministic pivoted QR uses `faer::Par::Seq`. Success establishes a uniform-error upper bound for the actual exported polynomial and an alternation lower bound for the best degree-bounded approximation. **`tolerance` bounds the gap between these bounds; it does not bound the total approximation error.** For the cubic exponential example, the error is about 0.00553 while the established minimax gap is at most (10^{-8}).

The function and its first two derivatives must have finite interval enclosures on the supplied domain. Stationary-point isolation distinguishes boxes with established root existence and uniqueness from unresolved boxes. A subdivision limit, undefined derivative, failed solve or insufficient enclosure returns an error instead of a sampled-grid certificate. Warm scalar/interval polynomial and function evaluations allocate no heap memory; conversion, root isolation and approximation are cold operations with explicit limits.

Callback-backed functions remain separate and require an explicit consistency assumption for independently supplied callbacks. Use the single-expression representation when mathematical provenance matters.

## Static expressions and arithmetic policies

`typed_function!(|x| (1.0 + x*x).ln())` retains the concrete expression tree in
`Function<E>`. Typed nodes are `Copy`; construction, arithmetic operators and
`static_metadata()` can be used in a const context. Constant-only expressions
such as `typed_function!(|x| 0.5)` are supported. The existing `Function` default
remains `Function<Expr>` and `function!` continues to construct the dynamic AST.
`Function::new(0.5.into())` retains its original inference; direct generic
construction uses `Function::from_expression(expression)`.
Static dispatch does not allocate an AST or use a vtable. This API deliberately
uses nightly `const_trait_impl`, `const_ops` and `generic_const_exprs`.

Static metadata describes node count, maximum depth, mathematical operation
count, and the positive arguments/nonzero denominators required by derivative
jets. Counts saturate at `usize::MAX`; dynamic DAG nodes cache this metadata,
so inspecting a shared subexpression never expands its whole evaluation tree.
These are structural facts, not domain proofs or convergence decisions.
Evaluation still rejects nonfinite arithmetic, invalid domains and trees deeper
than 256 nodes. `sqrt` has a defined scalar value at zero but its second-order
jet requires a positive argument.

All expression forms use the same open `Backend` interface. `ScalarBackend<f64>`
uses checked binary64 operations; `ScalarBackend<Interval>` uses outward
interval arithmetic. `JetBackend` lifts either policy, or a non-`Copy` backend,
to first and second derivatives using the same differentiation rules. An
external backend controls its own precision, errors and accounting through
`evaluate_backend` and `jet_backend`; no backend transition occurs implicitly.
Library error certificates always use the library's outward interval policy.

The arbitrary-precision offline Remez path supplies Astro Float arithmetic to
this shared evaluator. Captured binary64 constants are injected exactly, rather
than reparsed as decimal approximations. Each precision attempt reuses the
original typed expression. Success retains that expression and encloses error
for the actual exported binary64 coefficients. A terminal failure structurally
converts the original expression to the compatibility AST in its failure report;
this conversion retains every captured constant bit. Backend operations consume
the runtime work budget, and storage admission includes recursive derivative-jet
temporaries at the maximum requested precision.

`RemezBuilder::static_degree(StaticDegree::<N>::new())` selects an optional static
degree. Its result provides a checked borrow of `&[Complex64; N + 1]` from the very same
heap-backed polynomial that was certified, without allocating a duplicate array; `N + 2` alternation dimensions are also checked
for representability during type checking. Options cannot override the static
degree. Numerical convergence, tolerances, precision, allocation limits and
certification remain runtime decisions. The dynamic `.degree(n)` path remains
available; offline precision policy likewise remains explicit and runtime.

`Expression` is sealed. User evaluators implement the open `Callable` interface
or `GenericCallable` with `AssumedFunction`, and must supply
`ConsistencyAssumption::SameFunctionAndDerivatives`. The generic callable works
with arbitrary backend scalar types and conditional automatic derivatives.
`RemezBuilder::callable(callable, assumption)` admits a native approximation and
returns `ConditionalRemezResult`, which owns the callable and premise. Its
`conditional_error_bound` and `conditional_minimax_lower_bound` have meaning
only under that premise. It cannot convert to a trusted `RemezResult`, and
custom callable work is not claimed to be bounded by expression metadata.
Offline Remez certification accepts sealed expressions; custom generic
callables can be evaluated with an explicit arbitrary-precision backend but
are not admitted to that unconditional offline report.


Native trusted Remez precharges cached expression metadata before domain
admission, then charges every target/polynomial value and jet traversal against
the remaining work budget alongside the QR reservation. This prevents a shared
AST with exponentially many logical visits from expanding before admission.
Offline `.domain(...)` admits interval geometry only; `.policy(...)` checks
work/storage limits before evaluating interval derivatives and retains that
admission charge through all precision attempts. Custom callback work remains
caller-controlled and conditionally interpreted.

Use `.run_reported()` when a failed native approximation must retain its
original request. `RemezFailure<E>` owns the typed function, interval, options and
failure reason, including when reached through the static-degree wrapper.
`ConditionalRemezFailure<C>` additionally retains the callable premise. The
existing `.run()` is an error-only compatibility adapter and discards that
request on failure. These failure reports do not claim approximation evidence.
