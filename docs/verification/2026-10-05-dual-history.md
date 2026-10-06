# Bounded Burgers Carleman history execution

These receipts describe an executed short-window circuit and an identical-DG
classical comparison in the uncommitted implementation worktree based on
`dd9a869ebe9952cf85a6411a180ed7b688f568fb`. They do not close the default physical
benchmark campaign or the full workspace programme.

| Quantity | Recorded result |
| --- | --- |
| Physical space | Four DG1 cells, all eight mass coordinates |
| Physical problem | Viscous Burgers, homogeneous Dirichlet, viscosity 0.1 |
| Initial data | Cole–Hopf parameter 0.01 |
| Window / time space | T=0.001, one DG1 slab |
| Lift | Complete symmetric Carleman order two, 44 coordinates |
| History dimension | 88 |
| Backend | Actual local CPU QuEST matching QSVT execution |
| Circuit register / reciprocal degree | 13 qubits / 465 |
| Coherent RHS | 255 stored coefficients, 634 primitive gates |
| Relative stored-history residual | 4.3681676693e-6 |
| Inverse success probability | 0.0072095565425 |
| Inverse plus final-time/degree-one probability | 0.0028816608769 |
| Full-DG coordinate error against RK4 | 1.9317986836e-8 |

The native receipt is [burgers-c2-native.json](data/2026-10-05-cfd-history/burgers-c2-native.json).
The independently executed 100-step RK4 trajectory is
[burgers-classical-short-window.json](data/2026-10-05-cfd-history/burgers-classical-short-window.json).
[comparison.json](data/2026-10-05-cfd-history/comparison.json) records the coordinate
comparison and evidence limitations. The JSON files retain all coordinates,
physical scaling, coefficient evidence, success probabilities and stage timings.

Reproduce using a matching installed QuEST/MPICH pair:

```sh
export QUEST_ROOT=<installed-quest-prefix>
export MPICC=<matching-mpi-prefix>/bin/mpicc
cargo run -p quest-cfd --features quantum -- solve --case burgers \
  --lift carleman --carleman-order 2 --horizon 0.001 --time-cells 1 \
  --max-degree 8191 --no-certify --backend quest-cpu
cargo run -p quest-cfd -- reference --case burgers --dt 0.00001 --steps 100
```

This run explicitly skipped the optional projector-phase certificate. An
independent preparation-error certificate is also unavailable. The measured
linear residual and physical comparison are execution evidence; they do not
provide a rigorous combined bound for all errors. The RK4 comparison was not
itself accompanied by a validated trajectory enclosure.

Separate focused tests exercised the constant complex history with the optional
phase certificate on scalar and native backends. Additional tests cover
nonautonomous forcing, DG1 dissipative and DG2 nonnormal spectral evidence,
ordered/symmetric hierarchy equivalence, forced/zero initial conditions and
physical reconstruction defects. These test receipts are not a substitute for
the missing default T=0.1 physical/lift/time refinement campaign.

The native example uses one local environment. Actual multi-host capacity,
distributed RHS production, observable-sampling complexity, and quantum advantage
are not established by it. Runtime numbers describe this run only.

## Full frozen Burgers window

A later [full-window receipt](data/2026-10-05-burgers-full-window/comparison.json)
retains every one of the same eight physical coordinates and executes the frozen
T=0.1 demonstration using the complete order-two symmetric hierarchy and two DG1
time slabs. The [native output](data/2026-10-05-burgers-full-window/native.json)
records 176 history coefficients, 14 qubits, degree 7,555 and matching normalization
48.96. The measured relative history residual is `1.61778e-7`; inverse success is
`0.00190116`, and inverse/final-time/degree-one joint probability is `0.000351916`.
Coherent RHS preparation uses 511 stored coefficients and 1,274 primitive gates.

The default sparse Gershgorin certificate is inconclusive for these two slabs.
The explicit [bounded reference spectral check](../../crates/quest-cfd/docs/reference-spectrum.md)
instead proves bounds `[0.2125511366, 8.1147049347]` for the stored matrix using
an independently interval-checked inverse candidate. Its residual norm bound is
`1.20e-14`; the candidate is discarded before the circuit runs. This costs classical
dense work and is not a scalable spectral algorithm.

The first optional QSP-certificate attempt exceeded its memory budget. The final
native execution explicitly used `--no-certify`; it therefore has no projector
phase certificate or combined error theorem. A second preliminary binary lacked
the quantum feature and rejected before execution; both failed attempts are
preserved. The successful immutable binary's QuEST/MPICH linkage and byte hash
were checked. Under a real 2 GiB address-space cap, the process took 88.10 seconds
with maximum RSS 269,436 KiB. Native transform execution took 83.49 seconds;
these local debug timings establish no performance advantage.

