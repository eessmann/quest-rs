# Shared MathCore algebra and numerical lowering

The maintained `quest-mathcore` package is a normal workspace dependency under
the Rust name `mathcore`. [Pinned upstream and fork provenance](../../crates/vendor/mathcore/UPSTREAM.md)
and the preserved MIT license distinguish the maintained source from the original
archive. The migration restores the current audited Dashu-based core, not the old
approximate CAS or epsilon-based simplifier.

| Owner | Shared responsibility | Consumer boundary |
|---|---|---|
| MathCore | Scoped symbols, exact affine constants, ordered typed/dynamic expressions, bounded differentiation/substitution and rational sparse multivariate polynomials | No native execution or numerical backend implementation |
| `quest-symbolic` | Quantum-angle interface, original binding/domain obligations, independent source replay and transformation checks | Compatibility affine re-export; the superseded affine engine was removed |
| `quest-numerics` | Binary64, arbitrary precision, interval and automatic differentiation backends, sparse storage and execution budgets | Re-exports neutral MathCore backend/constant contracts |
| `quest-polynomial` | Basis conversion, approximation and function orchestration, exact target lowering and coefficient-rounding evidence | Reuses shared expression nodes instead of a duplicate expression hierarchy |
| Language/compiler | Parsing, types, integer widths, conversions, effects and control flow | Shared scalar operations underneath those semantics; lazy Boolean constant evaluation |
| QSP/QSVT | Target functions, polynomial synthesis, owned operator compositions and independent certificates | Shared exact targets; certificate checkers stay independent |
| CFD | Reference forms, finite-element basis derivatives, complete polynomial ODE coefficients and resource formulas | Prepares numerical kernels once; lift row queries do not invoke symbolic simplification |

Dependencies flow from neutral MathCore interfaces toward backend implementations
and domain consumers. Moving those interfaces into the foundation prevents a
MathCore/numerics/polynomial dependency cycle. Existing valid public entry points
are re-exported. Specialized Clifford algebra and independent interval/circuit
certificate checkers remain separate; sharing the engine being verified with its
checker would weaken the evidence.

## Exact identities versus ordered evaluation

`ExactConstant` distinguishes rational and decimal inputs, symbolic π and stored
binary64 values. The latter preserve their bit patterns, including signed zero.
Importing a binary64 value explicitly as an exact dyadic rational changes its zero
semantics; that choice must precede polynomial extraction. A decimal `0.1` is not
silently equated with binary64 `0.1`.

Exact polynomial canonicalization proves identities in its declared rational
algebra. It does not license reassociation of ordered floating arithmetic, erase
original bindings or remove domains. Dynamic differentiation retains an original
source guard, so a simplified derivative cannot hide an undefined source such as
division by zero. Typed evaluation preserves operand order and conversions.
Compile-time Boolean evaluation now short-circuits: an inactive right operand is
not evaluated, while typing and effects retain their language-level checks.

Expression nodes/depth, coefficient growth, expansion work, stored capacities,
scope-validation scratch and lowering overlap have explicit admission limits.
Cancellation does not make oversized input capacities free. Cached or shared
representations retain logical source obligations. Numerical kernels retain the
precision and converted coefficients selected at lowering; changing that policy
requires lowering again. A kernel is not a new exact-source certificate.

## Real cross-layer use and evidence

`Function::dynamic` captures static source nodes into scoped dynamic forms.
The shared polynomial extractor returns the canonical exact polynomial together
with its ordered source. `ExactMonomialTarget` retains that source, selected
binary64 coefficients and an outward coefficient-rounding bound on `[-1,1]` for
QSP/QSVT targets. Approximation, basis conversion, phase response and simulator
errors remain separately accounted.

`PolynomialOde` retains every physical coordinate and extracts quadratic state
coefficients with time as an external symbol. Burgers, doubled-field KdV and BDM
adapters compare prepared evaluation against independent physical residuals.
The bounded BDM snapshot uses centered polarization of known quadratic dynamics;
its exact dyadic representation does not prove the original floating assembly
exact. Stateless Carleman and KvN recipes consume these prepared kernels and
report all retained physical forms and repeated drift-query work.

The [stateless Carleman recipe](stateless-carleman-recipe.md) generates complete
normalized symmetric indices/rows without a hierarchy catalog. The reviewed
[full-coordinate KvN consumer](../../crates/quest-cfd/docs/distributed-history.md#generated-full-coordinate-kvn-consumer)
evaluates those prepared physical kernels at each derivative neighbor rather
than retaining a configuration-wide drift table. These are actual generated
consumers, with retained physical forms and repeated scalar work charged. The
separate [generated complete physical-force source](../verification/2026-10-05-generated-physical-force.md)
also prepares MathCore basis kernels once, lowers fixed cell/facet quadrature
tables, and evaluates full force from owned cells and bounded neighbour data.
Its collective drift and these independently queried polynomial row recipes have
different scheduling contracts; safe physical-to-history integration requires an
explicit collective construction stage.
The [Carleman/KdV scoped receipts](../verification/2026-10-05-dual-history.md)
separate coefficient/lift tests and classical order refinement from the short
local native inverse, while [KvN diagnostics](../../crates/quest-cfd/docs/kvn-refinement.md)
retain unresolved resolution, regularization and boundary errors. Exact shared
algebra does not supply the missing total physical error or convergence bound.

Focused regression entry points are:

```sh
cargo test -p quest-mathcore --test consolidation --test scope_admission
cargo test -p quest-polynomial --test static_core
cargo test -p quest-language --test ssa_contract
cargo test -p quest-qsp --test mathcore_target
cargo test -p quest-qsvt --test mathcore_target
cargo test -p quest-cfd --test polynomial_dynamics --test physical_order
```

These checks cover shared consumers, source guards, exact constant distinctions,
short-circuit behavior, capacity/work admission and independent transformations.
They supplement workspace/all-feature and native verification; they do not
establish that every symbolic expression or physical model is supported.
