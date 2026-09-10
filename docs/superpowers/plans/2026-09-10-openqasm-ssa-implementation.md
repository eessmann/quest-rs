# OpenQASM 3.1, typed SSA and hardware-independent optimization

Approved by the user on 2026-09-10. Implementation authority is the complete plan in the task conversation. This durable ledger records work against every milestone; unchecked work is not delivered.

## Global contracts

- Shared OpenQASM 3.1 simulator profile: project-owned spanned lexer/parser and typed semantic admission; text and Rust-token adapters share semantics without token stringification. `quest-language` is below `quest-macros` and `quest-circuit`; `quest-qasm` imports/exports structured semantic IR.
- Full structured classical control: scoped declarations, bool/bit/int/uint/angle/float, widths 1–64 (float 32/64), fixed arrays, dynamic indices, typed I/O, if/switch/for/while/break/continue/return/end, nonrecursive mixed `def`, alias/mutability checks. Unsupported hardware/timing/calibration/extern/complex constructs return capability diagnostics.
- Classical angle is modular; gate arguments remain unwrapped. Integer division remains integer. Floating pi never acquires ideal symbolic privileges. Correct OpenQASM 3.1 U phase everywhere and pin standard-library evidence.
- Program-owned distinct IDs, block arguments, sealed predecessors and terminated blocks, definite assignment, independent dominance/type/effect/interface/resource verifier. Quantum block DAGs exclude CFG back edges; classical CSE never deduplicates gate occurrences.
- Consuming verified stages; bounded interpreter (10,000,000 steps, 64 frames), checked source/IR/storage/preparation budgets, transactional native preparation and provenance-bearing errors. Retain structured IR as canonical export authority. Bounded/cycle-checked explicit includes; compiler-tracked macro files; captures evaluated once in construction order.
- Existing strict workspace lints, faer with explicit Par::Seq, RAII lifetimes, thiserror public contracts and pure-crate unsafe prohibition remain in force. color-eyre tooling only with track-caller and entrypoint hooks.
- Exact guarded transactional optimizations: classical constants/CSE/dead branches, dependency cancellation/commutation, affine parity phase folding, Gaussian then PMH CNOT synthesis, union-support fusion with a default maximum of four qubits with simulator-oriented state-vector/density costs.
- Optional one-request Linux workers: 30 seconds, 512 MiB memory, 64 KiB output; deterministic recorded seeds, versioned protocol, no partial replacement, capability error on unsupported enforcement. Candidate failures skip optional transformations; explicit synthesis fails its contract.
- rsgridsynth 0.2.2-derived pinned hardened fork: exact dyadic/rational-pi targets, pre-cache precision max(256,4*ceil(log2(1/epsilon_search))+64), <=1024 bits, bounded integers/search/output, exact number-theory verification, preserve phase/W and reverse product order. Rz plus exact Rx/Ry conjugation. Independent cyclotomic matrix and rational interval certificates; mandatory successful 1e-12 fixed/seeded fixtures. Separate mathematical and empirical native error; only justified approximation composition.
- QuiZX 0.3.0 worker: effect-free Clifford+T <=4 qubits and <=128 gates, no ancillas. Independent exact full-matrix equality, restore only verified eighth-root phase, reject unsupported outputs/extraction failures.
- Shared owned stable diagnostics, immutable source snapshots, multiple labels/traces/provenance, optional codespan/serialization. mdBook plus rustdoc, executable examples, migration/feature matrix, phase and arithmetic semantics, SSA, budgets and optimizer contracts. Criterion separates construction/compilation/preparation/execution.

## Milestones

- [x] M6: Strict lint compliance, U convention and independent phase fixtures, shared diagnostics and semantic gate registry.
- [x] M7: Shared typed admission, structured regions, SSA construction and independent verification.
- [x] M8: Text import/export, shared macro semantics and declarations, structured execution, extensive executable language guide.
- [x] M9: Exact classical/quantum transformations, PMH/phase resynthesis, union fusion and simulator cost reporting.
- [x] M10: Bounded worker infrastructure and independently certified scientific synthesis; mandatory 1e-12 successes.
- [x] M11: Exactly checked ZX candidates and integrated benchmarks.

## Acceptance record

Final integrated acceptance passed on Linux GNU with QuEST 4.3.0: 315/315 workspace
Nextest tests, zero skips, 13 doctests, strict all-feature/all-target Clippy,
formatting, workspace/example/rustdoc builds, six Criterion smoke stages,
mdBook, generator freshness and all twelve package inventories. A fresh pure
frontend/compiler build passed with QuEST/libclang paths unavailable; 164 pure
no-default tests passed. External direct and wrapped downstream executables
loaded and ran with loader overrides unset, including the indirect GPU library
closure. Certified synthesis, exact ZX, structured effect preservation and
independent worker/resource regression tests are included. Platform and
mathematical-versus-numerical limits are recorded in
[the verification record](../../verification/2026-09-10-m6-m11.md).

