# Numerical polynomials and function expressions

The numerical crates work without a native QuEST installation. `quest-numerics` supplies reusable binary64 FFT workspaces and finite real intervals; `quest-polynomial` adds immutable typed polynomials, mathematical functions and approximation. The ordinary build uses sequential scalar binary64 kernels. SIMD and caller-owned Rayon execution are explicit opt-ins; production failure never selects Astro Float.

The following imports are shared by the executable examples in these chapters:

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:numerical_prelude}}
```

## Bases, coefficients and intervals

`Polynomial<B>` stores immutable complex coefficients in increasing basis order. Choose `Monomial`, `Chebyshev`, `Hermite`, `Laguerre`, `Jacobi` or `Laurent` explicitly. Hermite explicitly selects `physicists()` or `probabilists()`. Laguerre requires finite alpha > -1; Jacobi requires finite alpha and beta > -1 through checked constructors. `Laurent::new(offset)` preserves signed exponent support; negative powers have a pole at zero. Real interval evaluation requires real coefficients and a defined finite domain.

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:polynomial_interval}}
```

This evaluates (0.1+T_2(x)) at (x=0.3), giving approximately (-0.72), and encloses its values throughout a neighboring interval. The derivative remains represented in the selected basis with its original parameters. Conversion is a separate cold operation: `to_monomial()` and `to_basis()` return a converted polynomial plus an outward coefficient-error bound. Unsupported support transformations return an error. A conversion bound in an output basis is not automatically a uniform bound on every real domain; the magnitude of that basis on the domain matters.

`Even` and `Odd` admission checks exact forbidden coefficients and basis symmetry. Admission consumes the polynomial and returns a parity-bearing type. Tiny forbidden coefficients are not silently discarded. The exact cosine-circle conversion preserves complex coefficients and explicitly rejects inexact subnormal halving.

## One expression for values and derivatives

`function!` constructs one mathematical expression. Scalar values, scalar derivatives and interval derivatives all interpret that same expression; callers do not supply unrelated derivative callbacks. Expressions support arithmetic, exponential, logarithm, sine, cosine and square root. Ordinary Rust ownership applies, so use `x.clone()` when an expression uses its variable more than once.

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:function_remez}}
```

The binary64 Remez builder needs a function and closed domain before it can run. Its deterministic pivoted QR uses `faer::Par::Seq`. Success establishes a uniform-error upper bound for the actual exported polynomial and an alternation lower bound for the best degree-bounded approximation. **`tolerance` bounds the gap between these bounds; it does not bound the total approximation error.** For the cubic exponential example, the error is about 0.00553 while the established minimax gap is at most (10^{-8}).

The function and its first two derivatives must have finite interval enclosures on the supplied domain. Stationary-point isolation distinguishes boxes with established root existence and uniqueness from unresolved boxes. A subdivision limit, undefined derivative, failed solve or insufficient enclosure returns an error instead of a sampled-grid certificate. Warm scalar/interval polynomial and function evaluations allocate no heap memory; conversion, root isolation and approximation are cold operations with explicit limits.

Callback-backed functions remain separate and require an explicit consistency assumption for independently supplied callbacks. Use the single-expression representation when mathematical provenance matters.
