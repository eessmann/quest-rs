# quest-cfd

Research example for complete incompressible BDM1/P0 and BDM2/P1 DG, nonlinear
Koopman–von Neumann (KvN) and Carleman lifts, and a global causal DG history
inverse using QSVT. Every
independent physical velocity coordinate is retained. The implementation makes
no reduced-order or hybrid nonlinear substitution.

## Method documentation

Read these chapters in order for the derivation, implementation contracts and
research choices:

1. [Method and theory](docs/method-and-theory.md): complete BDM constraints,
   nonlinear half-density KvN transport, weighted adjoints, configuration DG,
   global causal time DG, initial regularization and observable/error meaning.
2. [Quantum and distributed execution](docs/quantum-and-distributed.md):
   structured and stored sparse block encodings, matching unitaries, reciprocal
   QSVT scaling, preparation/readout costs and CPU/MPI ownership and capacity.
3. [Alternatives](docs/alternatives.md): direct Hamiltonian evolution,
   Schrödingerisation, classical nonlinear iterations, Carleman lifting,
   alternative bases, tensor representations and phase synthesis comparisons.
4. [Sources and attribution](docs/references.md): annotated primary references,
   benchmark definitions and the boundary between published results and this
   repository's extensions.
5. [Carleman histories](docs/carleman-history.md), [KdV](docs/kdv.md) and
   [higher physical order](docs/physical-space.md): complete coordinate spaces,
   additional approximation errors and executable scalar demonstrations.
6. [Streamed distributed histories](docs/distributed-history.md): generated rows,
   stateless Carleman/full-coordinate KvN consumers, coherent RHS preparation,
   direct non-Hermitian inversion and scalar reductions.
7. [Shared MathCore foundation](../vendor/mathcore/README.md): exact algebra,
   ordered evaluation, prepared kernels and bounded symbolic construction.
8. [KvN refinement diagnostics](docs/kvn-refinement.md): full-coordinate trajectory
   comparisons, independent sampling/domain refinements and concentration checks.
9. [Bounded reference spectral evidence](docs/reference-spectrum.md): independent
   interval inverse-residual checks for explicitly small stored DG1/DG2 histories.
10. [Paired histories](docs/paired-history.md): complete KvN and Carleman
    histories initialized from one sampled physical ensemble, with independent
    full-DG trajectories and separate temporal and hierarchy-order comparisons.
11. [Configuration boundary flux](../../docs/verification/2026-10-06-configuration-flux.md):
    bounded exterior trace rates, distinct from outer-cell occupation and
    integrated escape probabilities.
12. [Closed affine meshes](docs/affine-mesh.md): exact MathCore geometry,
    complete unequal-cell BDM spaces, topology admission, weighted pressure and
    independently reproducible rational fixture certificates.
13. [Mixed velocity and traction boundaries](docs/mixed-traction.md): complete
    open-domain constraints, absolute pressure, canonical mass-minimum lifting
    and polynomial-time boundary loads.
14. [Complete P2 cylinder snapshots](docs/cylinder-high-order.md): the full
    quadratic inlet trace, polygonal source identity, natural outlet, original
    pressure and labeled force, with cumulative construction/query admission.
15. [Bounded P2 cylinder evolution](docs/cylinder-p2-evolution.md): complete
    original drift, shared RK4, retained failure progress and a fixed temporal
    study whose failed accuracy criterion remains explicit.
16. [Initial configuration weak rates](docs/configuration-weak.md): streamed
    complex-amplitude contractions, original full-DG physical expectations,
    independent energy moments and complete sampling/work admission. The
    [fixed experiment](docs/configuration-weak-experiment.md) and its
    [seven-row outcomes](../../docs/verification/2026-10-06-configuration-weak-campaign.md)
    separate sampling bias, weak-rate defects and rejected requests. The
    [additive DG2 protocol](docs/configuration-weak-energy-shell-v2.md) tests
    nonconstant occupied energy; its [single-row evidence](../../docs/verification/2026-10-06-configuration-weak-energy-shell-v2.md)
    retains the larger defect and changed-quadrature interpretation.

The separate [TODO and next-step implementation plan](NEXT_STEPS.md) lists
remaining work, dependencies and acceptance gates. [Physical foundation](foundation.md)
is a shorter implementation/status reference. The chapters distinguish algebraic
identities, dated execution evidence and convergence claims still requiring work.