## Interface review and rulings

| Producer / consumer | Shared contract | Initial review |
|---|---|---|
| M6 / M7 / M8 | Language gate registry and owned diagnostics | Registry must have no circuit or macro dependency. |
| M7 / M8 | Typed structured module and verified SSA | Export retains structured module; execution consumes independently verified SSA. |
| M7 / M9 | Effectful SSA and quantum DAG | CFG cycles do not become DAG cycles; all passes reverify. |
| M9 / M10 / M11 | Transactional candidate replacements | No engine result is accepted without project-owned verification. |
| M10 / M11 | Cyclotomic arithmetic and bounded workers | Exact equality and approximate certificates share arithmetic, not engine proofs. |
| Every milestone | Tests, docs and published guarantees | Milestone checkboxes require acceptance evidence; no placeholder can satisfy them. |

Ruling: independent agents may edit disjoint crate files in parallel; root owns workspace manifests/lockfile and integration. This follows the active collaboration instruction and avoids shared-edit conflicts.
Ruling: create an isolated worktree under the already ignored `.superpowers/worktrees/`; user implementation authorization covers this reversible setup. Original main remains intact.

## Progress

2026-09-10: Started from clean signed commit 5beefbc on main. Worktree `codex/openqasm-ssa`. Baseline strict Clippy is known failing and M6 explicitly authorizes repair. Previous tests are historical evidence, not acceptance of this change.

M6 component evidence (2026-09-10, in progress toward integrated acceptance):
- Circuit/runtime: independent U red fixtures reproduced missing phase; direct/fused, forward/adjoint and signed-control fixtures green. 52/52 affected Nextest tests, 3 doctests and strict all-target/all-feature Clippy passed (dependency lint isolation only while other crates were being edited). Two ownership UI rust-src excerpts refreshed on the pinned toolchain.
- Bridge/tooling: quest-build 8/8, xtask 13/13, quest-sys 31/31; generator freshness and combined three-package Clippy passed. Native configuration regenerated against installed QuEST 4.3.0.
- Registry/diagnostics: 11 all-feature and 7 no-feature contracts passed. A review-discovered missing theta/2 in U registry phase was fixed with a failing regression before correction.
- Source/macro: shared registry integrated; checked operand/rational arithmetic replaces panic paths. Full integrated check remains pending while M7 modules are under active construction.

M7 progress: spanned shared grammar and scalar numeric value admission are implemented. Syntax precedence/control/declaration fixtures and integer/modular-angle regressions pass. SSA construction, scalar promotion, independent verifier and broader semantic fixtures are in progress. None of these partial checks marks M7 complete.

Ruling: scalar signed overflow is a structured execution error; unsigned arithmetic wraps at its admitted width. Angle narrowing uses the specification-permitted truncation policy, while float-to-angle conversion uses round-to-nearest ties-to-even. Document both in the DSL migration/semantics guide.

2026-09-10 steering: the user confirmed concurrent dependency upgrades should be retained: num-bigint 0.5, sha2 0.11 and shlex 2. The bridge's SHA256 encoding was adapted with an independent known-digest fixture. The circuit exposes its own `BigRational = Ratio<num_bigint::BigInt>` alias because num-rational 0.4's built-in alias fixes bigint 0.4. Binary64 conversion now rounds the exact ratio once, including ties/subnormals, and rejects overflow; 17 focused rational/circuit tests passed. This is a deliberate API migration, not a dependency downgrade.

M6 independent review: no open phase, ownership, source-refactor or macro-registry correctness findings in the reviewed M6 scope. Upgraded digest compatibility and selected color-eyre caller locations are verified. M7/M8 work and later bigint migration still need integrated final review.

M8 integration progress: the primary `circuit!` now uses the shared grammar/checker and returns `StructuredProgram`; `legacy_circuit!` retains the earlier static ideal-angle DSL for explicit migration. `circuit_file!` and macro includes use compiler-tracked `include_str!` expansion. Captures evaluate once and retain source locations. The facade prepares structured SSA transactionally and executes through the bounded interpreter against either register kind.

Focused actual-worktree integration: 26 tests passed across syntax, classical builtins, structured stages/macros, static optimization and native structured runtime. These include user gates/subroutines/loops, file includes, absent captures, Bell feedback, exact density reset, loop budget errors and full-phase cu/CX basis columns. One initial cu fixture incorrectly omitted U's theta/2 factor; it was corrected against the normative standard-library equation before passing. This is not full workspace acceptance.

Independent upgraded arithmetic review: 50,171 rational binary64 fixtures and 17,810 rational-times-pi fixtures matched separate Python Fraction and high-precision Gauss-Legendre oracles. A discovered premature subnormal underflow in rational-pi binding was fixed using certified adaptive Machin intervals. Ambiguous rounding at the bounded precision cap returns a resource error. Permanent bit-level fixtures cover subnormals, exact ties and difficult midpoint refinements.

