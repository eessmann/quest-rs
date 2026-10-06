# Coherent right-hand-side preparation

`quest_qsvt::state_preparation::AmplitudePreparation` is a stored-table baseline
for preparing an arbitrary nonzero complex vector. It constructs a unitary whose
first column is the normalized vector, including its complex phase and padded
zeros. It retains elementary rotation coefficients and streams gates in either
direction; it never constructs a dense isometry or stores the full circuit.

The construction follows the uniformly controlled rotation method of
[Möttönen et al.](https://arxiv.org/html/quant-ph/0407010v1), with the repository's
little-endian bit layout and rotation convention. It preserves the global phase
because that phase becomes observable when the preparation is controlled.

## Tree and rotation conventions

Pad the input to dimension $D=2^n$. Each binary-tree leaf stores its complex
phase and a scaled magnitude. Internal nodes store the Euclidean norm of their
two child magnitudes. A node whose children have magnitudes $l,r$ has
$y$ rotation angle $2\operatorname{atan2}(r,l)$. A completely zero subtree uses
angle zero. No epsilon comparison establishes a zero subtree.

For the phases, propagate child averages upward and store their difference:
$\phi=(\phi_L+\phi_R)/2$ and $\theta_z=\phi_R-\phi_L$. The root average supplies
the final scalar phase. This uses the actual leaf arguments without rewrapping
intermediate averages. The phase of an exactly zero leaf can be chosen freely;
it is fixed to zero for deterministic construction.

At depth $d$, the $M=2^d$ angles define a uniformly controlled rotation on target
bit $n-d-1$. Its elementary angles are

$$
\beta_j=\frac1M\sum_{k=0}^{M-1}(-1)^{k\cdot g(j)}\theta_k,
\qquad g(j)=j\mathbin{\mathrm{xor}}(j\!\gg\!1).
$$

A normalized Walsh transform computes these coefficients. Alternate each
rotation with the CNOT selected by the bit changed between consecutive cyclic
Gray words. For $M=1$ there is no CNOT. A $z$ rotation is represented by a scalar
phase $-\beta/2$ and a phase $\beta$ conditional on its target being one. The
adjoint reverses all primitive order and angle signs. Signed outer controls and
operand remapping also apply to scalar phases.

## Explicit costs and limits

For $D>1$ the unoptimized baseline emits $5D-6$ elementary primitives; for
$D=1$ it emits the scalar phase. Table storage is $2(D-1)+1$ coefficients.
Compilation takes $O(D\log D)$ arithmetic work and $O(D)$ temporary storage;
resource admission charges the simultaneous input, magnitude/phase trees and
retained coefficients. Cloning the prepared owner shares immutable storage.

This is a linear gate-count baseline, not a logarithmic-cost coherent lookup.
Explicit tables and their preprocessing/loading remain costs of an arbitrary
stored RHS. The local constructor takes a complete bounded input slice.
The separate collective
`CollectiveEnvironment::prepare_amplitudes_from_fn` implements scalar-query
tree compilation and sharded coefficient ownership. It charges retained tables,
query/compile/transport work and native replay scratch, preserving phase,
signed controls and literal adjoints. The local API alone does not provide that
capability; the [streamed history adapter](../../crates/quest-cfd/docs/distributed-history.md)
uses the implemented collective producer without a full RHS on any rank.

The implementation uses binary64 norms and angles and reports no independently
certified preparation-error bound. Tiny normalized components may underflow;
applications must retain preparation error separately from inverse-polynomial
and measurement error. An unrepresentable physical norm is rejected. An exactly
zero input has no normalized amplitude state and is rejected; an identically
zero physical problem should return its separately labelled exact zero solution.

Tests cover complex padded RHS values, all-branch forward/adjoint action,
spectators, and negative controls. The CFD history solver uses this circuit by
default (`--rhs-preparation coherent`), with scalar and native CPU QuEST tests
against a known complex constant-history solution. Its report separates table
compilation, gate count, physical RHS norm, circuit preparation time and the
currently unavailable independent preparation-error certificate.

`--rhs-preparation simulator-initialization` retains the separately labelled
direct classical initialization path. Collective preparation tests cover
1/2/4/8 ranks and split communicators; the reviewed generated KvN history
consumer separately exercises actual 1–2-rank preparation and inversion.
Local MPI evidence does not establish multihost or actual large-count capacity,
an independent preparation-error certificate, or efficient loading of arbitrary
exponentially large data.
