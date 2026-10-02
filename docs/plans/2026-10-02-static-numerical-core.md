> Historical architecture plan. Its arithmetic dependency choice is superseded by [Dashu consolidation](2026-10-02-dashu-consolidation.md).

# Static numerical architecture and whole-project consolidation

User-approved implementation plan, based on `001a2b656a5a80a60659a408f87f57670309a09b`.

## Binding requirements

Retain statically dispatched f64 alongside multiprecision. Break APIs and versioned
formats deliberately; remove legacy interpreters, forwarding shims and needless
aliases. Add rigorous MP interval certification. Preserve independent QSP and
exact-circuit verifier arithmetic, exact-angle semantics, NLFT defaults, native
ownership/ABI checks and scientifically meaningful runtime dimensions.
The C++ checkout and Zotero are read-only. No push; any commits must be unsigned.

## Implementation tasks

1. Establish baseline evidence and a removal/retention inventory. Record toolchain,
   exact inputs, matched-accuracy timings, allocations, peak storage, compile cost
   and code size. Existing audit receipts remain historical.
2. Put arithmetic/enclosure interfaces and concrete f64, interval64, MP point and
   directed MP interval backends in quest-numerics. Preserve exact f64 constants;
   add exact integer/rational/decimal inputs. Supply statically selected AD orders
   and vector Jacobians. Audited backends are the only unconditional proof sources;
   open functions/backends require retained assumptions. MP errors precede all
   comparisons/shortcuts. Trigonometric bounds include possible interior extrema
   using outward pi and exact integer bounds. Record modeled resource limits.
3. Make Function<E> and static function! canonical; delete interpreted Expr,
   to_dynamic, duplicate Callable/Real/backend delegation and compatibility APIs.
   Keep move-only targets and non-Copy scalars. Shared basis recurrences compute
   at backend precision. Generalize polynomial coefficients and shapes.
4. Generalize Newton/Krawczyk/Hansen-Sengupta and one deterministic root-cover
   driver. Track exclusion, unresolved coverage, existence, at-most-one and
   uniqueness separately. Preserve split branches and partial coverage on budgets.
   Use static dimensions with admitted heap storage for large workspaces.
5. Implement one owning generic Remez exchange engine. Keep distinct candidate,
   uniform-error and minimax-gap products; certify actual selected/exported
   coefficients. Retain optimized faer f64 QR and MP pivoted Householder QR with
   one diagnostics contract. Static/runtime degree policies share the engine.
   Every failure retains original function/request and attempts; retries bounded.
   MP proof removes enclosure precision floor but not f64 coefficient export error.
6. Remove quest-circuit facade, compiler forwarding modules/import hacks and
   native-synthesis worker relay. Make generators generic; separate native and
   process features. Replace erased compiler errors/JSON evidence with typed
   compiler-owned representations. Replace synthetic AST/placeholder capture
   imports by checked semantic/SSA insertion preserving exact data and effects.
   Increment changed formats only; reject old formats without a legacy decoder.
7. Consolidate native matrix preparation/accounting. Remove binding-generator
   bootstrap fallback/unused generated outputs. Use QUEST_ROOT installation prefix
   and conventional CMAKE_PREFIX_PATH; retain ABI/discovery/ownership correctness.
8. Add compiler contracts and adversarial mathematical tests, independent test-only
   MPFR/Rug oracle checks, whole-workspace/feature/scalar-SIMD/parallel checks,
   native consumers, binding freshness, docs/Clippy/fmt and performance comparisons.
   Investigate reproducible regressions; justify retained specialized kernels.
9. Independent mathematical and architectural reviews, migration guide, complete
   removal/retention ledger and reproducible validation records. State unavailable
   Linux/MPI/accelerator evidence explicitly.

## Acceptance scenarios

Const dimension/order/admission failures; conditional evidence cannot become
unconditional. Exact constant/target retention; lower-than-binary64 MP proof;
unattainable f64 export rejection. MP subnormals, cancellation, exponent limits,
division poles, sin/cos extrema and huge arguments, sqrt/log derivative domains.
Repeated/endpoint roots, constant residual derivatives, zero/singular
preconditioners, splits, stalls and incomplete coverage. Full QSP/GQSP/QSVT
responses, nullspaces, rectangular/complex cases, exact full-phase replay,
clean ancillas and forged workers. Runtime/build benchmarks compare matched
accuracy against the baseline, not only expression microbenchmarks.

## Ownership and interfaces

Numerics owner supplies arithmetic/AD/enclosure and generic contractor contracts;
polynomial/Remez owner consumes them without depending on QSP. Compiler owner
owns compiler/language/macro/facade migration; native owner owns build/generator
and runtime preparation. Root owns integration, cross-owner API migration,
validation, evidence and review. Shared Cargo edits are coordinated explicitly.
