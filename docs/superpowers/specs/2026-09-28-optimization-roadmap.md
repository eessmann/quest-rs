# Optimization research roadmap

This pass changes representation, admission and evidence. The algorithms below
are future work, not implemented features. The
[compiler collection](https://github.com/Yucheol-Choi/awesome-quantum-compiler)
was used to discover primary work, including GUOQ, QSSA, Paulihedral and relational
analysis. The proposals and acceptance criteria below are project-specific
inferences from those sources.

## Boundaries

The ideal circuit DAG describes finite ordered operations with exact rational-pi,
symbolic or explicitly floating angles. Structured programs add classical SSA,
control flow, dynamic interfaces, measurement and reset. Optimization must stay
within the evidence available in its representation. A numerically admitted
oracle is not an exact-unitarity certificate. Local approximation certificates
remain useful when a compositional whole-program bound is unavailable.

All candidates must preserve ordered targets, signed controls, barriers,
classical dependencies and stochastic effects. Compare complete complex
operators, including scalar phase. In particular, controlling a global phase
makes it observable as a relative phase; the
[OpenQASM 3.1 gate semantics](https://openqasm.com/versions/3.1/language/gates.html)
make phase-insensitive equivalence insufficient for general replacement.

## Candidate matrix

| Candidate | Representation and correctness evidence | Resource limits | Cost objective and benchmark |
| --- | --- | --- | --- |
| Commutation-aware scheduling and bounded fusion | Ideal DAG or proved straight-line structured window; registry-derived commutation rules, preserved explicit edges/effects, full-operator check of small instances; numerical fusion reports tolerance and multiplication order | Window width, queued operations, matrix bytes, proof/check work and deterministic tie breaks | QuEST dispatch count, state-vector memory passes, MPI exchanges and actual CPU/GPU time; compare existing exact and fusion passes on QFT, Pauli evolution and QSVT oracle windows |
| GUOQ-style rewriting plus local resynthesis | Immutable candidate snapshots and immediate-input provenance; use existing exact checker and local approximation certificates, with phase and interface validation before publication | Search work, candidate count, window width, retained histories, subprocess deadline/output bytes | Configurable native simulator cost versus Clifford+T count; ablate rewriting alone, synthesis alone and composition on the same circuits and error target |
| QSSA-informed quantum value flow | Add explicit quantum value-flow facts to verified SSA, retaining dynamic alias checks and linear/no-cloning constraints at joins/calls; prove effect and region-interface preservation | CFG edges, analysis facts, iterations, recursion depth; analyses tied to immutable snapshots | Redundant operations removed and compile cost on branch-heavy OpenQASM, loops and repeated oracle calls; negative fixtures for measurement, escaped aliases and non-dominating facts |
| Booth-style bounded meet-in-the-middle synthesis | Small unitary windows, phase-aware candidate keys, exact algebra or independently checked numerical certificates; duplicate search states can share payload but retain occurrence/evidence identity | Table bytes, sequence depth, coefficient bits, probes and deadline; deterministic ordering and bounded cache | Search time and certified error at fixed gate/T/depth budget; rotations and one/two-qubit windows compared with existing gridsynth worker |
| Further parity and ZX opportunities | Existing exact affine parity signatures retain scalar phase; current ZX worker candidates already cross an independent checker. Explore alternative parity scheduling or phase-teleportation candidates behind those boundaries | Parity width, coefficient bits, candidate size, graph vertices/edges, rewrite work and extraction budget | T count/depth, two-qubit cost and native runtime measured separately; Clifford+T arithmetic and phase-polynomial families with negative/signed-control cases |

## What the sources justify

[GUOQ](https://arxiv.org/abs/2411.04104) combines fast rewriting and slower unitary
resynthesis. This motivates orchestration of existing checked transformations,
not trust in an external optimizer's equivalence claim. A candidate should pay
for itself under the selected cost model, which for QuEST need not be T count.

[QSSA](https://arxiv.org/abs/2109.02409) represents quantum operations with explicit
SSA inputs and outputs and checks no-cloning. The current language's classical
SSA and effectful quantum references are not already that representation.
A quantum value-flow layer therefore needs an explicit design and verifier,
especially for aliases and dynamic control flow. The related
[relational analysis work](https://arxiv.org/abs/2410.23493) is a candidate source
for later stronger facts; its applicability must be established per operation.

[Booth](https://arxiv.org/abs/1206.3348) studies single-qubit gate-sequence search,
including bidirectional search, duplicate-sequence lookup and compact matrix
coordinates. These are search-space improvements, not a general DAG rewriting
algorithm. The paper uses a phase-insensitive distance and explicitly notes
exponential storage and finite table limits. Our controlled replacements would
need to restore and verify the phase discarded by such a search key. Claimed
search improvements in that paper are not predicted speedups for this project.

[Paulihedral](https://arxiv.org/abs/2109.03371) motivates treating algebraically
structured simulation kernels as blocks. Our first experiment should measure
whether exposing commutation enables existing bounded fusion to reduce memory
passes without expensive matrix growth. This is distinct from hardware routing.

[Matroid-based T-depth optimization](https://arxiv.org/abs/1303.2042) and
[ZX phase teleportation](https://arxiv.org/abs/1903.10477) offer directions beyond
the existing exact parity folding and checked ZX windows. They target different
costs; reducing T count can increase simulator work. The
[graph-theoretic ZX approach](https://arxiv.org/abs/1902.03178) also requires
bounded extraction, since a smaller graph alone is not an executable circuit.

## Ranking protocol

The [consolidated baseline](../../verification/2026-09-28-consolidation-review.md)
now records linear retained merge history, shared expression construction and
successful bounded branch-heavy SSA admission. It removes those representation
obstacles to experimenting with longer windows and structured analysis. It does
not measure the speedup of any proposed algorithm or establish that a particular
native workload is dispatch-bound.

The resulting engineering priority is:

1. **Commutation and bounded fusion:** first measure the existing simulator cost
   objectives on representative windows. Existing DAG, gate and matrix evidence
   provide the smallest extension boundary; stop if dispatch/memory-pass savings
   do not repay construction cost.
2. **Combined rewriting and local resynthesis:** build on the bounded provenance
   and checked worker infrastructure. Measure candidate rejection and search cost
   as well as accepted gate reductions.
3. **Quantum value flow:** the larger SSA baseline is now admissible, making it
   practical to study the required alias/no-cloning facts. This requires more new
   verifier work than the first two experiments.
4. **Further parity and ZX:** retain existing independent checks and first find
   workloads where current passes leave measurable cost. New candidate rules
   should justify their extraction and proof costs.
5. **Bounded meet-in-the-middle:** restrict to small windows and a hard table
   budget. Its exponential storage and extra phase-recovery requirements make it
   a specialized later experiment despite the improved surrounding history size.

This is a ranking of implementation experiments by evidence readiness and bounded
cost, not a forecast of numerical speedups. Re-rank after native workload profiling
identifies the dominant execution/compilation cost. Keep source circuits,
seeds, native configuration, exact phase convention and error target fixed.
Report unsuccessful candidates and budget exhaustion alongside accepted changes.
A timing improvement without equivalent semantics and complete evidence is not
an optimization result.
