# Alternatives and research decisions

The accepted route retains every independent physical DG velocity coordinate,
lifts its nonlinear dynamics to KvN, assembles a causal DG history system, and
applies a QSVT reciprocal through a coherent block encoding. Its implementation
and mathematical obligations are described in
[Method and theory](method-and-theory.md); execution boundaries are in
[Quantum and distributed execution](quantum-and-distributed.md).

Carleman is now an implemented comparison route using the same global history
machinery, with [separate derivations and error contracts](carleman-history.md).
The [encoding portfolio](../../quest-qsvt/docs/portfolio.md) also implements
bounded structured and sparse-access comparisons. Direct Hamiltonian evolution,
Schrödingerisation, hybrid nonlinear iterations and compression remain research
alternatives. The current classical references and admitted inverse circuits
provide experiments with which to assess them.
Neither a reduced physical model nor an estimate of an unexecuted system
satisfies the accepted full-KvN solve requirement. Preserving all physical
coordinates also does not eliminate configuration discretization error.

## Decisions to test

This table states comparison criteria. Implemented routes still require equal
accuracy and complete preparation/readout accounting; the table does not report
measured advantages.

| Route | Potential reason to investigate | Costs or errors requiring evidence | First discriminating experiment |
| --- | --- | --- | --- |
| Direct evolution of the KvN Hamiltonian | A few observation times; avoid history inversion | Hamiltonian normalization, evolution horizon, time dependence, measurement repetitions | Same generator and initial state, fixed observable accuracy, increasing horizon |
| Schrödingerisation | A naturally linear, nonunitary system | Auxiliary coordinate, recovery probability, truncation and preparation | Linear viscous test with independent auxiliary-domain refinement |
| Picard/Newton/Oseen plus quantum linear solves | Reuse mature nonlinear CFD iterations | Outer convergence, repeated loading, classical reconstruction | Count every iteration and transfer against a preconditioned classical solve |
| Carleman lift (implemented) | Polynomial drift with favorable dissipativity | Hierarchy truncation, corrected theorem hypotheses, solution decay | Full five-coordinate smoke system; increase polynomial degree |
| Matrix-free constraint projection | Avoid storing a dense nullspace chart | Global pressure solves and their tolerances | Compare reconstructed drift and pressure against exact bounded elimination |
| Fourier, Hermite, or tensor formats | Exploit smoothness, decay, or separability | Boundary artifacts, weights, tensor ranks and truncation | Match physical observables while refining each approximation independently |
| Paper-specific structured encoding (bounded portfolio implemented) | Repeated coefficients and reversible index rules | Normalization, arithmetic, workspace and loading | Same extracted matrix/cost comparison; each construction's full controls and adjoints |

## Schrödingerisation and direct Hamiltonian evolution

