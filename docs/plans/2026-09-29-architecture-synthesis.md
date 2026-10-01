# Architecture and synthesis implementation

The solver-default statements below record the September 29 design and validation.
The October 1 mathematical audit restores inverse NLFT as the production, offline
and catalogue default while retaining explicit RHW and persisted artifact identities.

Approved by the author in the Codex conversation on 2026-09-29. This document
records the implementation contract; it does not claim completion.

## Global requirements

One public program/builder and preparation/execution lifecycle must preserve the
union of the finite exact circuit and structured language capabilities. Native
arbitrary-angle state-vector/density-matrix simulation is the default. Discrete
gate synthesis and expensive offline QSP compilation are explicit. Breaking API
changes are intended: no compatibility macro, alias, shim or alternate legacy
pipeline may ship. Preserve user edits in the primary checkout and the C++ tree.

Keep exact symbolic angles, finite floating values and dynamic expressions
distinct. Preserve OpenQASM arithmetic, modular angles, once-only Rust captures,
original binding obligations, traps, effects, signed controls and full phase.
Numerical tolerance never grants an exact-unitary proof. Artifacts preserve
target/source identity, conventions, algorithms, bindings, limits and evidence;
loading revalidates rather than trusting serialized witnesses.

Candidate generation and independent certification remain separate. Error
reports distinguish mathematical rejection from budgets, precision uncertainty,
cancellation and malformed evidence. No silent algorithm/precision fallback.

## Task 1: Review record and baseline

Record actual checkout identity, toolchain/configuration and test results. The
review baseline is 7148ccaa47e9e39506d62ba1e441d657040b83f8. The initial main tree
also had Cargo.toml/Cargo.lock/devenv.nix/devenv.lock edits, which are not part of
the isolated implementation. C++ comparisons use immutable git objects, never
its dirty worktree: origin/main a932e7e081ac3766cad19ad6f8f4b920c8fa7fcf and
develop 4fc35983138d07a990862a4d83ad16f2b737c98f. Older fixtures at 7fe7f740 remain
historical evidence. At this initial review, live C++ branch parity had not yet
been established; the final verification record reports the completed comparisons.

## Task 2: Shared semantic core and compiler

Expand quest-language into the native-free shared semantic core, including typed
builder, checked program and quantum-region IR, exact/dynamic gate arguments,
identities, sources, diagnostics, effects, immutable matrix/oracle payloads.
Introduce quest-compile for exact rewrites, region extraction/reinsertion,
specialization, numerical fusion, cost policies, portable execution lowering and
reports. It depends on the core/math, never the facade or macros. quest-circuit
is the public facade. quest-macros depends directly on language/qasm/compiler.
Native handles and RAII remain in quest.

Extract the used exact affine implementation into project-owned symbolic code
and remove the unused vendored mathcore CAS. Preserve independent source replay
and all source conversion/binding obligations.

## Task 3: Unified construction, staging and runtime

Provide one ProgramBuilder and staged Program family, one Environment::prepare,
PreparedProgram and execution result. All frontends enter the same checked core:
construct -> verify -> specialize/compile -> prepare -> run. Complete builder
coverage for measurement/reset, inputs/outputs, arrays/references, definitions,
calls, signed controls, powers, adjoints and matrix/oracle payloads. Unitary
capabilities are checked views/witnesses, not a second public program model.

The circuit macro emits the already-checked template instead of runtime tokens
to parse/admit again. Typed captures accept explicit exact Angle expressions
(e.g. rz(${Angle::pi(1,4)?}) q) while ordinary QASM floats stay floats. Runtime
captures evaluate once in source order, never during macro expansion.

Specialize static operands/angles/modifiers once into reusable dispatch records;
retain VM handling for actual dynamic inputs/control/feedback. Keep source
export and compiled-artifact export explicitly distinct. Exact and numerical
passes use common region adapters and independently verify reinsertion.

Migrate all consumers and behavioral tests, then delete legacy_circuit and its
parser, old lifecycle aliases, duplicate public paths and obsolete dependencies.
Do not discard semantic test coverage merely because the old API disappears.

## Task 4: Rust synthesis

Add quest-synthesis for fresh paper-derived candidate algorithms depending on
quest-math. Newsynth 0.4.1.0 is a pinned source/executable reference, not code to
relicense by translation. Retain full provenance. Extend canonical cyclotomic
arithmetic with typed rings/residues/phases, distinguishing powers-of-two from
least sqrt(2) denominator exponents.

