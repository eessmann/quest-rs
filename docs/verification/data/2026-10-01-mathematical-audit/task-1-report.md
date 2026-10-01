# Task 1 implementation report

## Changes

- Added open contextual `Backend` with cloneable/non-Copy scalar support and one shared second-order `JetBackend`. Library binary64 and outward interval policies adapt the existing checked arithmetic. Legacy polynomial jet operations delegate this same AD implementation.
- Sealed `Expression` implementations cover the dynamic `Expr` and statically dispatched typed nodes. `Function<E = Expr>` preserves default `Function::new(0.5.into())` inference via a compatibility constructor; generic construction uses `from_expression`. `function!` explicitly selects its compatibility AST. `typed_function!` handles arithmetic/transcendentals/captured f64 and constant-only expressions without AST allocation.
- Nightly const operator implementations and `StaticExpression` provide const construction and structural metadata: nodes, operations, depth, derivative-positive arguments and nonzero denominators. Dynamic nodes cache saturating metadata without recursively expanding shared DAGs. Numerical values, domains, precision, convergence and budgets remain runtime decisions.
- Native Remez states and trusted reports retain generic original expression types. Optional `StaticDegree<N>` uses generic const expressions for representable N+1 coefficient and N+2 alternation dimensions. The result owns only the existing heap-backed polynomial; checked fixed-array coefficient borrowing does not duplicate it or put large arrays on the stack.
- Open `Callable` and `GenericCallable`/`AssumedFunction` admit user evaluators. The native callable Remez route reuses the private numerical core but returns a distinct `ConditionalRemezResult` retaining its callable and explicit premise; there is no conversion to unconditional evidence. Custom callables have no structural-work claim. Generic callables support conditional AD on arbitrary non-Copy backends.
- Offline Remez now implements arithmetic policy through its Astro Float Context and removes the duplicate AST differentiation interpreter. Generic typed targets survive all precision attempts and successful reports; terminal errors structurally erase into the compatible default failure report without changing captured binary64 constant bits.
- Offline logical arithmetic operations are charged individually, enclosure visits include conservative derivative-operation cost, and storage admission includes recursive second-order jet temporaries at maximum admitted precision.
- Added independent analytic derivative/backend tests, const metadata and dimensions, conditional admission, compile-fail sealing/evidence checks, exact captured constants, actual exported coefficients, original-target precision retry retention, and non-Copy/custom backend tests. Updated the numerical-polynomials book chapter.

## Verification

- Initial red test: `devenv shell -- cargo test --locked -p quest-polynomial --test typed_function` failed as expected on absent typed_function/StaticDegree/generic Function APIs.
- `devenv shell -- cargo test --locked -p quest-polynomial`: passed all existing and new polynomial tests, including all eight typed-function tests and eight doctests (compile-fail admission/dimension checks included).
- `devenv shell -- cargo test --locked -p quest-qsp --features offline-synthesis --lib offline::remez`: six tests passed, including independent 256-bit derivatives, exact 0.1 binary64 injection, typed coefficient export and retained target after 64/128-bit attempts including the final accounting and custom-evaluator changes.
- Formatting uses project nightly rustfmt on owned files only; owned-file `git diff --check` passed.
- `devenv shell -- cargo clippy --locked -p quest-qsp --features offline-synthesis --lib --test offline --no-deps`: passed after the boundary regression update.
- `devenv shell -- cargo clippy --locked -p quest-polynomial --test typed_function --no-deps`: passed.
- `devenv shell -- cargo clippy --locked -p quest-qsp --features offline-synthesis --lib --tests --no-deps`: own library/test diagnostics clear; blocked by two unrelated analytic-test lints in completion_outer.rs reported to parent.
- Extended offline storage-boundary regression to independently specified depth-one/depth-three targets, including the newly required 32 live jet temporaries per level. `devenv shell -- cargo test --locked -p quest-qsp --features offline-synthesis --test offline remez_policy_checks_exact_modeled_storage_boundary` passed.

## Boundaries

- Trusted offline Remez accepts sealed expressions. User generic callables can evaluate/derive on custom arbitrary-precision backends and use native conditional Remez; they do not enter the unconditional offline certification report.
- Optional static degree support is the native Remez wrapper. Offline approximation degree and all precision policies remain runtime.
- Generic const expressions trigger the installed nightly compiler's informational next-solver fallback warning; incomplete-feature allowance is local and intentional.
- No commits made. Parent owns cross-workspace final validation and performance receipts.

## Independent-review corrections

- Closed the DAG admission gap: trusted native domain admission precharges cached
  metadata against current limits. Native approximation reserves QR work and
  conservatively charges every target/polynomial scalar or second-order interval
  traversal before evaluation. Charges accumulate across iterations and
  subdivisions; shared DAGs that saturate node metadata reject without expansion.
  Standalone critical-point isolation uses the default finite work admission too.
- Offline domain construction now validates interval geometry only. Policy
  admission checks cached jet work and modeled storage before any derivative
  evaluation, then carries that initial charge across all precision attempts.
- Added `run_reported` and immutable generic `RemezFailure<E>` retaining the
  original target/domain/options/error. The static-degree wrapper uses that
  same retained failure path. Conditional callable failures retain the callable
  and explicit premise in `ConditionalRemezFailure<C>`. Existing `run` remains
  the source-compatible error-only adapter, documented as dropping the request.
- Added degree+1/+2 overflow compile-fail for `StaticDegree<usize::MAX>` with
  the required nightly feature. Preserved the parent's strengthened sealing
  example that implements every public method before failing on the sealed bound.

Focused post-review checks passed:

- `devenv shell -- cargo test --locked -p quest-polynomial --test typed_function`
  — 10 tests, including native DAG/tiny-budget/iteration-accounting and original
  typed/static/conditional failure-retention regressions.
- `devenv shell -- cargo test --locked -p quest-qsp --features offline-synthesis --lib offline::remez`
  — 7 tests, including policy-preflight rejection of the depth-71 shared DAG.
- `devenv shell -- cargo clippy --locked -p quest-polynomial --test typed_function --no-deps`
  — passed, apart from the documented nightly generic-const solver warning.

Sources are stable; parent owns the final integrated compiler/doctest/test gate.

Final integration follow-up: updated
`offline_remez_rejects_domain_and_insufficient_error_budget` to assert the
specific logarithm-domain error at policy admission, after successful cheap
interval-geometry construction. This preserves a meaningful domain assertion
under the intentional deferred-evaluation contract. Its focused cargo test
passed; the insufficient-error-budget/retained-export half also passed.
