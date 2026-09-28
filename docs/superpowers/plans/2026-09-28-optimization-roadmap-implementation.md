# Optimization roadmap with an audited mathcore exact subsystem

Status: approved for implementation in the conversation on 2026-09-28.

Spec: `docs/superpowers/specs/2026-09-28-optimization-roadmap.md`, supplemented by the complete approved implementation contract below. The older spec's description of future work is superseded by this explicit implementation authorization.

## Global constraints

- Preserve exact mathematical correctness, deterministic behavior, complete scalar phase, ordered targets, signed controls, classical/stochastic dependencies, traps and binding domains.
- Use immutable shared payloads, typed owner/snapshot identifiers, consuming validation stages, checked resource arithmetic, fallible admission and non-panicking RAII.
- Keep ideal circuits separate from executable SSA and its binary64/modular-angle evaluation semantics.
- mathcore is the required exact symbolic engine through a maintained MIT fork; Symbolica is excluded.
- Independent affine, cyclotomic and interval verification stays outside the fork. Candidate-generation output is never proof by itself.
- Native execution cost is primary; reuse is caller-declared and defaults to one. Approximation requires an explicit budget. Local and required-global certificate modes are distinct.
- Global bounds are mathematical operator-approximation bounds, excluding native floating execution error. Uncertified fusion cannot inherit them. Unsupported oracle/control-flow composition rejects a required-global request.
- Every task starts with focused failing regressions, ends with covering checks and independent spec/code review. Preserve evidence and make signed, focused commits; no remote push.

## Task 1: Vendor mathcore and add exact affine algebra

Import checksum-identified upstream mathcore 0.3.1 under `crates/vendor/mathcore`, preserve its MIT license and baseline provenance. Package it as `quest-mathcore` version `0.3.1-quest.1`, dependency alias `mathcore`. Isolate old approximate APIs behind `legacy`; add `exact`, with dependencies for legacy code optional. Keep the vendor outside workspace inheritance, following rsgridsynth precedent.

The exact module owns immutable canonical `r + q*pi + sum(c_i*s_i)` expressions. Use normalized arbitrary-precision rationals, distinguished symbolic pi, typed symbols with ownership namespaces, private shared flat storage and sorted unique terms. Provide checked constructors, negation, add/subtract, rational scaling/division, simultaneous substitution, inspection and deterministic export. Exact equality/zero/one use no tolerances. No implicit floating conversion, variable multiplication/division, powers, transcendental evaluation or parser in this domain.

Bound terms (4096), coefficient bits (16384), storage (64 MiB), work and all intermediate growth before arithmetic/publication. Tests: thirds, tiny nonzero values, >2^53 integers, negative/zero denominators, symbolic pi, canonical ordering, simultaneous substitution, foreign symbols, budget boundaries, exact-only feature dependency isolation. Publish API contract for Task 3.

## Task 2: Extend independent affine-pi certificates

Add an admitted target for exact `r+s*pi` after dyadic parameter substitution; retain old DyadicRadians and RationalPi identities. Independently enclose the half-angle with directed Grid arithmetic and full 4*pi rotation phase. Bound both rational inputs and intermediate argument reduction. Preserve original target in certificates; no float approximation may replace target identity. Extend protocol admission and controlled certification exhaustively. Tests: reduction boundaries, huge inputs, rationals not dyadic, old conversion vectors, controlled scalar phase, precision/budget rejection.

## Task 3: Integrate quest-symbolic and ideal angles

Add required workspace crate quest-symbolic depending only on mathcore's exact feature for algebra. Wrap immutable expressions with quest-owned parameter/source/binding obligations. Keep public Angle; migrate exact Pi/Parameter/Negative representations and centralize checked addition/scaling/substitution/classification for definitions, parity and worker adapters. Keep opaque binary64 and QASM modular/numerical expressions distinct and unchanged.

Independent affine checks must reconstruct equality without calling the fork normalizer. Retain all original parameter ownership and complete finite binding requirements after cancellation. Potential domain-changing merges stay deferred until checked finite binding succeeds, otherwise retain original. Bind new affine expressions to exact r+s*pi target plus checked execution value; preserve existing leaf signed-zero and rational-pi conversion semantics. Tests: foreign p-p, cancelled bindings, overflow fallback, original conversion failures, exact target identity, incorrect algebra rejected, QASM operation order/traps unchanged.

## Task 4: Shared optimizer contracts, dependencies and native cost

Add thin consuming Optimizer APIs for ideal+bindings, bound and verified structured input; preserve individual passes. Constructor-validated immutable options/targets/limits/outcomes expose provenance, evidence, rounding, budget/stop reasons. Add ideal snapshot IDs. Preserve dependency kinds through bind and project mandatory ordering paths through all deletion/contraction/substitution or reject candidates.

Share pure dispatch/preparation recipes with the facade, covering Id, U/Sx decompositions, signed-control flips, control matrix embedding, forward/adjoint storage and sharing. Expose actual register deployment snapshot. Default versioned exact rational score B/1024 + R*[D+k*(64P+A)+X+64C/L], k=1 SV/2 DM, single-sided P/A and complete D/X/C. R defaults one. Unknown communication is not zero: changing its ordered opaque component makes candidate unscorable. Clifford+T profile lexicographically orders T count, two-qubit count, dependency depth, total operations. Profiles predict preferences, not timings.

One shared ledger reserves candidates/verification/frontiers/provenance/worker allowances: 10 million logical work and 256 MiB. Typed approximation Disabled/Local/Global; global rejects unsupported composition and skips uncertified fusion. Best admitted result on deterministic search exhaustion; malformed inputs and output admission errors stay errors; timeout discards unfinished request.

