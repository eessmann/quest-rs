# Sparse operators, integrated MathCore and dual CFD history solvers

Status: approved implementation programme, 2026-10-05. This document records
requirements, not completed implementation. The public
[implementation evidence](../../verification/2026-10-05-workspace-programme.md)
records scoped progress and verification. It extends `crates/quest-cfd/NEXT_STEPS.md`; the existing
full-DG benchmark and verification obligations continue to apply.

## Global constraints

- Retain every independent physical coordinate, including conserved mean flows
  and the evolving KdV auxiliary field. Carleman order truncation has its own
  approximation error; representation size is not an accuracy certificate.
- Keep native QuEST unchanged. Checked adapters in quest-sys are in scope.
  The new scalable path targets CPU/MPI statevectors and explicitly rejects
  unsupported deployments without changing existing supported behavior.
- No production rank requires complete CSR/CSC, dense dilation, a global state,
  a global gate stream or a global permutation table.
- Preserve whole-unitary behavior, complex phases, failure branches, padding,
  spectators, signed controls and adjoints. Operator identity and unitary
  construction identity are distinct.
- Keep numerical storage in quest-numerics, serialization in quest-qsvt-io,
  coherent recipes in quest-qsvt and MPI orchestration above those pure layers.
- Apply QSVT directly to the non-Hermitian history with correct orientation and
  physical scaling. Do not form normal equations.
- Preserve exact, typed and domain semantics when consolidating MathCore.
  Independently verify certificates and lower construction algebra into kernels.
- Record actual tests, rejected budgets and unavailable environments honestly.
  Local capacity tests do not close actual multi-host acceptance. Documentation
  and receipts use generic paths. Do not modify unrelated user work.

## Task 1: General owning encodings and transform schedules

Introduce reusable encoding descriptors carrying logical dimensions,
normalization, layout, compact left/right projectors, clean workspace,
preparation/encoding error, source identity and unitary construction identity.
Generalize the existing owning matching QSVT schedule without duplicating the
phase iteration algorithm. Preserve MatchingSchedule and OracleFragment
compatibility through adapters. Supply owning replayable implementations for
existing matching and arithmetic structured sources, with controls, remapping,
adjoints, fallible storage accounting and bounded forward/reverse replay.

Acceptance: a non-matching encoding drives the shared schedule; mismatch of
source/layout/construction is rejected; compact projectors stay compact;
portable whole-register reference tests cover controls and adjoints. Existing
matching/QSVT tests remain passing. Avoid adding a descriptor unused by callers.

## Task 2: Native admission and local-partition execution

Expose native capability/admission evidence for dense CompMatr, DiagMatr and
FullStateDiagMatr, statevector/density deployments, native signed indices and
MPI counts, including concurrent peak local storage. CompMatr is replicated;
for distributed statevectors k targets require 2^k <= 2^n/P. Full-state diagonal
density execution may gather the complete diagonal. Native QuEST source stays
unchanged. Use native controls without matrix expansion only where unitary
semantics are established; preserve general linear operators.

Replace prepared matching execution's replicated global-index traversal with
local-state-owner traversal and bounded source/destination routing. Preserve
every flag branch and spectator, controls and adjoints. Reuse immutable
schedules, bounded send/receive buffers and separate transport communicator;
admit per-rank/per-node scratch and checked large-count chunking. Preserve
collective preflight and distinguish recoverable rejection from bounded fatal
native/post-mutation failure. Do not promise unsupported MPI fault recovery.

Acceptance: whole-state portable/fused comparisons; 1/2/4/8 ranks, split
communicators, cross-rank routes, inactive controls, source mismatch, count and
allocation failures, timeout-bounded fatal failures. Measure setup separately
from repeated execution. Source-visible opportunity is not a speedup result.

## Task 3: Distributed sparse producer and permutation completion

Accept genuinely sharded streamed COO and local CSR. Preserve stable global
input ordinals for ordered duplicate reduction, then remove canonical zeros.
Use bounded exchanges and admitted external merge storage. No full source may
be assembled first. Freeze round snapshots for deterministic distributed edge
coloring: each edge proposes the least color unused at both endpoints; endpoint
owners choose the minimum (row,column) edge per proposed color; commit only
after both owners agree. Admit rounds, probes, retained endpoint records and
communication. At least the smallest uncolored edge progresses each round;
Delta_row + Delta_column - 1 colors suffice before label padding.