Schrödingerisation and KvN solve different initial problems. Jin, Liu and Yu
transform a linear nonunitary evolution into a larger Hamiltonian evolution
using an auxiliary coordinate. Their detailed treatment also discusses
nonlinear equations through a Liouville representation. Applying the transform
directly to a frozen Oseen or linearized Navier–Stokes operator therefore solves
that linear problem; recovering the original nonlinear dynamics still requires
an outer iteration or a nonlinear lift. See their
[Schrödingerisation paper](https://arxiv.org/abs/2212.13969) and
[technical analysis](https://arxiv.org/abs/2212.14703).

For `quest-cfd`, the immediate comparison would use a purely viscous linear
subproblem. Refine the auxiliary interval and resolution separately, measure
the probability and normalization required to recover the physical solution,
and charge preparation and recovery to the algorithm. A second experiment
could apply Schrödingerisation to a genuinely non-skew configuration operator.
For the current skew-Hermitian, mass-weighted KvN generator, adding another
coordinate is not needed to obtain a Hermitian Hamiltonian.

In fact, with the repository convention

$$
\dot z=Gz,\qquad G^\dagger=-G,
$$

the definition $H=iG$ gives $H^\dagger=H$ and, for autonomous dynamics,
$z(t)=e^{-itH}z(0)$. Time-dependent boundary lifting instead produces a
time-dependent Hamiltonian and requires time ordering. KvN norm conservation
does not imply conservation of physical kinetic energy: the latter is an
observable of a probability distribution transported by a potentially
contracting viscous flow.

Jemcov–Morris supply the unitary discretization motivation and finite-dimensional
fluid examples. Their spectral truncations and per-step absorbing construction
do not establish convergence of this repository's full-DG global history
extension. An absorbing operation would require a separately derived global
formulation and error accounting.
[Jemcov–Morris](https://arxiv.org/html/2605.19187v1)

Direct Hamiltonian simulation is thus the closest alternative that preserves
the same nonlinear lift. Qubitization offers a principled implementation once
coherent Hamiltonian access is supplied; its query bounds concern that access
model, with the Hamiltonian normalization entering the effective evolution
time. They do not price arbitrary matrix loading or observable extraction.
[Low–Chuang](https://arxiv.org/abs/1610.06546)

## Propagation versus an entire time horizon

The history approach trades sequential evolution for a larger linear system;
it does not remove dependence on the requested horizon. For a history operator
$A_T$ encoded with normalization $\alpha_T$, reciprocal synthesis must
resolve a certified normalized singular-value gap
$\delta=\sigma_{\min}(A_T)/\alpha_T$. Temporal refinement, horizon,
weighting, and encoding normalization can all affect this quantity. Eigenvalues
alone are insufficient for a nonnormal causal operator.

QSVT acts on singular values through the projected unitary and its adjoint.
The left/right projector orientation is essential for implementing an inverse
of a non-Hermitian history operator; forming normal equations is unnecessary.
[Gilyén–Su–Low–Wiebe](https://arxiv.org/abs/1806.01838)

Quantum linear-ODE algorithms already demonstrate evolution encoded into a
linear system, but their constructions and bounds are not automatic bounds for
this DG temporal matrix.
[Berry–Childs–Ostrander–Wang](https://arxiv.org/abs/1701.03684)

A useful comparison holds the spatial/configuration generator, initial
regularization, observable tolerance, and horizon fixed. Measure preparation,
oracle calls, polynomial degree, success probability, and sampling for both
routes. Then vary horizon and temporal resolution separately. A history state
may suit time-integrated quantities, but conditioning on a narrow time window
can reduce its usable probability. Direct evolution may suit a few output
times, yet repeated preparations and measurements remain costs. These are
testable preferences, not a guarantee that either organization is faster.

## Classical nonlinear iterations and Carleman lifts

Picard/Oseen iterations freeze transport coefficients; Newton solves a Jacobian
correction equation. A quantum linear solver could replace the inner solve
while the nonlinear iterate remains classical. This is a useful comparison
route, but it changes the accepted primary algorithm: it has no single full
nonlinear KvN evolution. Count assembly, preconditioning, iteration count,
state preparation, and any field reconstruction needed to form the next
iterate. PETSc's primary documentation provides concrete nonlinear and Krylov
solver baselines; this repository does not thereby gain a PETSc backend.
[SNES](https://petsc.org/release/manual/snes/),
[KSP](https://petsc.org/release/manual/ksp/)

Classical preconditioning also deserves investigation for the history solve.
It is useful only if its complete application cost is affordable. A classical
matrix-free preconditioner is not automatically a coherent quantum oracle.
Left and right preconditioning change RHS preparation, normalization, recovery,
and the relevant singular-value bound. Compare total work at the same physical
residual, including preconditioner setup and reuse.

Carleman linearization instead embeds a polynomial ODE into an infinite
hierarchy of monomials. Truncating that hierarchy introduces a distinct error;
it does not discretize the KvN amplitude equation. Liu and colleagues analyze a
quantum algorithm for dissipative quadratic systems under quantitative
conditions on dissipation, nonlinearity, forcing, and solution decay.
[Liu et al.](https://arxiv.org/abs/2011.03185)

Their 2026 correction adds $\|F_0\|\leq\|F_2\|$ to the stated
$R<1$ assumptions of the supporting theorem and lemma, in the paper's
quadratic-system notation. These conditions must be checked together with the
remaining hypotheses and scaling; they are not a universal Carleman
convergence criterion. Forced cavity or inflow problems cannot inherit a
guarantee from viscosity alone.
[Corrected supporting theorem, v4](https://arxiv.org/html/2011.03185v4)

The five-coordinate periodic smoke system retains its uniform mean coordinates,
so full-space strict viscous decay is unavailable. A conservation-aware theorem
would need to account explicitly for those coordinates and the complementary
subspace; removing them changes the problem. Burgers and the doubled-field KdV
implementation now have [order-refinement receipts](../../../docs/verification/2026-10-05-dual-history.md)
against their identical complete DG dynamics at T=0.1. These fixed-mesh classical
comparisons establish neither continuum error nor a quantum complexity benefit.

An optional conservative continuous truncation certificate uses actual
coefficient bounds, full-space logarithmic contraction, and the corrected
forcing condition after scaling. It deliberately returns inconclusive evidence
for the current undamped mean modes and conservative KdV examples. Its proof
and scope are in the [Carleman chapter](carleman-history.md). The broader
[Jennings et al. framework](https://arxiv.org/abs/2509.07155v2) remains a route to
investigate with a concrete metric/conservation construction and all hypotheses
checked; a citation alone does not supply those certificates.

## Physical discretization and exact constraint projection

A finite-volume fork could build a nonlinear ODE from conservative face fluxes
and lift every independent cell variable. It would need its own pressure
coupling, boundary lifting, conservation, and smoothness analysis. Switching
limiters or absolute-value wave-speed choices can complicate differentiability
of the drift. The present central/split DG design avoids making those switches
part of the first research problem. Neither local conservation nor formal
order alone establishes the weighted-adjoint KvN identity.

BDM/DG is valuable because the velocity and pressure spaces expose the
divergence constraint explicitly. There are alternatives to storing a complete
nullspace basis. On the crate's local coefficient space, let $\widehat C$ contain
an independent set of rows spanning **all** normal-continuity, prescribed-normal
and divergence constraints. For fixed mass matrix $M$, the constraint Schur
operator $S_C=\widehat C M^{-1}\widehat C^\top$ gives the exact homogeneous projector

$$
P_M=I-M^{-1}\widehat C^\top S_C^{-1}\widehat C.
$$

The independent-row restriction makes $S_C$ invertible; a formulation retaining
dependent rows needs an equivalent inverse on the compatible range. A
pressure-only formula using $B$ instead of $\widehat C$ is valid on an already
assembled, normal-continuous and boundary-admissible velocity space, with the
pressure gauge handled consistently. It is insufficient on unconstrained local
BDM coefficients. The homogeneous drift is $P_M M^{-1}r$, with the appropriate
lifting and its time derivative included for inhomogeneous constraints. This
retains the full constraint kernel, but constraint and pressure work remains
global. Fu's divergence-free DG method illustrates the connection between
constrained velocity evolution and a mixed Poisson solve.
[Fu](https://arxiv.org/abs/1808.04669)

The implemented [distributed Householder chart](physical-space.md#generated-constraints-and-distributed-complete-charts)
already avoids a global dense null basis, while retaining local dense fill and
globally coupled queries. Its [supplied-force pressure wrapper](physical-space.md#distributed-physical-pressure-from-a-broken-force)
is focused-tested and awaiting independent review; it does not generate nonlinear
physical force. A Schur-projector alternative to either chart requires more than
a matrix-free classical API. A KvN configuration chart, drift evaluation and
its derivatives must remain consistent; coherent execution must account for
the pressure work. First compare drift, reconstructed momentum and continuity
residuals with the current exact chart. Then tighten inner pressure tolerances
independently. An approximate projection creates an additional error budget,
even when every physical coordinate is retained.

## Configuration bases and tensor representations

DG in each configuration coordinate provides local polynomial structure and
explicit face fluxes. Fourier differentiation is an alternative for smooth
periodic amplitudes, but physical coefficient space is not inherently periodic.
Periodic wrapping at a truncated configuration boundary must be exposed by
domain refinement. A skew derivative alone does not show that this wrapping
approximates the desired transport.

Hermite functions offer a different proposed representation on an unbounded
coordinate domain. Their scale, tail resolution, quadrature and weighted
adjoints would need validation for the evolving distribution. Viscous
concentration can require finer representation even as physical energy falls.
For either basis, compare probability, energy, coordinate moments and boundary
or tail diagnostics while separately refining initial width, domain extent
and resolution. The current two-cell-per-coordinate smoke case cannot establish
small boundary leakage merely by inspecting boundary-cell mass: every cell
touches the boundary.

Exact tensor expressions can retain all coordinates without materializing the
full matrix. A Kronecker sum is an operator representation, not a reduced
physical model. Tensor-train representations likewise factor a tensor, but
useful compression depends on its ranks; rank truncation introduces its own
approximation.
[Oseledets, Tensor-Train Decomposition](https://doi.org/10.1137/090752286)

An exact tensor circuit preserving the full tensor index space is compatible
with the primary scope. A rounded tensor state is a separate numerical
approximation and must report rank growth, discarded error and convergence.
Removing physical velocity modes is excluded. Tensor compression must not be
used to replace the untruncated dimension in resource reports, and an efficient
classical tensor contraction is not automatically an efficient quantum
preparation circuit.

## Structure, stored sparse matrices, and qRAM

The implemented matching baseline handles general stored sparse coefficients
with $\alpha=K\beta$, where $K$ is the padded matching count and
$\beta=\max_{ij}|a_{ij}|$. Its complex rotations, completed permutations,
failure branches and dummy labels define an entire unitary. Lowering
normalization is valuable only alongside the cost of implementing that unitary.

Sünderhauf–Campbell–Camps exploit repeated data and reversible row/column index
maps. Their base normalization involves $\sqrt{S_cS_r}\max|a_{ij}|$,
with $S_c,S_r$ the scheme's index-label counts. Their PREP/UNPREP variant
requires the relevant index oracles to commute with the multiplexed data
rotations; coefficient repetition alone does not prove the required identities.
The paper also compares data-loading cost multiplied by normalization.
[Structured block encodings](https://arxiv.org/html/2302.10949v2)

The [implemented portfolio](../../quest-qsvt/docs/portfolio.md) supports explicitly
admitted arithmetic maps and PREP identities, not every scheme in that paper.
The [measured same-matrix fixture](../../quest-qsvt/docs/portfolio-comparison.md)
compares a four-site complex circulant across uniform/per-matching bounds,
SCC base/eligible PREP, explicit sparse QROM, weighted LCU, tensor product and
Kronecker sum. It independently extracts A and records normalization, actual
primitive/preparation counts, workspace, precision, managed storage and local
constructor timings. Whole unitaries can differ across constructions; their own
complex phases, adjoints, inactive controls, padding and failure branches require
separate behavioral checks. Native/equal-accuracy comparisons over larger sizes,
temporal DG stencils and complete CFD operators remain open. A better projected
block alone does not validate coherent composition or cheaper total loading.

qRAM would supply coherent access to stored data, a different assumption from
ordinary classical memory access.
[Giovannetti–Lloyd–Maccone](https://arxiv.org/abs/0708.1879)
No qRAM is supplied by distributing CSR records across MPI ranks. The fused
QuEST backend simulates coefficient rotations and permutation routing on
classical state partitions. Hardware estimates must specify the actual memory
oracle, initialization, update and fault-tolerance costs, or explicitly state
that these remain assumptions.

## Phase synthesis, baselines, and evidence

QSP, QSVT and GQSP are related circuit constructions, while NLFT is also a
classical route to their parameters. GQSP uses general SU(2) processing
rotations to broaden polynomial transformations of a unitary signal.
[Motlagh–Wiebe](https://arxiv.org/abs/2308.01501)
Laneve establishes an equivalence between GQSP and an SU(2) nonlinear Fourier
transform and develops synthesis connections.
[Laneve, supplied version](https://arxiv.org/abs/2503.03026v2)

The word “nonlinear” in NLFT does not solve the physical nonlinear ODE. The
current reciprocal path synthesizes and certifies the actual projector-phase
payload for its supported QSVT convention. A GQSP fork must establish its
signal, polynomial completion, left/right inverse interpretation and readout,
then certify rounded circuit parameters. Compare synthesis time and memory,
degree, achieved response error and executable gates at a common reciprocal
target. Do not infer better CFD accuracy solely from a faster phase engine.

Use two classical baselines: trajectories of the identical full constrained DG
ODE, and a classical solution of the identical KvN history discretization.
The first checks the lift and regularization; the second isolates encoding,
polynomial and execution error. Small dense references are useful within
explicit caps; larger comparisons need sparse iterative methods and independent
residuals. All routes should report the same observation windows and quantities.

A statevector run establishes simulated circuit behavior. It neither measures
quantum hardware runtime nor proves an advantage over classical CFD. Report
classical assembly, preparation, repeated execution and observable extraction
separately, with peak memory and maximum-rank MPI timings. Actual multi-host
capacity, hardware resources and converged benchmark campaigns retain their
own acceptance criteria in [Next steps](../NEXT_STEPS.md).
