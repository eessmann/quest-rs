# Carleman and the common DG history solver

The `--lift carleman` route uses the same complete physical dynamics as the
classical reference. It replaces the configuration grid by a finite polynomial
hierarchy. `--lift kvn` keeps the configuration-space half-density route. Both
assemble one causal temporal DG history and use the direct non-Hermitian inverse
orientation described in [the method](method-and-theory.md).

## Physical polynomial forms and shared algebra

`PolynomialOde` imports one MathCore sparse polynomial for every physical
coordinate. The symbol order contains every state variable, followed optionally
by time. State degree is at most two; coefficients may be polynomials in time.
Nonpolynomial expressions require an explicit different model and are rejected.

For a known lifting `a=b+l(t)`, `with_lifting` substitutes the complete vector
and subtracts its MathCore derivative. This produces

\[
\dot b=F(b+l(t),t)-\dot l(t).
\]

The shared engine differentiates and extracts coefficients during construction.
Point and interval kernels freeze coefficients once; numerical evaluations do
not construct symbolic expressions. The [MathCore provenance](../../vendor/mathcore/UPSTREAM.md)
records the maintained fork. Exact recorded coefficients, their numerical
assembly error, and ordered binary64 evaluation are distinct contracts.

Burgers uses direct polynomial DG forms. Existing BDM1 models have a bounded
numerical coefficient-snapshot bridge: centered polarization of the known
quadratic residual, followed by independent full-state probes. Every nonzero
recorded coefficient is retained. Its receipt reports residual calls, numerical
probe errors and an explicitly heuristic roundoff estimate. This bridge is not
an exact symbolic proof of the original floating-point assembly and does not
certify that an arbitrary black-box residual is quadratic.

## All normalized symmetric monomials

Write the complete physical system as

\[
\dot a=F_0(t)+F_1(t)a+F_2(t)(a\otimes a),\qquad b=a/s,\quad s>0.
\]

Then the coefficients of the scaled drift are `F0/s`, `F1` and `s F2`.
For every multi-index of degree one through `r`, retain

\[
y_\alpha=c_\alpha b^\alpha,\qquad
c_\alpha=\sqrt{\frac{|\alpha|!}{\alpha!}}.
\]

If component `i` of the physical drift contains the monomial
`f[i,p](t) a^p`, the chain rule contributes to row `alpha`, column
`beta=alpha-e_i+p`, with coefficient

\[
\alpha_i\frac{c_\alpha}{c_\beta}\,s^{|p|-1}f_{i,p}(t).
\]

Use `c[0]=1`. Degree-zero terms form the external source; terms above degree
`r` are the explicitly omitted closure. The implementation enumerates sparse
coefficient recipes, rather than a dense quadratic tensor. Materialized
multi-index vectors still have one exponent per physical coordinate, and the
recipe constructor enforces dimension, degree, work and storage limits.

The full hierarchy dimension is

\[
D_r=\sum_{k=1}^r {m+k-1\choose k}={m+r\choose r}-1.
\]

This removes duplicate permutations in ordered tensors, not physical modes.
For a word with occupation numbers `alpha`, the independent ordered reference
contains `b^alpha`. The embedding from symmetric coordinates assigns
`y_alpha/sqrt(|alpha|!/alpha!)` to each such word. Its columns are orthonormal.
Tests check this isometry and generator intertwining on arbitrary lifted states,
including forcing and mixed quadratic terms.

## Scaling and evidence

The current execution route is experimental: nonzero initial states use
`s=2||a0||`, unless an explicit positive `--carleman-scale` is supplied. A forced
zero initial state uses a reported positive scale based on the forcing bound and
horizon, with `RC` undefined. An identically zero trajectory returns exact zero
without preparing a normalized quantum state or allocating its history.

`coefficient_evidence` computes outward bounds for the actual recorded `F0`,
`F1` and symmetrically distributed ordered-pair `F2`. Its decay evidence is a
Gershgorin upper bound for the logarithmic norm of `F1`; negative eigenvalues
alone are not used as a norm-decay certificate. The bound may be inconclusive
for a dissipative matrix. The extra scaled-forcing inequality is checked using
outward endpoints, so rounded equality cannot falsely establish the premise.
An explicit scale override is distinguished from evidence for the default scale.