The recovered physical coordinates differ from the
[independent direct solve of the same history](data/2026-10-05-burgers-full-window/direct-history.json)
by `2.45e-10`, and from the
[identical full-DG RK4 trajectory](data/2026-10-05-burgers-full-window/rk4.json)
by `1.35e-7` (relative `3.35e-5`). The direct-history reference itself differs
from that RK4 trajectory by `1.35e-7`, separating the measured circuit error from
the fixed history/lift approximation. These numerical references are not
validated trajectory enclosures. The coarse physical Cole–Hopf L2 error remains
`2.07e-4`; physical convergence and total observable errors remain open.

```sh
cargo run -p quest-cfd --features quantum -- solve --case burgers \
  --lift carleman --carleman-order 2 --horizon 0.1 --time-cells 2 \
  --reference-spectral-bound --max-degree 16383 --no-certify --backend quest-cpu
cargo run -p quest-cfd -- reference --case burgers --dt 0.000025 --steps 4000
```

## Independent temporal refinement

The [fixed-lift temporal receipt](data/2026-10-05-cfd-history/burgers-temporal-refinement.json)
retains all eight physical coordinates and all 44 degree-one/two symmetric
Carleman coordinates. A bounded dense direct reference solves the original
complex DG history; the independent endpoint reference integrates the identical
truncated hierarchy with 4,000 RK4 steps. This isolates temporal error from
Carleman truncation and physical approximation.

| Temporal order | Slabs | Full-hierarchy endpoint error |
| --- | --- | --- |
| DG1 | 1 / 2 / 4 | 2.07e-4 / 5.52e-5 / 1.43e-5 |
| DG2 | 1 / 2 / 4 | 8.59e-7 / 4.26e-8 / 3.55e-9 |

Sparse relative residuals are below `2.7e-15`. These are classical bounded
reference solves, not quantum executions or a certified RK4 error bound. Run
`cargo test -p quest-cfd --test classical_history -- --nocapture` to repeat.

## Physical-order and nonlinear reference campaign

The bounded [campaign fixture](fixtures/quest-cfd/campaign.py) independently
changes physical mesh, physical order, time step, and cylinder geometry. Its
interim [receipt](data/2026-10-05-cfd-history/campaign.json) records 41 physical
attempts: 40 classical executions and one explicit 3D-wake boundary rejection.
Each child ran under a 512 MiB address-space cap with an external timeout. The
largest observed physical-reference RSS was 14,272 KiB. It also records 205
full-coordinate KvN and fixed-order Carleman representation estimates. These
are small local reference experiments, not a sparse-input capacity test.

The Navier–Stokes references cover only `T=0.0002`, far shorter than the frozen
benchmark windows. Burgers and KdV cover the frozen demonstration `T=0.1`.
The separately recorded [cylinder-window experiment](2026-10-05-cylinder-window.md)
subsequently reached the full DFG observation interval. Its coarse solution did
not resolve shedding, and it does not replace mesh/geometry/time convergence.
The receipt preserves the initially rejected KdV order-2/order-3 constructions.
Those rejections exposed an unnecessarily Cartesian recipe capacity estimate.
A subsequent exact combinatorial contribution count admits both while retaining
all 24 KdV physical/auxiliary coordinates. The separate
[updated KdV order study](data/2026-10-05-cfd-history/kdv-order-refinement.json)
records the new executable hash and actual results:

| Fixed physical system, T=0.1 | Order 1 coordinate error | Order 2 | Order 3 |
| --- | ---: | ---: | ---: |
| Burgers, four DG1 cells, all 8 coordinates | 2.48474e-6 | 1.97018e-9 | 1.43838e-12 |
| KdV, four DG2 cells, all 24 coordinates | 1.30329e-3 | 2.71814e-5 | 5.41706e-7 |

Both trajectories use classical RK4 with `dt=0.0001` and compare against the
identical complete semidiscrete dynamics. These errors isolate lift-order
refinement at fixed numerical time resolution; RK4 and physical DG errors
remain separate. They do not establish continuum convergence, a theorem for
KdV, normalized-state error, quantum execution, or published CFD statistics.

BDM2/P1 box construction and CLI integration have independent analytic-mass,
quadratic-shear, topology-rank and pressure tests. Coarse periodic 2D/3D boxes
retain 10/85 independent coordinates; cavities retain 4/49. The materialized
physical reference has a 768-local-coefficient admission cap. The existing
polynomial snapshot bridge still admits at most 64 independent coordinates,
so an 85-coordinate estimate does not imply a built Carleman operator.