Complete partial matchings using distributed predecessor/successor/path-end
records, retaining bounded forward/reverse directories rather than a global
permutation table. Keep baseline alpha=K beta, dummy labels and the established
zero-source convention. Expose immutable records for persistence and replay.

Acceptance: canonical duplicates and coloring independent of rank/chunk
partition, adversarial conflicts, high-degree capacity rejection, zero and
rectangular sources, permutation bijectivity and whole-unitary equality to the
same-construction portable reference. Include a producer run starting with
shards, with no full-source constructor anywhere in its execution path.

## Task 4: Immutable shard persistence and portable replay

Add chunked serial-HDF5 input/resource files and a versioned Serde JSON
manifest. Fixed logical buckets are independent of producer rank count; one
writer owns each file. Execution counts dividing the bucket count reuse files;
other layouts require an explicit bounded repartition. Preserve exact binary64
words and certified replay angles. Bind schema/construction versions,
dimensions, layout, normalization, ownership, counts, file sizes/hashes and a
canonical semantic digest. SHA-256 byte integrity is distinct from mathematical
evidence and existing runtime FNV checks. Publish manifest last after success.

Add bounded forward/reverse portable gate replay directly from immutable
resources. Do not reconstruct original certified angles through fresh inverse
trigonometric evaluation. Test whole-unitary equivalence, missing/duplicate/
altered shards, truncated chunks, collective failure, reuse and source lifetime.

## Task 5: Workspace MathCore consolidation

Restore the maintained fork with pinned provenance and license as a normal
dependency. MathCore owns exact constants, scoped symbols, shared typed/static
and dynamic expressions, bounded simplification, substitution, differentiation,
sparse multivariate polynomials and exact affine algebra. Move neutral backend
interfaces down to avoid cycles; numerical implementations remain in
quest-numerics. Migrate quest-symbolic, quest-polynomial, language/compiler
scalar evaluation, QSP/QSVT function construction and CFD consumers. Keep
independent certificate replay/checks outside the engine they verify.

Distinguish exact rationals/decimals, symbolic pi and floating-point values.
Preserve signed zero, original binding/domain/conversion obligations and
ordered typed evaluation. No epsilon zero proofs, unsafe cancellation or
floating reassociation. Bound nodes, depth, coefficient growth, expansion,
storage and work; caches retain logical admission costs. Fix eager compile-time
Boolean evaluation. Lower algebra once, not inside numerical inner loops.
Remove duplicate implementations and retain compatibility re-exports where
contracts hold. Acceptance exercises real cross-crate consumers and independent
checks; adding an unused optional dependency is insufficient.

## Task 6: Encoding portfolio

Implement weighted PREP-SELECT-UNPREP with alpha=sum |w_t| alpha_t, complex
coefficient phases, tensor products and Kronecker sums. Add per-color bounds
for stored matchings. Account rounding slack and preparation errors explicitly.
Implement Sunderhauf-Campbell-Camps base arithmetic maps then PREP/UNPREP only
with required identities. Start circulants, bounded-band Toeplitz and tensor
stencils. Implement reversible sparse row/column location and value oracles
with an explicit bounded QROM baseline and lookup/unlookup/precision costs.
Compare normalization, preparation, queries, workspace and execution, not
degree alone. Keep preamplification/hierarchical compression follow-on work.

Sources: https://arxiv.org/html/2302.10949v2 and
https://arxiv.org/abs/1806.01838. Test extracted blocks and whole unitaries,
including all failure sectors, controls and adjoints for each construction.

## Task 7: Polynomial dynamics, Burgers and Carleman hierarchy

Introduce full PolynomialOde F0(t)+F1(t)a+F2(t)(a tensor a). Extract through
MathCore and verify by independent evaluation/polarization. Include lifting
derivatives, reject unsupported nonpolynomial dynamics. Implement ordered
tensor reference and production normalized symmetric monomials
sqrt(|alpha|!/alpha!) (a/s)^alpha for degrees 1..r. Dimension is
binomial(m+r,r)-1. Keep degree zero as external forcing and generate entries
through recipes/shards. Verify symmetric/ordered isometric equivalence.