The corrected [Liu analysis](https://arxiv.org/html/2011.03185v4) motivates the
nonlinearity/forcing-to-dissipation diagnostic. Its sufficient hypotheses and
complexity analysis are not automatically inherited by our temporal DG scheme.
A small `RC`, short-time agreement, or small register alone does not prove the
complete algorithm accurate or efficient. [Jennings et al.](https://arxiv.org/abs/2509.07155v2)
develop Lyapunov and conservation-aware extensions. Applying them requires a
specific verified metric or conserved-subspace construction and all applicable
hypotheses. The current report therefore never promotes coefficient diagnostics
to a complete convergence certificate. Periodic mean-flow coordinates remain
present even when they prevent a strictly dissipative full-space certificate.

## Histories and physical recovery

`HistoryDynamics` visits the generator and source at each temporal quadrature
node. Both DG1 and DG2 are supported. DG1 bounds the stored slab Hermitian part
with outward Gershgorin arithmetic. DG2 instead uses a small exact temporal
inverse and an interval Neumann-residual test. Either bound can reject a history;
allocation or a plausible spectrum is not substituted for a positive bound.

The inverse solver encodes the adjoint required by its singular-vector
convention and never forms normal equations. Its default coherent RHS circuit
is the [amplitude-tree baseline](../../../docs/research/coherent-rhs-preparation.md).
Direct simulator initialization is an explicitly selected alternative.

Physical recovery reads every degree-one coordinate and multiplies by `s`.
Reports retain the inverse scaling, physical RHS norm, inverse success
probability, final-time/degree-one selection probabilities and solution norm.
The present bounded demonstration reads the full simulated state; it does not
claim a certified efficient observable-sampling procedure.

Two diagnostics have different inputs and meanings:

- The **chain-rule truncation defect** compares the full chain rule at a physical
  vector with the truncated hierarchy evaluated on its exact monomial lift.
- The **physical reconstruction defect** compares `s P1 (H y+f)` with
  `F(s P1 y)` for the actual hierarchy vector. It can detect inconsistent moments
  even when the top-order truncation defect at a lifted physical vector is small.

Neither is, by itself, an integrated error bound. Physical mesh, temporal DG,
Carleman order, coefficient extraction, block encoding, polynomial synthesis,
state preparation and readout errors require separate evidence.

## Runnable Burgers experiments

The frozen default is four DG1 cells on `[0,1]`, homogeneous Dirichlet data,
viscosity `0.1`, Cole–Hopf parameter `0.01`, horizon `0.1`, and two DG1 time
slabs. All eight physical coordinates are retained. Run from the repository root:

```sh
cargo run -p quest-cfd -- reference --case burgers
cargo run -p quest-cfd -- build --case burgers --lift carleman --carleman-order 2
cargo run -p quest-cfd -- estimate --case burgers --lift carleman --carleman-order 4
cargo run -p quest-cfd -- reference --case burgers --cells 8
cargo run -p quest-cfd -- reference --case burgers --physical-order 2
cargo run -p quest-cfd -- build --case burgers --lift carleman --carleman-order 3 --time-cells 4
```

A deliberately shorter circuit experiment can be admitted independently:

```sh
cargo run -p quest-cfd --features quantum -- solve --case burgers \
  --lift carleman --carleman-order 2 --horizon 0.001 --time-cells 1 \
  --max-degree 8191 --no-certify --backend quest-cpu
```

This last command executes the native circuit but skips the optional independent
projector-phase certificate; its report preserves that missing evidence. It is
not the default-window convergence campaign. Omit `--no-certify` to request the
certificate and pay its separate construction cost. Set `QUEST_ROOT` and `MPICC`
to a matching native installation when required. A rejection remains a result;
no failed circuit falls back to a classical PDE solve.

## Streaming the complete time history

`TemporalHistoryRecipe` accepts a `HistoryRowDynamics` implementation and emits
only a caller-owned row range. It retains fixed DG1/DG2 temporal coefficients,
one row buffer and the admitted dynamics kernel. The input ordinal is derived
from the global row and the term's reserved slot, so repartitioning does not
change duplicate summation order. `rhs_value` evaluates one source coefficient
and, in the initial block, calls a scalar initial-state recipe. It never builds
a complete RHS.

```rust,ignore
use quest_cfd::stream_history::{HistoryStreamLimits, TemporalHistoryRecipe};
let history = TemporalHistoryRecipe::new(
    &hierarchy, horizon, time_cells, time_order, HistoryStreamLimits::default(),
)?;
let local_coo = history.rows(first_owned_row..last_owned_row)?;
// Feed the iterator directly into the distributed sparse producer. For the
// reciprocal convention, transpose indices and conjugate values to encode H†.
let rhs_at = |index| history.rhs_value(index, |row| {
    Ok(hierarchy.lift_entry(row, &physical_initial)?.into())
});
```

The materialized `SymmetricCarleman` supplies row and scalar-initial queries by
searching its immutable, row-ordered recipes. Its complete monomial catalogue
and physical polynomial kernel still have their own explicit construction
limits. Streaming time history does not make those retained resources sharded.
The row stream reports their storage and query costs. A source recipe is a
classical producer, not a free coherent oracle; the stored matching/QROM and
coherent preparation costs still apply after production.

`tests/stream_history.rs` compares every entry and RHS coefficient with the
independent stored history on forced, time-dependent DG1/DG2 cases and the full
eight-coordinate Burgers hierarchy. It also checks local queries in a
six-billion-row logical history and malformed-provider termination. That large
logical count tests indexing and bounded row storage; it is not an actual
large-count transfer or multi-host capacity measurement.

## Optional continuous Carleman truncation bound

`carleman_certificate::admit_truncation` checks a conservative sufficient
condition for the **recorded polynomial ODE**. The CLI flag
`--certify-carleman` requires this admission and selects its checked scale;
`--carleman-scale` remains an incompatible experimental override. This flag is
separate from the inverse-polynomial phase certification switch. It does not
certify spatial errors, numerical coefficient extraction, temporal DG,
preparation, encoding, inverse approximation, or measurement.

Let `A` enclose the initial Euclidean norm, `b` bound the spectral norm of the
ordered-pair quadratic coefficient, `c` bound the forcing norm throughout the
horizon, and `mu_2(F1) <= -gamma < 0`. The implementation requires autonomous
`F1,F2`, a nonzero initial state, and

\[
 b A+c/A<\gamma,\qquad
 \rho=A/s<1,\qquad b_s=sb,\quad c_s=c/s,\qquad
 b_s+c_s<\gamma.
\]

The first inequality makes the radius-`A` ball forward invariant by the scalar
norm differential inequality. It therefore bounds every exact lifted block
by `rho^j`. The corrected forcing condition is checked using an upper forcing
bound and a *lower* quadratic-norm bound, after scaling. This is stronger than
comparing two upper bounds. Candidate scales use ordinary floating arithmetic;
all admitted inequalities and the final error bound use outward intervals.
Failure to find an admissible candidate is inconclusive.

For clarity, the contraction argument used here is stronger than an assumption
on real parts of eigenvalues. For a hierarchy block vector with block norms
`v_j`, the Hermitian part is bounded by a tridiagonal scalar comparison matrix.
Its diagonal is at most `-j gamma`, and the off-diagonal between blocks `j` and
`j+1` is at most `(j b_s+(j+1)c_s)/2`. An interior row sum is bounded by
`j(b_s+c_s-gamma)+(c_s-b_s)/2`, hence is nonpositive when `c_s <= b_s`.
The endpoint rows omit nonnegative terms. The same argument is uniform in
time for polynomial forcing. Thus the truncated hierarchy propagator is a
contraction. The degree-`r` omitted coupling has norm at most
`r b_s rho^(r+1)`. Duhamel's formula gives the physical degree-one error bound

\[
 \|a(t)-s\widehat y_1(t)\|_2
 \le s\,t\,r\,b_s\,\rho^{r+1},\qquad 0\le t\le T.
\]

The normalized symmetric hierarchy inherits this bound by its tested isometric
embedding into ordered tensor powers. No eigenvector condition number or
unstated removal of mean coordinates is used. The implementation uses the
upper endpoint at `T` for the whole interval. Conserved periodic means usually
prevent this full-space admission; the experimental route and independent
refinement remain available. Zero initial states retain their explicitly
labelled zero/forcing route.

This construction incorporates the additional scaled-forcing restriction in
[Liu et al., corrected v4](https://arxiv.org/html/2011.03185v4) while supplying
its own logarithmic-norm contraction argument. It does not transplant that
paper's Euler-history complexity theorem to the temporal DG solver. Tests
compare scalar quadratic dynamics with an independent analytic solution,
exercise forcing that requires changing `2||a0||`, and reject nonnormal,
undamped, zero-initial and time-varying-linear counterexamples.
