# Conditional bounds and numerical observations

`quest_qsvt::analysis` is pure Rust and does not initialize `QuEST` or MPI.
`ValidatedTransform::analysis()` computes conditional analytical reports;
`diagnostics()` computes finite observations of the pure circuit model. Neither
operation changes a transform, synthesizes phases, or normalizes a state.

## Explicit mathematical premises

A `BlockEncodingBound` states

```text
||A - alpha L† U R|| <= epsilon,
||A/alpha|| <= 1,  ||L†UR|| <= 1.
```

Normalization, absolute error, and `assume_contract(Assumption)` are required
builder states. `Assumption::stated(...)` requires the caller to name the asserted
mathematical fact. A separate `full_oracle_error(delta, assumption)` describes a
full-unitary error; a projected-block error never supplies it automatically.

Standard-QSVT reports additionally require `StandardPremises::for_transform(t)`
and all three named premise groups:

- exact projected-unitary/subspace semantics, the standard extraction, and
  applicability of the supplied source bound;
- linkage of the actual frozen phases and convention conversions to the claimed
  polynomial response and transform degree;
- required degree parity, polynomial contractivity, and QSP completion hypotheses.

These tokens borrow the exact immutable transform. A token for a different
transform object is rejected, including a content-equivalent clone. The API does
not accept a synthesis certificate for unrelated or pre-conversion phases.
Rounded phase shifts do not inherit exact linkage from an earlier payload.
A caller must establish the premise for the actual frozen transform or leave the
report uncertified. Numerical observations remain available separately through
`TransformReport::observations()` and have no conversion into theorem premises.

With those explicit premises, the standard robustness contribution is
`4*d*sqrt(epsilon/alpha)`. This is the setting of Lemma 22 in the first arXiv
version of [Gilyén, Su, Low and Wiebe, Quantum singular value transformation and
beyond](https://arxiv.org/pdf/1806.01838). The returned status is
`ConditionalOnExplicitPremises`, never a claim that evaluated binary64 matrices
have been proved exactly unitary. Optional caller-stated approximation,
synthesis, and execution budgets describe absolute error of the final extracted
operator. Omitting an execution term provides no native-execution guarantee.

Generalized routes cannot obtain the standard premise type. Their
`uncertified()` reports retain optional full-oracle query telescoping but expose
no standard robustness or total theorem bound. The distinction follows the
separate route setting in [Sünderhauf, Generalized Quantum Singular Value
Transformation](https://arxiv.org/abs/2312.00723). The legacy
`ValidatedTransform::theorem_error_bound()` remains `None`: a transform alone
contains no caller-supplied theorem premises.

The following helper shows the complete standard report transition. It receives
the source guarantee and mathematical assumptions from its caller; supplying
these named statements remains the caller's responsibility. The API checks
normalization, route, and transform identity, and encloses the stated formula.
It does not prove these assumptions from finite construction residuals.

```rust
use quest_qsvt::{ValidatedTransform, analysis};
use analysis::{Assumption, BlockEncodingBound, StandardPremises, TransformReport};

fn conditional_standard_report<'t>(
    transform: &'t ValidatedTransform,
    source_bound: BlockEncodingBound,
    exact_subspaces: Assumption,
    actual_phase_response: Assumption,
    parity_and_completion: Assumption,
) -> analysis::Result<TransformReport<'t>> {
    let premises = StandardPremises::for_transform(transform)?
        .assume_projected_unitary_subspaces(exact_subspaces)
        .assume_actual_phase_response(actual_phase_response)
        .assume_parity_and_completion(parity_and_completion)
        .build();
    transform
        .analysis()
        .encoding_bound(source_bound)?
        .standard_premises(premises)?
        .build()
}
```

To retain a report without the standard theorem premises, use
`transform.analysis().encoding_bound(source_bound)?.uncertified()?`. Its
`robustness()` and `total()` remain `None`; a separately stated full-oracle error
can still supply `oracle_telescoping()`.

## Outward arithmetic and normalization

Formulas use `quest_numerics::Interval` and its directed binary64 arithmetic
contract. Exact input binary64 scalars are point intervals. Composition retains
an interval for the exact normalization expression; its rounded midpoint never
becomes a new exact normalization implicitly. Attaching a composed guarantee to
a transform requires its normalization interval to equal that transform's exact
normalization point. Nonfinite, negative, overflowing, and incompatible values
are rejected.

Under the two contraction premises, product normalized error is `eL + eR`, where
`e = epsilon/alpha`. Product normalization is enclosed as `alphaL*alphaR`, giving
the corresponding absolute bound `alphaL*epsilonR + alphaR*epsilonL`.
Hermitianization preserves the complete normalization/error enclosure and named
assumptions. Full-oracle errors add only when both product operands supplied
them; otherwise the optional full-oracle term remains absent.

Full-oracle telescoping uses **physical forward plus adjoint source calls** from
transform metadata, rather than semantic degree. This deliberately corrects the
old C++ generalized report's Hermitianized query-count limitation. The current
standard extracted-response circuit also retains more physical source calls
than its polynomial degree. Every interval query count is converted exactly;
counts exceeding `u32::MAX` are rejected rather than rounded.

## Empirical references

`Reference::dense()` owns finite supplied complex values.
`Reference::svd_polynomial()` numerically constructs an odd rectangular
`U p(Sigma) V†` map or an even right-space `V p(Sigma) V†` map. The even reference
includes `p(0)` on the full right nullspace. Odd references require exactly zero
`p(0)`; callback parity and approximation quality are not certified.

Diagnostics subtract full complex operators without global-phase alignment or
renormalization. Dense mode computes a numerical SVD spectral norm. Sampled mode
uses deterministic complex sign probes from a fixed `SplitMix64` stream and
`D†D` power iterations. Method, seed, probes, iterations and route are recorded.
Repeated seeds are reproducible on the same numerical platform. Both methods
report `is_certified_upper_bound() == false`; finite arithmetic provides neither
a rigorous sampled lower bound nor a rigorous SVD upper bound.

These are explicit cold, bounded dense materializations of the pure model, not
native `QuEST` measurements, matrix-free estimators, or distributed gather APIs.
Allocation, solver scratch, active nested oracle-decomposition metadata, and
expanded operation work are checked before construction. Diagnostics pass their
own admitted policy into the shared pure materialization kernel; a larger policy
on the original encoding cannot bypass that budget. Failures do not switch to
a different numerical method or precision.

For an empirical comparison, pass the complete expected **subnormalized**
logical operator, with shape matching output logical dimension by input logical
dimension. The following helper runs dense-SVD mode; add
`.sampled(probes, iterations, seed)?` before `.run()` to select reproducible
power-iteration observations instead. Both modes materialize dense operators.

```rust
use quest_qsvt::{Complex64, NumericalPolicy, ValidatedTransform, analysis};
use analysis::{EmpiricalEvidence, Reference};

fn compare_reference(
    transform: &ValidatedTransform,
    expected: faer::MatRef<'_, Complex64>,
    policy: NumericalPolicy,
) -> analysis::Result<EmpiricalEvidence> {
    let reference = Reference::dense(expected, policy)?;
    transform.diagnostics().policy(policy).reference(reference).run()
}
```

Return to the [pure QSVT guide](https://github.com/eessmann/quest-rs/blob/main/docs/book/src/qsvt-model.md)
for encoding/route construction, or the [native runtime guide](https://github.com/eessmann/quest-rs/blob/main/docs/book/src/qsvt-runtime.md)
for execution and postselection. These observations cover the pure retained
model and do not include native runtime errors.
