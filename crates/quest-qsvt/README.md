# quest-qsvt

Owned projected encodings, quantum singular value transforms, and bounded dense
references in pure Rust. This crate constructs immutable circuit models without
initializing `QuEST`. Native preparation, execution, retained mass, and consuming
postselection live in the `quest` facade's `qsvt` module.

Start with the [workspace QSVT guide](https://github.com/eessmann/quest-rs/blob/main/docs/book/src/qsvt-model.md),
then continue to the [native runtime guide](https://github.com/eessmann/quest-rs/blob/main/docs/book/src/qsvt-runtime.md).
The [QSP crate](https://github.com/eessmann/quest-rs/blob/main/crates/quest-qsp/README.md)
owns phase/control sequences, synthesis, and independent certification;
[quest-qsvt-io](https://github.com/eessmann/quest-rs/blob/main/crates/quest-qsvt-io/README.md)
owns JSON, inverse catalogs, and optional HDF5 interchange.

## Quickstart: encode a matrix and inspect its transform

Both builders consume their inputs. A dense encoding requires an explicit
positive normalization before `build()` is available; a transform requires an
encoding and a typed route. This example needs no installed native `QuEST` package.
The two symmetric phases implement the degree-one response `p(x) = x`, so the
result is the normalized block `A / alpha`, with its complex phase preserved.

<!-- ANCHOR: pure_transform -->
```rust
use faer::mat;
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{Complex64, DenseEncodingBuilder, NumericalPolicy, TransformBuilder};

fn main() -> quest_qsvt::Result<()> {
    let matrix = mat![[Complex64::new(0.3, 0.4)]];
    let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
        .normalization(2.0)?
        .build()?;
    let phases =
        PhaseSequence::<WxSymmetric>::builder(vec![std::f64::consts::FRAC_PI_4; 2])
            .build()?;
    let transform = TransformBuilder::new()
        .encoding(encoding)
        .standard(phases)
        .build()?;

    let block = transform.materialize_block()?;
    assert!((block[(0, 0)] - Complex64::new(0.15, 0.2)).norm() < 1e-12);
    assert_eq!(transform.normalization().get(), 2.0);
    assert_eq!(transform.degree(), 1);
    assert_eq!(transform.query_counts().source_forward, 2);
    assert_eq!(transform.query_counts().source_adjoint, 0);
    Ok(())
}
```
<!-- ANCHOR_END: pure_transform -->

`DenseEncodingBuilder` snapshots the logical complex values of a `faer` view,
including strided and conjugated views. It pads to a power-of-two size and builds
a numerical Julia dilation of `A / alpha`, checking contraction, PSD roots, and
whole-oracle unitarity. The original row and column dimensions remain separate.
Choose `alpha` to make the supplied matrix a contraction after division; the
builder rejects insufficient normalization instead of choosing a new scale.

## Encodings and logical spaces

For an oracle `U` on a physical space of dimension `2^n`, write `L` and `R` for
the ordered left and right logical isometries. The encoding represents

```text
B = L† U R,           A = alpha B,
L: C^m -> C^(2^n),    R: C^k -> C^(2^n),    A: C^k -> C^m.
```

`LogicalSpace<Left>` and `LogicalSpace<Right>` prevent swapping the two
interfaces. Their logical dimensions `m` and `k` need not agree, and logical
dimensions need not be powers of two. Their physical dimensions must both match
the oracle. `encoding.logical_matrix()` returns `A`, including `alpha`;
`transform.materialize_block()` returns the extracted, subnormalized transform
block, with no automatic multiplication by `alpha`.

For an existing coherent circuit, use `EncodingBuilder`. All four required
fields can be supplied in any order, but `build()` is unavailable until all are
present. This example retains the caller's left coordinate order `[1, 0]` and a
one-dimensional right interface:

```rust
use quest_circuit::{Gate, OracleFragment, ProgramBuilder};
use quest_qsvt::{EncodingBuilder, Left, LogicalSpace, NumericalPolicy, Right};

fn main() -> quest_qsvt::Result<()> {
    let policy = NumericalPolicy::default();
    let mut body = ProgramBuilder::new(1, 0)?;
    body.gate(Gate::H, &[body.qubit(0)?], &[])?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let encoding = EncodingBuilder::new()
        .oracle(oracle)
        .left(LogicalSpace::<Left>::coordinates(2, &[1, 0], policy)?)
        .right(LogicalSpace::<Right>::coordinates(2, &[0], policy)?)
        .normalization(1.0)?
        .policy(policy)
        .build()?;
    let block = encoding.logical_matrix()?;
    assert_eq!((block.nrows(), block.ncols()), (2, 1));
    Ok(())
}
```

Choose the logical-space constructor from the representation you actually have:

| Constructor | Retained representation and admission |
| --- | --- |
| `coordinates(dimension, indices, policy)` | Exact computational coordinates, in caller order; rejects duplicates and invalid indices. Storage scales with the list length. |
| `from_isometry(basis, policy)` | Copies complex `V` and numerically checks `V†V` against identity at `1e-12`. |
| `from_dense_projector(projector, basis, policy)` | Retains both `P` and ordered `V`; additionally checks `P` against `VV†` at `1e-12`. |

Coordinate spaces remain compact: `dense_isometry()` returns `None` for them,
and the fallible `isometry_snapshot(policy)` explicitly requests a dense copy.
Small entries in a dense basis never become exact coordinate membership.
Projection and conditioning use canonical `VV†`. For a supplied dense projector,
`projector_matrix()` returns the retained `P` used for coherent reflections;
its measured discrepancy from `VV†` does not establish exact agreement.

Default `EncodingBuilder::build()` materializes and checks the complete oracle
with a fixed `1e-12` numerical unitarity tolerance. Individual gate-admission
tolerances do not replace that check. For a large structured source with an
external mathematical argument, explicitly call
`.unitarity_assumption(ExplicitUnitaryPremise::new(description)?)` before building.
This avoids the dense whole-oracle check and records `OracleUnitarityEvidence::Assumed`.
It still checks dimensions and retained storage. Neither measured residuals nor
an explicit premise grant symbolic inverse cancellation or a theorem certificate.

## Select a route explicitly

All routes use the normalized block `B = A / alpha`. Let
`H_B = [[0, B], [B†, 0]]` with the left space first, and `G = B†B` on the right
space. For generalized controls, let `p` denote their Chebyshev polynomial:
the coefficients of the upper-left entry of
`C_0 diag(z,1) C_1 ... diag(z,1) C_d` become the coefficients of `T_j(x)`.
The following ideal polynomial descriptions require the corresponding exact
unitary/subspace hypotheses; construction preserves a numerical circuit and
records admission evidence.

| Builder method | Input to output | Extracted response and requirements |
| --- | --- | --- |
| `standard(phases)` | Right to right for even `d`; right to left for odd `d` | Standard singular-value response from typed Wx symmetric, Wx Laurent, or canonical imaginary-U00 phases. |
| `direct(controls)` | Right to right | `p(B)`; checks the **whole oracle** for Hermiticity and checks both projectors and ordered logical bases for agreement. A Hermitian projected block alone is insufficient. |
| `hermitianized_full(controls)` | Joint left/right to joint left/right | `p(H_B)` on the direct sum, ordered left then right. |
| `hermitianized_even(controls)` | Right to right | The right-right block of `p(H_B)`, selecting the even response. |
| `hermitianized_odd(controls)` | Right to left | The left-right block of `p(H_B)`, selecting the odd response. |
| `multiplication_even(controls)` | Right to right | `p(G)`; controls describe a reduced polynomial in the Gram matrix. |
| `multiplication_odd(controls)` | Right to left | `B p(G)`; retains an intermediate right-space projection and a final forward source call. |

The multiplication methods do not extract the parity of an arbitrary polynomial
in `B`. Their degree is the **reduced control degree** `d`; the corresponding
singular-value expressions have degree at most `2d` and `2d+1`. Their
`normalization()` still reports the original `alpha`: `G = A†A / alpha²`, and the
odd continuation applies `A / alpha`. That continuation remains present at
reduced degree zero.

The generalized routes consume a `quest_qsp::ControlSequence`, whose nonempty,
finite, immutable matrices are admitted by its builder. For example:

```rust
use faer::mat;
use quest_qsp::ControlSequence;
use quest_qsvt::{Complex64, DenseEncodingBuilder, NumericalPolicy, TransformBuilder};

fn main() -> quest_qsvt::Result<()> {
    let matrix = mat![[Complex64::new(0.3, 0.4)]];
    let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
        .normalization(1.0)?
        .build()?;
    let controls = ControlSequence::builder().angles(&[0.31], &[0.47])?.build()?;
    let transform = TransformBuilder::new()
        .encoding(encoding)
        .multiplication_odd(controls)
        .build()?;
    assert_eq!(transform.degree(), 0);
    assert!(transform.bridge().is_some());
    assert!(transform.continuation().is_some());
    assert_eq!(transform.query_counts().source_forward, 1);
    assert_eq!(transform.query_counts().source_adjoint, 0);
    let block = transform.materialize_block()?;
    let expected = Complex64::new(0.3, 0.4)
        * Complex64::from_polar(0.31_f64.sin(), 0.47);
    assert!((block[(0, 0)] - expected).norm() < 1e-12);
    Ok(())
}
```

## Phases, stages, and physical operands

Standard construction converts the typed source convention to projector phases
and retains the source tag in `convention()`. The source convention includes
special degree-zero and degree-one readout: symmetric phases `[phi]` extract
`sin(phi) I`, while `[phi, phi]` extract `sin(2 phi) B`. Conversions retain a
separate `phase_conversion_roundoff_estimate`; this is a diagnostic estimate,
not an outward certificate. An upstream QSP polynomial certificate does not
automatically cover the actual rounded conversion or subsequent circuit lowering.

Generalized construction applies the stored `C_d` first, followed by walk/control
rounds in descending control order. It preserves the final K convention and the
complete complex scalar phase already present in the supplied controls. It does
not refactor matrices into angles. Hermitianization retains a shared structural
barred oracle with forward and numerical-adjoint source branches.

A `ValidatedTransform` has execution order

```text
input embedding -> main -> optional bridge projection
                -> optional continuation -> output extraction
```

If these embeddings are `V_in`, `V_bridge`, and `V_out`, its cold reference is
`V_out† U_cont V_bridge V_bridge† U_main V_in`, omitting absent stages.
`main()` and `continuation()` are coherent `BoundProgram`s; projections are
separate objects. The complete staged transform cannot be passed as an oracle.
Running `main()` alone omits the staged extraction contract, and for the odd
multiplication route also omits its bridge and continuation.

The default `OperandLayout` places response at qubit 0, auxiliary at qubit 1
when the route needs one, and source targets in increasing order after them.
Standard and direct routes use one extra qubit; all other routes use two.
Use `.operands(OperandLayout::new(width, ordered_source, response, auxiliary)?)`
to select physical positions. Source target order is significant: local source
bit `j` maps to `ordered_source[j]`, including nonsorted layouts. Active positions
must be distinct and cover the register. `with_idle_high_qubits(n)` explicitly
appends high qubits; every projection fixes them to zero.

## Query accounting and resource limits

`degree()` and `query_counts().semantic` count the phase/control degree `d`.
The forward/adjoint fields count retained applications of the supplied source,
including controlled branches and the odd multiplication continuation:

| Route | Forward source calls | Adjoint source calls |
| --- | --- | --- |
| Standard | `d + (d mod 2)` | `d - (d mod 2)` |
| Direct | `d` | `0` |
| Any Hermitianized route | `d` | `d` |
| Multiplication even | `d` | `d` |
| Multiplication odd | `d + 1` | `d` |

`retained_oracle_calls` additionally counts structural wrapper instructions and
nested source calls. None of these is a native FFI-call or density-operation
count; native preparation accounts its selected execution operations separately.

`NumericalPolicy { max_bytes }` bounds numerical storage and associated work;
the default is 64 MiB. It cannot change mathematical admission tolerances.
Dense dilation, `materialize_oracle`, `materialize_program`, `logical_matrix`,
`materialize_block`, and diagnostics are explicit cold references whose storage
grows with physical Hilbert-space dimension. A compact encoding need not fit
these dense operations. Dense factory and reference kernels use `faer::Par::Seq`.
Failures return errors without changing the normalization, numerical precision,
or selected method. A successful materialization returns the subnormalized
logical block and does not condition or renormalize it.

## Conditional bounds and numerical diagnostics

`analysis` separates caller-stated mathematical guarantees from observations.
An analysis report requires a `BlockEncodingBound` with explicit normalization,
absolute error, and a named contraction/encoding assumption. Standard conditional
bounds additionally require `StandardPremises` attached to the exact immutable
transform, covering projected-unitary semantics, the actual frozen phase response,
and parity/completion. A matching clone is still a different transform for this
purpose. A transform by itself returns `None` from `theorem_error_bound()`.

Generalized routes retain uncertified reports and may use a separately supplied
full-oracle error for query telescoping, but do not receive the standard theorem
bound. Dense-SVD and seeded-power-iteration diagnostics preserve full complex
phase and return empirical discrepancies; `is_certified_upper_bound()` is always
false. They do not turn observations into theorem premises or measure native
`QuEST` execution. See the [analysis contract](https://github.com/eessmann/quest-rs/blob/main/crates/quest-qsvt/docs/analysis.md)
for bound composition, normalization enclosures, certificate limits, and diagnostic
reference semantics.

Run the executable crate documentation with
`cargo test -p quest-qsvt --doc --locked --offline`. The
[native QSVT example](https://github.com/eessmann/quest-rs/blob/main/crates/quest/examples/qsvt.rs)
continues from a constructed transform through admission, preparation, execution,
conditioning, and reusable overlap resources.

This Rust port draws on `quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`.
Its MIT notice is retained in `LICENSE-quest-qsvt`.