## Task 5: Commutation scheduling and bounded fusion

Build 64-operation windows with all originally ordered noncommuting/unknown pairs constrained plus mandatory edges. Prove disjoint full support, computational diagonality and reviewed same-axis/control-projector cases. Matrices/calls/effects/traps are fences. Stable ready selection favors compatible interfaces, source order ties. Preserve ordered embedding and later*earlier multiplication. Charge matrix work before operation. Fusion defaults four qubits, 32 operations, 1 MiB matrices, 64 MiB provenance. Compare identical terminal treatment and reuse for baseline/candidate; publish only strict cost improvement.

Tests: nontransitive A-B-C commutation, signed/cross controls, anticommutation/full phase, edge contraction, numerical width/work limits and accurate cost/preparation accounting.

## Task 6: Deterministic combined rewriting/resynthesis beam

GUOQ-inspired bounded deterministic beam over exact symbolic/linear/parity/ZX and enabled synthesis generators. Separate generation from local shorter-circuit acceptance. Independently verify before frontier admission; allow temporarily worse candidates while retaining best complete result. Algebraic frontier with terminal fusion only. Candidate identity includes relevant evidence state; fresh occurrence provenance/certificates per use. Accumulate approximation against original input, never reset epsilon per round. Defaults width4, rounds8, candidates128, workers16 under shared limits. Test temporary regression paths, cycles/ties, evidence-sensitive dedup, caps/timeouts and transactional fallback.

## Task 7: QuantumFlow and structured optimization

Snapshot-bound derived value flow on verified executable SSA: separate physical storage and value versions; controls consume/produce versions; multiwire nodes coupled. Edge-tagged branches, finite headers/backedges; Same/Disjoint/MayAlias facts, dynamic root clobbers. Tighten quantum reference exclusivity verification before using it. Sparse bounded facts/edges/alias comparisons/CFG work. Optimize scalar parameter bodies and safe one-successor/one-predecessor chains within a region. Calls/effects/traps/joins/backedges remain fences and runtime checks remain. No quantum CSE, dead-result elimination or loop-carried cancellation. Test forged readonly alias params, valid normalized source, arrays, branches/loops, modifiers and foreign analysis handles.

## Task 8: Parity and ZX candidate extensions

Shared-pivot parity scheduling reuses symmetric differences, restores each pivot, preserves affine output and scalar phase, checks exact signatures. Use exact symbolic coefficients where admitted. ZX adds deterministic bounded local complementation/pivot/gadget fusion; retain existing simplifier baseline and independently check extracted full-phase matrices/permutations. Limits4qubits/128input/4096vertices/16384edges/100000examinations/4096rewrites/1000output. Bound upstream extraction by graph/output/process limits without claiming an instrumented step count. Not full phase teleportation. Tests compare signatures/full operators and all graph/extraction caps.

## Task 9: Exact and approximate MITM workers

Versioned optional capability. Exact 1/2-qubit full-phase cyclotomic keys, predecessor chains and safe cost/depth duplicate representatives. Chronological L,R gives R*L, lookup R-dagger*target. Alphabet existing H/X/Y/Z/S/Sdg/T/Tdg/Cx/Cz/Swap/W as interface permits. Depth12/6, states32768, table/index64MiB, work1million, table coefficients256bits. Preserve distinct target input admission up to existing16384bits.

Approximate one-qubit rotations use deterministic 8D interval kd-tree of exact half matrices, outward boxes, largest midpoint-spread/stable-median split, leaves8. R-dagger*target queries; prune only rigorous squared lower bound > exact epsilon^2. Midpoint ranking is not proof. Precision128 doubling to configured <=4096bit cap. Shortlist and cert attempts64. Reuse independent target enclosures for dyadic/rational-pi/affine-pi; parent independently reconstructs/certifies. No phase quotient or optimality claim. Distinct incomplete/exhausted/unresolved/no-candidate/timeout outcomes; existing30s/512MiB/64KiB process/protocol ceilings. Tests against tiny exhaustive search/radius query, boundary equality, I/-I/omegaI, subnormals/huge angles, duplicate dominance and forged results.

## Task 10: Structured terminal fusion and end-to-end orchestration

For proved-static windows in !region.gate regions only, map ordered wires to local positions, create immutable OracleFragment and fresh capture identity, replace with unmodified synthetic call, and transactionally publish/reverify SSA+oracle bank+combined storage. Preserve original syntax/capture evaluation order. Skip zero-wire windows and keep gate bodies symbolic because inverse/negative-power execution rejects numerical oracles. Integrate terminal costs/error modes/budgets across all optimizer entrypoints. Test inv/nested/controlled gates, arity/capture collision, source export, transactional rejection and global-mode fusion skip.

## Task 11: Benchmarks, complete validation and delivery

Fixed corpus: QFT, Pauli evolution, QSVT, Clifford+T, control-heavy and branch-heavy. Ablate unchanged/existing/new stages and combinations. Record symbolic construction, search, preparation, warm execution, reuse break-even, allocations/retained bytes, costs/evidence and every failed/exhausted case in completion manifest. Fixed seeds/toolchain/native modes and actual CPU/OMP/GPU selection; SV/DM; max-rank timing for MPI2/4. No speedup claims from gate count alone.

Run workspace build, Nextest, separate doctests, fmt, strict Clippy, freshness, optional-worker/certification/offline/HDF5/MPI matrix, fork tests and affected standalone consumer RUNPATH checks. Preserve compile-fail owner/lifetime/kind/stage coverage. Independent final review, migration/examples/measured report, signed focused commits. Do not push. General legacy CAS repairs and unrelated algorithms are outside scope.
