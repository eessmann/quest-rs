# quest-cfd

Research example for full incompressible BDM1/P0 DG, a nonlinear Koopman–von
Neumann (KvN) lift, and a global causal DG history inverse using QSVT. Every
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

The separate [TODO and next-step implementation plan](NEXT_STEPS.md) lists
remaining work, dependencies and acceptance gates. [Physical foundation](foundation.md)
is a shorter implementation/status reference. The chapters distinguish algebraic
identities, dated execution evidence and convergence claims still requiring work.

## Workflows

```sh
cargo run --release -p quest-cfd -- reference --case smoke
cargo run --release -p quest-cfd -- build --case smoke
cargo run --release -p quest-cfd -- solve --case smoke
cargo run --release -p quest-cfd --features quantum -- solve --case smoke --backend quest-cpu
cargo run --release -p quest-cfd -- estimate --case tgv3d --mesh 100
cargo run --release -p quest-cfd -- estimate --case shedding3d --reynolds 300
```

Run `--help` on a subcommand for independent physical/configuration/temporal
resolution, regularization and resource limits. Output is JSON. Rejected or
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
* `build` assembles the full mass-scaled configuration generator and entire
  causal temporal system. Its result is construction evidence.
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
| 2D Taylor–Green | Re100 | Full triangular BDM1, analytic velocity/pressure errors |
| 3D Taylor–Green | Re100, Re1600 | Full tetrahedral BDM1, energy/enstrophy |
| 2D cavity | Re100, Re1000 | Full triangular BDM1, weak tangential lid, profiles |
| 3D cavity | Re100, Re1000 | Full tetrahedral BDM1, stationary side/end walls, probes |
| DFG 2D-2 cylinder | Re100 | Polygonal cylinder, boundary lifting, forces/residuals |
| 3D stationary wake | Re300, periodic span 4D | Topological full-resource estimate; execution rejected until the published Neumann far-field and convective outlet are supported |

A coarse snapshot is not a converged published benchmark. Cylinder geometry
error is reported independently. Published-window statistics, all requested
refinement campaigns, higher physical BDM orders and the approved 3D wake
boundary operator remain outstanding. See [foundation.md](foundation.md).

## Numerical evidence and its limits

The configuration generator is assembled as
`W^(1/2) [-1/2 sum(F_j D_j + D_j F_j)] W^(-1/2)` with a positive tensor quadrature
mass and central DG fluxes. Its algebraic skew-adjointness is tested separately
from manufactured-wave consistency/refinement. Temporal DG1/DG2 uses causal
traces over the complete requested horizon. The history operator is used
without normal equations. An outward DG1 slab-coercivity bound supplies a
conservative physical singular-value interval; temporal DG2 currently requires
external spectral evidence for a solve. The current CFD workflow does not accept
such external evidence and therefore rejects temporal DG2 solves; DG2 assembly
remains available.

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
  Arithmetic tensor shifts and generic weighted matchings are implemented;
  the paper's complete base/PREP/UNPREP construction is not claimed complete.
* [Laneve, supplied GQSP/NLFT paper](https://arxiv.org/abs/2503.03026v2).
* [2D cavity reference](https://doi.org/10.1016/0021-9991(82)90058-4),
  [3D cavity reference](https://doi.org/10.1016/j.jcp.2004.12.024),
  [official DFG 2D-2](https://wwwold.mathematik.tu-dortmund.de/~featflow/en/benchmarks/cfdbenchmarking/flow/dfg_benchmark2_re100.html),
  [3D stationary wake reference](https://doi.org/10.1017/jfm.2024.1079).

Schrödingerisation and direct Hamiltonian evolution remain comparison routes,
not implemented substitutes for this history solve. No local simulation closes
multi-host acceptance for input larger than each node's memory cap.
