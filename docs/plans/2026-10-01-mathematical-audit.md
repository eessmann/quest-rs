# Mathematical audit and nightly-first Rust implementation

Approved specification: the user's 2026-10-01 implementation request in this chat.
Baseline Rust commit: `4f873db4a01bddfbe2f98a317fa2edccde9ff53f`.
Reference C++ checkout: `/Users/erich/Projects/quest-qsvt`, read-only.

## Task 1: Generic mathematical functions and Remez

Implement a backend-generic evaluator for scalar, outward interval, derivative
jet and arbitrary-precision arithmetic. Add statically dispatched
`typed_function!`, const construction and static metadata, preserving `Function`,
`Expr` and `function!`. Keep trusted expression evidence sealed and custom
callable evidence conditional. Preserve exact captured binary64 constants and
the original target through offline precision attempts and export. Parameterize
owning builder states and reports with compatible defaults. Add optional
const-generic degree support, using nightly const traits and generic const
expressions for actual structural invariants. Keep numerical convergence,
precision, budgets and certification at runtime. Test independent analytic
values/derivatives, backend agreement, evidence admission, static dimensions,
non-Copy scalars and actual exported coefficients.

## Task 2: Mathematical utility parity

Compare supported C++ capabilities with Rust and implement symmetric
Laurent-to-Chebyshev conversion with rounding evidence, interval/disc norms with
pole handling, extended Newton, Krawczyk and scalar/vector Hansen-Sengupta
contractors, and dense Pauli decomposition. Require bounded work, finite
arithmetic and sound interval enclosures. Record precise capability scope and
test against independently derived examples, including singular and empty cases.

## Task 3: Solver defaults and QSVT applications

Make inverse NLFT the default in library, offline, catalog and CLI paths; retain
explicit RHW and artifact-recorded algorithms. Do not introduce automatic
algorithm switching or relaxed tolerances. Fix intermediate overflow in rank
thresholds and inspect scale/residual arithmetic. Complete catalog solving,
automatic routes, embedded-matrix workflows, seeded presets, and raw physical
register I/O while retaining logical interfaces. Validate route/domain/shape
contracts and physical interpretation. Test default/explicit algorithms and
persisted artifacts, rectangular/rank-deficient and extreme-scale cases.

## Task 4: Evidence ledger, circuit audit and consolidation

Produce a supported-capability matrix and paper-to-code evidence ledger. Audit
contractivity, outer completion, recursion, controls and terminal factors; all
seven QSVT routes; exact ring/determinant/denominator/norm-equation contracts;
full-phase and clean-ancilla replay; lattice-region containment; and worker
recertification. Distinguish soundness, search completeness, minimax gaps,
function/export/synthesis/execution errors and hardware versus query costs.
Consolidate provenance and fixed-budget failure handling without weakening the
independent verifier. Fix demonstrated issues with regression tests.

## Task 5: Integration, review and validation

Run focused tests followed by workspace tests, doctests, Clippy, formatting,
bindings and consumer checks in the project environment. Cover both synthesis
algorithms, parallel/scalar/SIMD execution, large-degree regressions and
native modes where available. Record exact compiler/source identities and
runtime/allocation/build-size measurements. Independently review the diff and
fix findings. Explicitly separate unavailable Linux/MPI/accelerator validation
from local evidence. Deliver migration documentation and reproducible records.

## Constraints

Preserve APIs through adapters where practical; no changes to Zotero or C++.
Keep independent verification arithmetic separate from producer arithmetic.
No multiprecision interval certification, signed-Laurent signal schedules or
new circuit algorithms absent from both projects. Do not commit, push or merge
as part of this implementation request; leave the reviewable worktree diff.
