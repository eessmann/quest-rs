# Numerical polynomials and functions

`quest-numerics` owns checked arithmetic, outward intervals, automatic differentiation and root contractors. `quest-polynomial` owns expression structure, polynomial mathematics and Remez. Both work without a native QuEST installation. Binary64, binary64 intervals, multiprecision points and multiprecision intervals are explicit, statically dispatched choices. Runtime precision and convergence remain numerical obligations.

## Coefficients, bases and shapes

`Polynomial<B, C, D>` owns immutable coefficients of type `C`, a basis `B`, and an admitted coefficient shape `D`. `DynamicShape` checks runtime input lengths; `StaticShape<N>` keeps the same length in the type. Coefficients and large numerical workspaces live on the heap. The default complex binary64 coefficient type supports QSP and scientific interchange; it does not determine the approximation backend.

Choose `Monomial`, `Chebyshev`, `Hermite`, `Laguerre`, `Jacobi` or `Laurent`. Recurrence coefficients are computed through the selected backend from original parameters and exact integer constants. Multiprecision evaluation does not lift an already rounded binary64 recurrence. Hermite conventions are explicit; checked Laguerre and Jacobi constructors require their real orthogonality parameters to exceed minus one.

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:polynomial_interval}}
```

`stored_support()` includes zero coefficients; `effective_support()` counts only nonzero terms. The zero polynomial has no mathematical degree. Signed Laurent support retains poles at zero. Conversion retains the original payload and outward rounding evidence; a coefficient bound needs basis/domain information before it implies a uniform error bound. Parity admission checks exact forbidden coefficients and basis symmetry.

## One static function interface

`function!(|x| (1.0 + x*x).ln())` produces a concrete `Function<E>`. Arithmetic and exp/log/sine/cosine/square-root nodes share one backend evaluator. `GenericFunction::evaluate`, `first` and `jet` select value, first-order or second-order arithmetic at compile time. The first-order path does not compute unused second derivatives.

Variable leaves are `Copy`; captured exact decimal and rational constants can be owned and non-`Copy`. `typed::exact(ExactConstant::Decimal(source))` retains the original source. Binary64 constants retain their exact dyadic value. No interpreted `Expr`, dynamic conversion, callback pair or alternate function macro remains.

Static metadata exposes depth, node and operation counts, input dimensions and derivative-domain requirements. Const construction uses nightly const traits and const operators; owned expression composition uses `const_destruct`. Numerical execution still charges backend calls and checks domains. `generic_const_exprs` remains an incomplete nightly feature; compiler-contract tests and validation records identify the tested compiler revision.

`function!(|x, y| [x*x + y, x*y])` constructs a typed system. Its Jacobian uses the same expression nodes and first-order AD, checked const dimensions, heap-owned derivative rows and explicit `JacobianLimits`.

## One approximation engine

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:function_remez}}
```

`RemezRequest` owns the function, exact domain, shape, arithmetic policies, linear solver and limits. Runtime degree selection and `.degree::<N>()` use the same engine; the latter checks `N + 1` coefficients and `N + 2` alternation dimensions in the type system. There is no separate offline exchange loop.

`PivotedQr` retains faer's optimized binary64 QR. `MpHouseholder` uses scaled column-pivoted Householder arithmetic. Both enforce the same numerical rank and backward-residual contract. These candidate checks do not establish a certificate.

`UniformErrorCertificate` bounds the actual selected polynomial's uniform error. `MinimaxGapCertificate` additionally uses strict, ordered alternation to bound its distance from the optimal degree-bounded error. `Accuracy` selects the required meaning independently of precision. For the cubic exponential example, uniform error is about 0.00553 even when the minimax gap is below `1e-8`.

Critical-point isolation uses the shared deterministic root-cover driver on the residual derivative. Coverage, root existence, at-most-one-root evidence and established uniqueness are separate facts. Narrow boxes may cover repeated roots without proving uniqueness. Identically zero derivatives form a covered continuum. Unresolved branches from exhaustion, arithmetic failure or resolution stalls prevent a global certificate.

The enclosing backend evaluates the complete domain, including outward rounding of exact endpoints. Alternation points must lie inside inward-admitted endpoints, so a rounded rational endpoint cannot create an invalid minimax lower bound. Undefined derivative domains return an error.

Multiprecision requests may certify the MP polynomial directly. Selecting `.export_binary64()` freezes coefficients before certification; the audited enclosing backend checks that each frozen coefficient equals the selected mathematical coefficient. Later exports reuse that payload. Increasing proof precision cannot hide binary64 coefficient rounding error.

Every failure owns its original request, exact inputs, attempted precision, last candidate, partial coverage and resource accounting. `run_with_precisions` follows only its explicitly bounded schedule; it retains one owned target, all attempts, total-work limits and admitted history storage. There is no algorithm fallback or tolerance relaxation.

## Evidence and extension boundaries

`Expression` is sealed. The single open function interface is `GenericFunction`; `AssumedFunction` retains the explicit premise that implementations evaluate the same pure function and respect backend operations. Custom enclosing backends require their own enclosure assumption. Certificate types carry both admissions; unconditional access is available only for sealed expressions with audited enclosing arithmetic.

`BudgetedBackend` accounts executed application-level operations. It does not claim to measure opaque work or allocations inside transcendental libraries or arbitrary callback code. Interval operations check domain and exponent limits. Production MP point and interval arithmetic use the pinned Dashu backend; independent test bounds use exact rational series and inequalities. The QSP verifier retains separate algorithms and the exact-angle subsystem retains symbolic semantics. Storage accounting follows actual significand words, guard bits and retained constant caches. Opaque work and temporary allocations inside transcendental calls remain outside the modeled operation budget.
