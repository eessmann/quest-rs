# Correctness-first review and consolidation

Status: implementation authorized by the user on 2026-09-27.

## Goal and constraints

Consolidate all 18 workspace crates without weakening mathematical correctness,
deterministic arithmetic, ownership, resource admission, or independently checked
evidence. Justified public API breaks are approved. Preserve the pure/native
crate boundaries, ideal DAG versus structured SSA semantics, current OpenQASM 3.1
simulator profile and macros. New quantum optimization algorithms are research
only. Verify Linux GNU and available CPU/GPU/MPI modes; report unavailable
coverage honestly. Native QuEST is the installed 4.3 fork at
`/var/home/erich/Projects/opt/quest`, source `/var/home/erich/Projects/QuEST`.

Baseline: HEAD 9d614fb; formatting and compiler/frontend Clippy passed; 189 pure
compiler/frontend tests and 74 native tests passed (native tests outside sandbox).
Full workspace check was blocked by absent serial HDF5. MPICH reinstall repaired
the native dependency closure. Existing audit memories are historical evidence,
not the authority for current implementation.

## Task 1: Native correctness

Validate probability target counts, bounds, uniqueness and output size before
native exponential allocation. Modify generator source and regenerate outputs.
Add invalid-count/duplicate/index regressions. Generator tests must report an
explicitly selected but broken package as failure, not skip it.

## Task 2: Numerical and IO correctness

Preserve empty Laurent polynomial zero semantics in generalized synthesis.
Replace independently mutable generalized-angle fields with an immutable,
constructor-admitted payload. Separate source interchange from frozen execution
inputs; MPI transmits exact admitted matrix words with immutable provenance.
Account sparse data, indices and pointer storage consistently. Preserve primary
computation errors when trace export also fails. Regression first for each bug.

## Task 3: Optimization correctness and provenance

Withhold structured whole-program approximation bounds through numerical oracle
calls without sufficient unitary/norm evidence; retain local certificates. Cover
direct, nested, controlled and adjoint calls. Replace cumulative provenance
clones with an append-only DAG of immediate rewrite inputs, budget source-leaf
expansion, work and retained storage, and expose bounded optimization options.
Maintain occurrence identities, source provenance and exact global phase.

## Task 4: Native consolidation

Share one CXX complex type and remove redundant Rust conversion buffers. Generate
repetitive adapter families from reviewed descriptors; keep exceptional adapters
explicit and check descriptor/wrapper correspondence. Centralize C++ owner
cleanup while preserving fail-closed lifecycle/accounting and distributed
behavior. Use one authoritative evaluated CMake discovery result. Preserve final
executable RUNPATH helpers and indirect dependency responsibilities.

## Task 5: Compiler and facade consolidation

Collapse ideal LoweredProgram into checked BoundProgram::plan(); retain real
structured lowering. Centralize gate adapters, traversals and remapping without
merging scalar domains. Freeze published operand collections and share immutable
payloads without merging occurrences/evidence. Consolidate facade allocation,
reset, controls and matrix transfer helpers. Update examples/docs/compile-fail
fixtures for API changes.

## Task 6: Language representation and verification

TypedModule retains verified SSA; reverify actual transformations and untrusted
inputs, not unchanged immutable values. Use shared immutable typed-builder
expression nodes with construction/materialization budgets. Replace cloned
dominator sets with bounded per-region immediate-dominator analysis. Give
analysis handles and reports immutable snapshot identity. Preserve capture order,
diagnostics, effects, dominance/definite-assignment and independent verification.

## Task 7: Numerical consolidation

Use a direct/projected-continuation enum instead of paired optional fields.
Share storage calculations and synthesis/reporting orchestration while retaining
arithmetic order and separate production/offline/certification implementations.
Bound Remez subdivision/storage/work and use fallible solver scratch allocation.

## Task 8: Integration, review, evidence and roadmap

Run workspace build, Nextest, separate doctests, fmt, Clippy and generation
freshness with serial HDF5; exercise workers, certification, offline synthesis,
HDF5 and MPI feature configurations. Test direct/facade/renamed/wrapper consumers
outside Cargo with loader overrides cleared; inspect RUNPATH and dependencies.
Exercise actual GPU and multi-rank MPI independently. Preserve failed/blocked
validation evidence. Review changes independently and resolve actionable defects.

Add a severity-ranked review record, migration notes and measured before/after
results (duplicate rules, bytes, allocations and API complexity). Benchmark long
merge chains, shared expressions and branch-heavy SSA. Research roadmap maps
commutation/fusion, GUOQ rewrite/resynthesis, QSSA value flow, Booth bounded
meet-in-the-middle synthesis, parity and ZX candidates to representations,
evidence, limits, cost and benchmarks. Use primary sources; do not implement new
quantum algorithms in this pass.

## Delivery

Correctness changes precede structural consolidation. Make focused commits for
independently reviewable subsystems. Preserve main, unrelated worktrees and native
installations. No remote publication or merge is part of this authorization.