Current scoped evidence includes the [Burgers/KdV and temporal studies](../../docs/verification/2026-10-05-dual-history.md),
[KvN refinement diagnostics](docs/kvn-refinement.md),
[same-matrix encoding comparison](../quest-qsvt/docs/portfolio-comparison.md) and
[capped local sparse pipeline](../../docs/verification/2026-10-05-sparse-capacity.md), and
[scheduled complete box KvN histories](../../docs/verification/2026-10-05-generated-box-kvn-history.md).
Later evidence adds [polynomial-time boundary sources](docs/generated-time-boundaries.md),
[full-window classical box studies](../../docs/verification/2026-10-06-box-windows.md)
and a [resolved coarse nonlinear inverse signal](../../docs/verification/2026-10-06-resolved-nonlinear-box-higher-budget.md)
with mandatory phase certification on one and two local MPI ranks.
The [affine mesh verification](../../docs/verification/2026-10-06-affine-mesh.md)
adds closed and periodic unequal-cell construction evidence at both physical
orders, with explicit resource and host-configuration limits.
The [mixed-boundary foundation](../../docs/verification/2026-10-06-mixed-traction.md)
adds independently reviewed open-facet rank, convection, lifting, pressure and
resource checks on bounded affine meshes.
The [P2 cylinder record](../../docs/verification/2026-10-06-cylinder-p2.md)
contains the fixed represented geometry and independent exact proof of its
54-coordinate constraint kernel. The later [evolution record](../../docs/verification/2026-10-06-cylinder-p2-evolution.md)
retains that entire chart in three short trajectories; all completed, but the
fixed state/force temporal sensitivity criterion failed.
The [initial weak-generator checks](../../docs/verification/2026-10-06-configuration-weak.md)
verify a separate classical rate diagnostic and preserve a full-tensor work
rejection. Its later seven-row experiment completed three requests, retained
four rejections and exposed a constant-energy support degeneracy; it does not
establish configuration convergence. The separate DG2/two-cell diagnostic
removes that degeneracy but retains unresolved concentration and a larger
energy-rate defect under changed quadrature weights.
The generated history consumers and distributed physical constraints have their
own [execution scope](docs/distributed-history.md) and [pressure contract](docs/physical-space.md#distributed-physical-pressure-from-a-broken-force).
These results do not close physical convergence or multihost acceptance.

Reusable sparse sources also have separately reviewed
[matching fingerprint and persistence-version checks](../../docs/verification/2026-10-06-sparse-record-fingerprints.md)
and [weighted/structured identity checks](../../docs/verification/2026-10-06-portfolio-source-fingerprints.md).
These distinguish operator identity from the complete unitary construction;
their truncated fingerprints are provenance checks, not equality proofs.
The [persisted weighted inverse](../../docs/verification/2026-10-06-persisted-weighted-transform.md)
also connects sharded publication to reusable QSVT execution on 1/2/4/8 ranks
and split communicators, with full-coordinate residuals and measured routing.
It is a sparse-operator demonstration, separate from the physical CFD histories.

## Workflows

```sh
cargo run --release -p quest-cfd -- reference --case smoke
cargo run --release -p quest-cfd -- build --case smoke
cargo run --release -p quest-cfd -- solve --case smoke
cargo run --release -p quest-cfd --features quantum -- solve --case smoke --backend quest-cpu
cargo run --release -p quest-cfd -- estimate --case tgv3d --mesh 100
cargo run --release -p quest-cfd -- estimate --case shedding3d --reynolds 300
cargo run --release -p quest-cfd -- reference --case burgers --lift carleman --carleman-order 3
cargo run --release -p quest-cfd -- reference --case kdv --lift carleman --carleman-order 3
cargo run --release -p quest-cfd -- reference --case tgv2d --physical-order 2
```

Run `--help` on a subcommand for independent physical/configuration/temporal
resolution, regularization and resource limits. CLI output is JSON. Rejected or
failed requests return a nonzero exit status and cannot be mistaken for a
solution. `solve` defaults to the bounded scalar simulator of the complete matching QSVT
circuit. The `quantum` feature enables `--backend quest-cpu`. Native execution
uses the same matching unitary and retained certified schedule. Both CLI paths
are bounded local validation; the reusable sharded MPI API is separate.

`max_bytes` models managed numerical storage and concurrent stage workspace;
it is not a process RSS limit. Native libraries, allocator overhead and opaque
third-party scratch are outside that model. Capacity reports measure RSS
separately, and the symbolic estimator has a separate OS address-space-cap test.

* `reference` executes classical RK4 on the identical full constrained physical
  DG ODE. It reports pressure/constraint residuals and physical observables.
* `build` assembles the selected full-KvN or explicitly order-truncated Carleman
  generator and entire causal temporal system. Its result is construction evidence.
* `solve` synthesizes reciprocal QSP phases, independently certifies the actual
  projector payload by default, simulates the whole unitary, and checks the
  decoded physical residual. `--no-certify` explicitly removes independent QSP
  certification. It does not change the residual check.
* `estimate` computes full tensor and state dimensions with arbitrary-width
  integers. The ancilla allowance is explicit and must be replaced with the
  encoding-specific count for a capacity decision. Neither an estimate nor a
  rejected allocation is a quantum solution.

The default smoke case is a periodic square split into two triangles. Its 12
local BDM1 coefficients have constraint rank 7 and exactly 5 independent
coordinates. The default configuration DG1 grid has two cells per coordinate
(1,024 coefficients), and one DG1 time slab gives a 2,048-dimensional history.
The nonlinear generator is nonzero. A single periodic DG1 configuration cell
has a zero derivative and is unsuitable as nontrivial transport validation.

## Physical cases and current execution coverage

Frozen contracts are in [cases](cases). The reference backend uses bounded dense
constraint elimination and sparse lift/history assembly. Refining the physical
mesh beyond its admitted dense chart budget is rejected before construction.

| Family | Configurations | Current classical path |
| --- | --- | --- |
| 2D Taylor–Green | Re100 | Full triangular BDM1/BDM2, analytic velocity/pressure errors |
| 3D Taylor–Green | Re100, Re1600 | Full tetrahedral BDM1/BDM2, energy/enstrophy |
| 2D cavity | Re100, Re1000 | Full triangular BDM1/BDM2, weak tangential lid, profiles |
| 3D cavity | Re100, Re1000 | Full tetrahedral BDM1/BDM2, stationary side/end walls, probes |
| DFG 2D-2 cylinder | Re100 | Legacy polygonal BDM1 trajectory; complete P2 snapshot and bounded short evolution with retained temporal accuracy failure |
| 3D stationary wake | Re300, periodic span 4D | Topological full-resource estimate; execution rejected until the published Neumann far-field and convective outlet are supported |
| Viscous Burgers | ν=0.1, T=0.1 | Complete scalar DG, Cole–Hopf reference and Carleman order refinement |
| KdV | Periodic [0,2π], T=0.1 | Fu–Shu doubled DG field; all auxiliary coordinates retained |

A coarse snapshot is not a converged published benchmark. Cylinder geometry
error is reported independently. Bounded physical, lift-order, temporal and KvN
configuration studies are recorded in the scoped evidence above; published-window
statistics and independently converged profiles/forces remain outstanding.
The separate P2 evolution API has no general CLI case dispatch or accepted
developed-cycle result. Curved or
mixed-order geometry, distributed arbitrary mixed geometry and the literal 3D
wake's far-field/outlet pressure closure remain open. See [foundation.md](foundation.md).

## Numerical evidence and its limits

The configuration generator is assembled as
`W^(1/2) [-1/2 sum(F_j D_j + D_j F_j)] W^(-1/2)` with a positive tensor quadrature
mass and central DG fluxes. Its algebraic skew-adjointness is tested separately
from manufactured-wave consistency/refinement. Temporal DG1/DG2 uses causal
traces over the complete requested horizon. The history operator is used
without normal equations. An outward DG1 slab-coercivity bound supplies a
conservative physical singular-value interval. A separate interval temporal-inverse
argument admits sufficiently bounded DG2 histories. Failed sufficient bounds reject
certified solves; successful construction alone supplies no spectral proof.

Initial states are smooth compact regularizations, represented in mass-weighted
coordinates. Reports retain probability, coordinate means/variances, kinetic
energy and mass in the outermost configuration cells. On the default two-cell
grid **every cell is a boundary cell**. Consequently its boundary-mass diagnostic
cannot establish negligible truncation error. Width, domain, configuration,
physical mesh and temporal refinement are independent requirements before a
physical KvN convergence claim.

The matching encoding has explicit `alpha = K beta`, with padded matching colors,
complex coefficient rotations and reversible permutation completion. Failure
flags, dummy colors, rectangular padding, signed controls and adjoints are part
of the unitary. The portable gate circuit and fused references are compared on
whole-register states. QSVT synthesis uses the existing NLFT/certification code;
physical scaling and measured residuals are retained. The scalar reciprocal
bound excludes unproved oracle/execution error, which must be accounted for
separately in a rigorous end-to-end error budget.

## References

* [Jemcov–Morris, unitary KvN construction](https://arxiv.org/html/2605.19187v1).
  Full BDM DG and a global history inverse are research extensions here.
* [Sünderhauf–Campbell–Camps, structured encodings](https://arxiv.org/abs/2302.10949v2).
  [Supported arithmetic base/PREP constructions](../quest-qsvt/docs/portfolio.md)
  validate their map identities; preamplification remains a comparison experiment.
* [Laneve, supplied GQSP/NLFT paper](https://arxiv.org/abs/2503.03026v2).
* [2D cavity reference](https://doi.org/10.1016/0021-9991(82)90058-4),
  [3D cavity reference](https://doi.org/10.1016/j.jcp.2004.12.024),
  [official DFG 2D-2](https://wwwold.mathematik.tu-dortmund.de/~featflow/en/benchmarks/cfdbenchmarking/flow/dfg_benchmark2_re100.html),
  [3D stationary wake reference](https://doi.org/10.1017/jfm.2024.1079).

Schrödingerisation and direct Hamiltonian evolution remain comparison routes,
not implemented substitutes for this history solve. No local simulation closes
multi-host acceptance for input larger than each node's memory cap.

The [Carleman history guide](docs/carleman-history.md) derives the normalized
symmetric hierarchy, scaling and reconstruction diagnostics, with runnable
Burgers workflows and explicit convergence limits.

The [complete polynomial-time boundary foundation](../../docs/verification/2026-10-05-polynomial-boundary-lifting.md)
retains full BDM1/P0 or BDM2/P1 lifting, trace and body-force data, its lifting
derivative and original-coordinate pressure certificates. It is a bounded
reference; distributed mixed/time-dependent boundary production remains separate.