Text frontend component acceptance: 15 integration tests and 1 doctest passed, including immutable include resolution, cycles/budgets, canonical structured round trips, exact pinned standard-library authority and forged-body rejection. Registry elision is shared by text and macro admission. Source provenance and the two explicit standard-library phase corrections are documented in `docs/openqasm-provenance.md`.

Profile rulings: ordinary integer narrowing preserves low bits consistently in explicit casts and assignments; signed arithmetic overflow remains an error. Range slices and register concatenation currently produce capability diagnostics. They are outside this initial simulator profile; scalar dynamic indexing and multidimensional comma indices are supported. Width-preserving circular bit rotation, standard `log`/`ceiling`, `mod`/`pow` and unsigned `popcount` are shared classical builtins.

M8 guide component evidence (2026-09-10): 12 mdBook chapters now cover one frontend/builder/export feature matrix, grammar and precedence, numeric widths/casts and U migration, calls and arrays, SSA/sealing/verification, explicit includes, runtime ownership/budgets, owned diagnostics, exact and numerical optimization, and worker certificate boundaries. Nineteen source includes reference executable Rust examples/tests or the compiler-tracked Bell QASM fixture. Root and circuit READMEs now distinguish primary structured `circuit!` from `legacy_circuit!` and the ideal builder. Dated audit documents are unchanged.

Guide verification: mdBook 0.5.4 installed in project-local `target/tools`; HTML builds and include/anchor/chapter-link validation pass. Pure tutorial tests pass 9/9 with macros and 7/7 without defaults; circuit doctest passes. Native tutorial tests pass in an isolated process, covering Bell, teleportation, feedback, bounded repeat-until-success, once captures and mutable array arguments. Both ordinary and worker-enabled CLI runs pass against installed QuEST 4.3.0. The explicit real synthesis worker returns a checked 1e-12 candidate (340 gates for the recorded 0.17-radian target, seed 2026), and both worker-enabled integration tests pass. The structured quantum tutorial verifies H/H cancellation with original syntax retained. Focused strict tutorial Clippy passed before a concurrent SSA allocator doc edit; final integrated lint remains the root verification responsibility. These component results do not mark the entire M8 milestone complete.

Bounded independent SSA/runtime review (2026-09-10): red regressions exposed noninteger gate-power admission, foreign region-entry identity acceptance, duplicate named I/O, invalid parameterized/value-returning program entries, and statically forbidden explicit casts. Fixed with shared Int/Uint power checks and authoritative OpenQASM 3.1 explicit-cast rules at admission/verifier/runtime boundaries. Gate parameter interpretation now has a distinct pure SSA instruction so stored angles still supply radians without granting forbidden Angle→Float source casts. Twelve added regression functions and the existing adversarial suite pass: 86 language runtime tests plus 7 compile-fail doctests; strict all-target/all-feature language Clippy passes. The SSA allocator documentation lint is fixed; guide cast/power text and mdBook render updated. Root workspace acceptance remains separate.

### Structured worker review follow-up (2026-09-10)

Independent adapter review added bounded deterministic classical tracing of original verified SSA for capture-bound finite-loop rotation multiplicity, while retaining acyclic maximum-path fallback and local-only certificates when no whole-program proof completes. Mixed signed-control physical mappings and a real extracted nonidentity scalar have exact worker regressions; W lowering has independent controlled-phase fixtures. Two adapter unit tests, five real-worker structured contracts, native state/density comparisons, both executable tutorial tests, scoped strict Clippy and mdBook 0.5.4 build passed. Detailed scoped evidence: `/tmp/quest-m11-structured-worker-review-report.md`. This entry records component evidence and does not change milestone acceptance checkboxes.


Final M9–M11 integration (2026-09-10): structured SSA now has explicit exact
quantum windows and optional certified synthesis/ZX methods. Worker adapter
lowering is checked exactly, including scalar W and mixed signed controls on
nonsorted interfaces. Fresh program-owned result allocation preserves memory
chains and every transformed program passes independent verification. Original
structured source remains the export authority. Deterministic observation-free
classical traces prove finite loop/call multiplicities within separate analysis
budgets; acyclic branches use maximum-path composition, while unresolved cases
keep local certificates only. Numerical fusion remains the bound ideal-circuit
stage and reports its sequential faer rounding and simulator cost estimates.

Final review repaired integer-only gate power admission, authoritative explicit
cast rules (with a separate gate-parameter interpretation instruction), foreign
region entry IDs, duplicate I/O interface names, process leader cleanup ownership,
resource admission before cloning and synthesis tolerance checks. No lint policy
was weakened. The executable guide includes a tested capture-once/finite-loop
certificate example. All milestone checkboxes above refer to the documented
simulator profile and the scoped initial synthesis/ZX contracts, not full hardware
OpenQASM or unverified platforms.
