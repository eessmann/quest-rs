# Full-DG KvN and the causal history system

This derivation states the finite-dimensional operator implemented in
`quest-cfd`. Algebraic identities and convergence conditions have different
roles; numerical preservation of an identity is not a convergence proof.

## Complete constraint elimination

Collect normal-trace matching, prescribed normal velocity and element divergence
into `C u = g`. The physical pressure part is `B u = d`; closed-domain pressure
has a volume-weighted zero-mean gauge. The small classical reference computes a
particular stationary boundary lift `u_l` and a basis `Q` for the complete kernel
of C, with `Q^T M Q = I`. It represents `u = u_l + Q a`. There is no truncation of
`a`, and both the rank of C and its residual on Q are checked.

For a stationary lifting,

    a_dot = Q^T r_h(u_l + Q a) = F(a).

Constraint reactions are reconstructed using the same complete mass and
constraint matrices. Momentum and continuity residuals are checked in original
physical coordinates, independently of the reduced coordinate representation.
The coordinate representation eliminates constraints but is not a reduced-order
model. In particular, global pressure elimination generally makes F dense in a.
Current reference chart construction is bounded dense work; this cost is not
hidden behind the sparse configuration operator.

Central normal-flux convection with divergence-conforming velocities gives
`a^T F_convection(a)=0` for homogeneous periodic boundaries. Symmetric
interior-penalty viscosity dissipates kinetic energy when its penalty is
coercive. Nonhomogeneous boundaries add explicit boundary work. These identities
are checked on the actual assembled triangular/tetrahedral DG operator.

## Half-density transport

The density satisfies the Liouville equation `rho_t + div(F rho)=0`. Writing
`rho=|psi|^2`, its unitary half-density transport is

    psi_t = -F . grad(psi) - (div F) psi/2.

For central configuration DG, the discrete derivative satisfies
`D_j^* W = -W D_j`. Real drift multiplication F_j commutes with diagonal W.
Consequently

    L = -1/2 sum_j (F_j D_j + D_j F_j)
    L^* W + W L = 0.

The implemented quantum amplitudes are `z=W^(1/2) psi`, with generator
`G=W^(1/2) L W^(-1/2)`. G is skew-adjoint in Euclidean coordinates. Tensor mass
underflow/overflow is rejected. The configuration dimension is exactly
`[cells*(order+1)]^dim(a)` and admitted before tensor arrays are allocated.

The numerical domain uses periodic central closure. A nonperiodic quadratic
physical drift is not made periodic by this choice: agreement with the desired
transport requires negligible probability at the artificial boundary over the
entire horizon. The implementation records boundary mass; it does not add a
per-step absorber.

On a smooth periodic coefficient extension, with regularized initial amplitude
and sufficient DG regularity, central SBP consistency together with the
skew-adjoint energy estimate bounds the semidiscrete error by the accumulated
consistency defect. If a smooth exact solution is inserted into the discrete
operator, its weighted residual tends to zero under refinement; Duhamel's
formula bounds the solution error by its initial projection error plus the time
integral of that residual. This conditional argument requires coefficient and
solution regularity and compatibility at the artificial boundary. It does not
establish the vanishing-width deterministic limit or domain-truncation error.
Manufactured constant-drift waves test configuration consistency independently
of skew-adjointness. A full nonlinear CFD convergence campaign remains required.

## Global causal DG time

For each temporal element of length dt, use Lobatto basis nodes xi, weights w
and reference derivative D. Temporal weak assembly gives

    K_ab = -D_ba w_b + delta(a,last) delta(b,last)
    A_slab = K tensor I - diag(dt*w/2) tensor G.

The previous slab's right trace enters the current left test equation with
coefficient -1. Only the first left trace contains the initial source z0. All
slabs are assembled into one non-Hermitian A. The unknown stores spatially
mass-weighted coefficients; temporal quadrature weights are retained separately.
Neither `A^* A` nor normal equations are formed.

For temporal order one, K=[[1/2,1/2],[-1/2,1/2]]. With exactly skew-adjoint G,
each slab's Hermitian part is `(1/2)I`, so its inverse norm is at most 2. For the
stored floating-point A, interval arithmetic bounds the Hermitian defect of
each actual diagonal slab; write the resulting positive coercivity constant c.
Causal off-diagonal trace blocks have norm one. The finite triangular inverse
series gives

    norm(A^-1) <= sum_(k=1)^number_of_slabs c^(-k).

Its reciprocal is a conservative lower singular-value bound. A sparse
`sqrt(norm_1(A)*norm_inf(A))` bound gives the upper endpoint. Proof work and
storage receive separate admission. The bound is intentionally conservative
and does not claim favorable conditioning for long horizons.

## Encoding, inversion and observables

The stored sparse path colors a bipartite row/column graph into partial
matchings. Each matching is completed by closing directed paths into cycles;
untouched basis labels stay fixed. Let K be the padded color count and beta the
largest entry magnitude. Uniform color preparation, coefficient-controlled flag
rotations, complex success-sector phases and SELECT permutations give

    <0_color,0_flag| U |0_color,0_flag> = A / (K beta).

All unsuccessful branches participate in the same unitary. Sparse storage,
preprocessing, retained permutation metadata, gate replay and state simulation
costs are explicit and distinct. Tensor modular shifts have direct arithmetic
circuits with normalization one; no unverified PREP/UNPREP identity is assumed.

The reciprocal approximation starts from

    p(x) = c [1-(1-x^2)^b] / x,
    c <= 1/(2 sqrt(b)).

Positive binomial tails produce its odd Chebyshev coefficients. Truncation and
coefficient rounding are bounded by their coefficient one-norms; the remaining
analytic error on `|x|>=delta` is `c (1-delta^2)^b/delta`. NLFT synthesizes the
frozen polynomial, and the actual converted projector phases are independently
certified and retained by the replay schedule.

To solve `A x=b_rhs`, the matching source encodes **A adjoint**. Odd singular-value
transformation then maps the left singular basis of A to its right singular
basis. The unnormalized successful amplitudes y obey

    x_approx = norm(b_rhs) y / (alpha*c).

The measured success mass, physical rescaling and residual of the exported
rounded vector are retained. Readout uses half-density probability for energy,
coordinate moments and concentration. Sampling error, physical DG error,
regularization width, configuration truncation/resolution, temporal error,
encoding error, polynomial error and native execution error require separate
budgets. A small residual of the assembled history system addresses only part
of this list.
