# Sources, attribution and reading guide

This bibliography records the sources behind the method and the proposed
comparisons. A cited theorem applies only under its stated hypotheses; a paper
using a different discretization is motivation, not an acceptance certificate
for this implementation. Source details were checked on 2026-10-05. The supplied
structured-encoding and Laneve papers are linked at their supplied version 2.

Read [method and theory](method-and-theory.md) for the derivation,
[quantum and distributed execution](quantum-and-distributed.md) for encoding,
solver and runtime contracts, and [alternatives](alternatives.md) for comparison
experiments. The separate [next-step plan](../NEXT_STEPS.md) identifies missing
proofs, implementations and measurements.

## Primary method

### Physical incompressibility and complete DG coordinates

**Guosheng Fu, “An explicit divergence-free DG method for incompressible flow.”**
[Preprint](https://arxiv.org/abs/1808.04669),
[journal article](https://doi.org/10.1016/j.cma.2018.11.012).

Use this for divergence-conforming velocity spaces and the relationship between
divergence-free evolution and a global mixed Poisson problem. It helps explain
why local physical matrices do not imply a local pressure-eliminated drift.
The crate specifies its own central convection, SIP viscosity, complete mass
chart and boundary lifting. Those forms and their convergence need independent
verification; citing Fu does not establish their equivalence to every operator
in his paper.

### Unitary KvN construction

**Aleksandar Jemcov and Scott C. Morris, “Unitary discretization of the
Koopman–von Neumann equation for quantum simulation of fluid and plasma
dynamics.”** [Version 1](https://arxiv.org/html/2605.19187v1).

The central connection is Weyl-ordered transport of a half-density under a
nonlinear drift. The crate uses the weighted discrete anticommutator to obtain
an algebraically skew-adjoint generator. The paper's spectral examples and
absorbing treatment are distinct from this crate's complete physical DG chart,
configuration DG and global causal time system. In particular, a per-step
absorbing construction has not been transferred to the global inverse.

### Structured block encodings

**Christoph Sünderhauf, Earl Campbell and Joan Camps, “Block-encoding structured
matrices for data input in quantum computing.”**
[Supplied version 2](https://arxiv.org/abs/2302.10949v2),
[Quantum 8, 1226 (2024)](https://doi.org/10.22331/q-2024-01-11-1226).

Use this for repeated-data/index-map constructions, normalization and the
conditions needed by the PREP/UNPREP variant. Coefficient repetition alone does
not establish the required oracle identities. The current generic weighted
matching encoding and arithmetic tensor operators are useful baselines; the
paper's complete base/PREP/UNPREP construction remains a planned extension.

### Singular-value transformation and reciprocal solves

**András Gilyén, Yuan Su, Guang Hao Low and Nathan Wiebe, “Quantum singular value
transformation and beyond: exponential improvements for quantum matrix
arithmetics.”** [Preprint](https://arxiv.org/abs/1806.01838),
[STOC 2019](https://doi.org/10.1145/3313276.3316366).

Use this for projected-unitary singular-value transformations and their
polynomial conditions. Applying a reciprocal requires a justified singular
gap, the correct left/right orientation and physical rescaling. Query
complexity does not include an arbitrary classical matrix loader, RHS
preparation or full-field reconstruction for free. The crate's causal DG
matrix and its spectral evidence are separate constructions.

### GQSP and phase synthesis

**Lorenzo Laneve, “Generalized Quantum Signal Processing and Non-Linear Fourier
Transform are equivalent.”**
[Supplied version 2](https://arxiv.org/abs/2503.03026v2).

This connects GQSP with an SU(2) nonlinear Fourier transform and provides phase
synthesis theory. It does not supply the Navier–Stokes lift, the history
matrix, a reciprocal spectral gap or a distributed data oracle. The actual
projector-phase payload used by the solver must be certified in its own circuit
convention after numerical synthesis.

**Danial Motlagh and Nathan Wiebe, “Generalized Quantum Signal Processing.”**
[Preprint](https://arxiv.org/abs/2308.01501).

Read this alongside Laneve for the generalized processing rotations and
polynomial transformations. A future GQSP execution path needs its own signal
and inverse interpretation; a synthesis relationship does not make distinct
circuit conventions interchangeable.

## Alternatives and access-model assumptions

| Source | What to investigate | Assumption to carry into an experiment |
| --- | --- | --- |
| Shi Jin, Nana Liu and Yue Yu, [Quantum simulation of partial differential equations via Schrödingerisation](https://arxiv.org/abs/2212.13969), with [technical details](https://arxiv.org/abs/2212.14703) | Hamiltonian embedding of nonunitary evolution | Auxiliary-domain error, solution recovery and nonlinear lifting or outer iteration remain explicit costs |
| Guang Hao Low and Isaac L. Chuang, [Hamiltonian Simulation by Qubitization](https://arxiv.org/abs/1610.06546) | Direct evolution of the same full-KvN Hamiltonian | Efficient coherent access and its normalization must be supplied |
| Dominic W. Berry, Andrew M. Childs, Aaron Ostrander and Guoming Wang, [Quantum algorithm for linear differential equations with exponentially improved dependence on precision](https://arxiv.org/abs/1701.03684) | Time evolution represented through a quantum linear solve | Its linear-system construction and bounds do not automatically apply to this DG history |
| Jin-Peng Liu, Herman Øie Kolden, Hari K. Krovi, Nuno F. Loureiro, Konstantina Trivisa and Andrew M. Childs, [Efficient quantum algorithm for dissipative nonlinear differential equations](https://arxiv.org/abs/2011.03185) | Carleman lifting of the full polynomial drift | Hierarchy truncation and all corrected dissipativity/forcing hypotheses must be checked |
| I. V. Oseledets, [Tensor-Train Decomposition](https://doi.org/10.1137/090752286) | Exact tensor structure and controlled tensor approximation | Rank growth and rank-truncation error are separate from physical mode removal |
| Vittorio Giovannetti, Seth Lloyd and Lorenzo Maccone, [Quantum random access memory](https://arxiv.org/abs/0708.1879) | Coherent stored-data access | Ordinary distributed CSR and MPI statevector simulation do not supply qRAM hardware |
| PETSc development team, [SNES nonlinear solvers](https://petsc.org/release/manual/snes/) and [KSP linear solvers](https://petsc.org/release/manual/ksp/) | Classical nonlinear/Krylov comparison methods | Count preconditioning, iteration, assembly and data movement; no PETSc integration is currently claimed |

The Liu et al. supporting information has a
[2026 correction](https://doi.org/10.1073/pnas.2615307123), published online on
2026-05-20. Its revised theorem and lemma add the condition
$\|F_0\|\leq\|F_2\|$ alongside $R<1$, in that paper's quadratic-system
notation and with its other assumptions retained. Use the
[corrected text](https://pmc.ncbi.nlm.nih.gov/articles/PMC13213974/) when checking
a forced DG system. Neither viscosity alone nor the original abstract is a
sufficient convergence test.

The [alternatives chapter](alternatives.md) translates these sources into
experiments at equal physical error and observable requirements. No source in
this table is evidence that a comparison has already been executed here.

## Physical benchmark definitions

Manifests in [cases](../cases) are the executable case contracts. A published
reference and a project-selected initialization or observation window must be
identified separately. A coarse snapshot demonstrates code execution; it does
not establish agreement with a benchmark table or continuum solution.

| Family | Source or reference definition | Attribution boundary |
| --- | --- | --- |
| 2D Taylor–Green | [Manifest](../cases/tgv2d.json) and [case implementation](../src/cases.rs) | Analytic velocity and pressure provide a manufactured/reference solution under the stated periodic domain, amplitudes and viscosity |
| 3D Taylor–Green | [Manifest](../cases/tgv3d.json) | The crate freezes its initial field and Re100/Re1600 conventions; resource estimates and coarse runs do not establish a converged turbulence reference |
| 2D cavity | U. Ghia, K. N. Ghia and C. T. Shin, [High-Re solutions for incompressible flow using the Navier-Stokes equations and a multigrid method](https://doi.org/10.1016/0021-9991(82)90058-4), JCP 48, 387–411 (1982) | Compare profiles using the same lid speed, length, Reynolds number and steady-state criterion |
| 3D cavity | S. Albensoeder and H. C. Kuhlmann, [Accurate three-dimensional lid-driven cavity flow](https://doi.org/10.1016/j.jcp.2004.12.024), JCP 206, 536–558 (2005) | Stationary side/end walls and lid-edge singularities matter; silently smoothing the lid changes the problem |
| DFG 2D-2 cylinder | [Official Re100 benchmark](https://wwwold.mathematik.tu-dortmund.de/~featflow/en/benchmarks/cfdbenchmarking/flow/dfg_benchmark2_re100.html) | Preserve geometry, parabolic inflow, Reynolds/force conventions and probe positions; refine polygonal cylinder error independently |
| 3D stationary cylinder | Youngjae Kim, Vedasri Godavarthi, Laura Victoria Rolandi, Joseph T. Klamo and Kunihiko Taira, [Influence of three-dimensionality on wake synchronisation of an oscillatory cylinder](https://doi.org/10.1017/jfm.2024.1079), JFM 1001, A24 (2024); [preprint](https://arxiv.org/abs/2411.06279) | The selected reference is the stationary validation setup within a study of oscillatory cylinders; this crate does not request moving-cylinder dynamics |

The 3D wake's approved Neumann far field, convective outlet and periodic span
are currently an execution boundary: the backend rejects that physical run.
Its symbolic resource estimate does not relax the boundary contract or the
minimum observed-cycle requirement. See [physical foundation](../foundation.md)
and [next steps](../NEXT_STEPS.md) for the remaining work.

## Repository evidence and provenance

* [Accepted implementation plan](../../../docs/superpowers/plans/2026-10-04-quest-cfd.md)
  records the primary algorithm and acceptance requirements.
* [2026-10-04 verification record](../../../docs/verification/2026-10-04-quest-cfd.md)
  records the specific tests and bounded experiments run for the existing
  implementation. It is historical evidence, not a claim of fresh execution
  during this documentation update.
* [Distributed matching contract](../../../docs/research/distributed-matching.md)
  describes runtime ownership, admission and the source-bound matching path.

Future results should cite a source revision, manifest revision, command,
budgets, spectral/error evidence and raw receipts. Record failed admission and
unsupported environments alongside successes. A local circuit simulation, a
classical CFD validation and a multi-host capacity result answer different
questions and should retain separate status labels.