Implement exact one-qubit Clifford+T synthesis and Matsumoto-Amano normalization,
then bounded Ross-Selinger Rx/Ry/Rz approximation for exact dyadic radians,
rational pi and affine r+s*pi. Independently certify actual output against the
exact epsilon. Rz(pi/4) is not exactly T; preserve scalar phase.

Implement Giles-Selinger multiqubit reduction for exact D[omega] matrices, with
deterministic column order/residue pairing/phase choices. Use typed basis indices
and Gray-code lowering to actual elementary Clifford+T gates. Arbitrary controls
are not allowed to remain as disguised terminal gates. Default AllowOneClean
allows at most one reusable clean ancilla; explicit NoAncilla enforces the
paper's determinant restrictions. The contract is C J = J U (clean input and
return), not C = U tensor I. Never reset an occupied wire implicitly.

The independent verifier replays matrix reduction, permutations, local gate
identities and composition tied to the emitted word. Use bounded in-place replay
and dense small-case oracles. Admit matrix/proof/coefficients/output/work before
allocation; retain typed failure reasons, request-owned precision and a pinned
RNG with deterministic logical work. Direct bounded Rust compilation must work
on macOS and Linux, without mandatory Linux-only process client. External
engines remain optional. Completely remove rsgridsynth after replacement passes.

References: https://arxiv.org/abs/1403.2975,
https://arxiv.org/abs/1212.0506,
https://hackage.haskell.org/package/newsynth-0.4.1.0.

## Task 5: RHW and QSP/QSVT evidence

Implement explicit algorithm identities: RhwHalfCholesky (default after tests),
InverseNlftDivideConquer (supported explicit alternative), dense RHW (bounded
independent test oracle). Keep algorithm, convention, precision, FFT backend and
execution policy independent. Support binary64 and explicit offline precision.

RHW needs a typed Weiss ratio containing Fourier coefficients of b/a computed
from samples b*exp(-G*), preserving target/gauge/grid/contractivity evidence.
Implement the actual structured Half-Cholesky displacement recurrence rather
than renaming the existing inverse or dense LDL. Compare to the direct complex
Toeplitz block solve. References: Laneve 2503.03026v2 section 5 and Ni/Ying
2410.06409v2 sections 2.4-2.6. The reflection coefficient indexing and conjugation
must be pinned independently before making it the default.

Use explicit Wx-real-parity and generalized-circle convention identities,
separate from generalized gauge. Preserve basis/parity/support/normalization and
route-specific lifted/reduced polynomial meaning through QSP into QSVT. Do not
conflate z^2, x^2 and T2(x). Conversion evidence is immutable and bound to both
ends. Separate storage span from effective support and mathematical degree,
including zero. Unavailable diagnostics are typed, not infinity sentinels;
unestablished bounds are not proved violations.

Retain independent certification of actual frozen controls and all four matrix
entries. Add separately certified Wx-to-projector conversion bound to the
converted payload, domain/norm/convention; do not inherit pre-conversion proof.
Certified consumers demand exact matching evidence; numerical imports remain
explicitly weaker. 2310.12683v2 is theory under its stated assumptions, not a
blanket floating point guarantee; 2312.00723v1 gives distinct transform routes.

Create isolated adapters for both pinned C++ revisions and a branch-specific
capability/convention/provenance manifest. Compare full complex operators,
responses, normalizations and certificates; raw phases alone are insufficient.
Keep exact real/parity admission in Rust and record C++ develop's tolerance-based
projection as intentional difference. Preserve stronger Rust frozen-export checks.

## Task 6: Acceptance and documentation

Migrate the book, crate docs, examples and CLI to the unified API; no current
documentation may direct users to removed compatibility paths. Record exact
commands/results and remaining platform evidence boundaries under verification.

Run cross-frontend and compile-fail contracts; exact-vs-float angles; controls,
global phase, adjoints, loops, feedback, alias/trap/partial execution; capture and
specialization reuse; artifact rejection. Synthesis tests cover normalization,
ring laws, affine approximation, determinant classes, clean-ancilla leakage,
corrupted proofs, deterministic concurrency, every resource failure. QSP tests
cover dense-vs-structured RHW, both parities, complex/asymmetric/support-offset
targets, near-contractivity, actual converted export and rectangular QSVT.

Measure compile/synthesis/certification/prepare/repeated-run time separately,
including memory and gates; do not infer speedup from design. Run ignored large
fixtures explicitly in release mode. Run workspace nextest/doctests, format and
Clippy, binding freshness, native consumers, and book build. Distinguish macOS,
Linux, native, GPU and MPI validation; never claim unexecuted platforms passed.