Freeze Burgers: u_t+(u^2/2)_x=nu u_xx on [0,1], zero Dirichlet, nu=0.1,
T=0.1, four DG1 cells, polynomial entropy-conservative flux, SIP viscosity,
Cole-Hopf reference amplitude parameter 0.01, two temporal DG1 slabs initially.
Refine space/order/time independently and retain every DG coordinate.

Calculate real coefficient/forcing/log-norm evidence and RC. Corrected Liu
forcing hypotheses and nonnormality matter. Check complete conservation-aware
or Lyapunov hypotheses before certification; retain periodic means. Otherwise
label experimental refinement or validated reconstruction-defect evidence.
Default experimental scale s=2||a0|| when nonzero; use a positive forcing scale
for forced zero initial state and RC undefined. Identically zero problems return
exact zero, without claiming a normalized quantum solve.

Sources: https://arxiv.org/html/2011.03185v4 and
https://arxiv.org/abs/2509.07155v2. No register count proves truncation accuracy.

## Task 8: Shared history solve, coherent preparation and observations

Add LiftKind Kvn/Carleman and explicit order/scaling options across reference,
build, solve and estimate. Generalize causal DG history for time-dependent
generators and forcing at quadrature nodes. Establish independent DG1/DG2
singular-bound evidence for nonnormal histories without normal equations.

Implement coherent full-history RHS preparation with sharded amplitude-tree
norms/angles and bounded forward/adjoint gate replay. Count construction,
multiplexor decomposition, storage, communication and elementary gates. Keep
local simulator initialization a separate convenience. Zero unresolved sampled
forcing is not a proof of zero physical forcing. Retain global RHS norm,
physical scale and basis metadata. Source: https://arxiv.org/abs/quant-ph/0407010.

Recover degree-one coordinates with inverse and degree/time success reported
separately. Include physical norms, temporal interpolation/weights, preparation
error amplification and observable recovery/sampling cost. Full readback stays
bounded. Compare both lifts with identical full-DG trajectories and independently
refine physical/time/lift/encoding/polynomial/sampling errors.

## Task 9: KdV and complete physical DG foundation

Freeze KdV: u_t+6u u_x+u_xxx=0, periodic [0,2pi], u0=0.05 cos x, T=0.1,
four DG2 cells, two temporal DG1 slabs. Use Fu-Shu energy-conserving ultraweak
DG with evolving auxiliary field initially zero; retain all auxiliary DOFs.
Validate linear Airy before nonlinear refinement. Source:
https://arxiv.org/html/1805.04471v1.

Complete original BDM2/P1, time-dependent lifting and declared Re300 3D wake
boundary requirements. Use cell-local mass whitening and distributed pivoted
Householder QR of complete constraints, storing reflectors without dense global
basis. A bounded classical implementation using existing MPI is sufficient;
count global projection/factorization work and reject ambiguous numerical rank.
Verify every original constraint, gauge, momentum and energy balance.

## Task 10: Campaigns, capacity, documentation and repository acceptance

Retain all six existing benchmark families, manifest geometry/Re/boundary/
observation definitions, including 200 measured cycles for the 3D wake; add both
1D demonstrations. Establish classical references and separate physical,
temporal, geometry, regularization/configuration and Carleman refinements.
Supply full arbitrary-width resource curves and actual encoding/preparation/
observation costs. Only admitted executed circuits carry quantum-validation
claims; estimates and classical solves are distinct results.

Run capped-memory local jobs from genuinely sharded input first. Actual
multi-host input exceeding each node's memory cap and actual large-count jobs
remain open until that infrastructure exists. Record rank/node peak memory,
max-rank timings, send/receive volume and all setup/repeated-execution costs.
Update method, alternatives, distributed and NEXT_STEPS docs with derivations,
provenance, APIs, reproducible commands and honest status.

Run affected tests, workspace/all-feature tests and doctests, formatting,
Clippy, binding freshness and matching native MPI checks. Record environment
restrictions and explicit lint exceptions. Independently review each stage.
