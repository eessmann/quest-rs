# Optimization and certificates

Optimization APIs report which representation changed and which guarantee was established. A smaller gate count, a floating-point residual, and an exact certificate are different evidence.

## Classical structured optimization

The consuming `optimize_classical` wrapper preserves source snapshots, frozen structured syntax, and once-evaluated captures. Constant propagation, pure common-expression elimination, branch simplification, and dead pure computation are bounded and the resulting SSA is independently verified.

```rust
{{#include ../../../crates/quest-compile/tests/tutorials.rs:classical_optimization}}
```

Constants flow across joins only when incoming values justify the same result; loops use a bounded fixed-point analysis. A potentially trapping operation is retained unless its successful evaluation is proved. Quantum operations, memory effects, and stochastic events cannot be removed as dead scalar work.

The exported syntax in this tutorial still contains `else`, although the executable branch was simplified. This is intentional: `export_source` preserves the original syntax, while `export_compiled` preserves the optimized executable.

## Structured quantum windows

The explicit `optimize_quantum` pass works within individual SSA blocks. It
resolves static scalar qubit places and constant indices, cancels exact inverse
pairs across proven commuting operations, and resynthesizes CNOT/Clifford+T affine
parity windows. It never puts CFG back edges into a quantum DAG.

```rust
{{#include ../../../crates/quest-compile/tests/tutorials.rs:structured_quantum_optimization}}
```

Calls, effects, barriers, dynamic operands, reference-parameter slots, broadcasts,
and power modifiers end these windows. Identical floating parameter bits may
justify structural cancellation against an explicit inverse; they do not become
ideal rational-π angles or permit floating angle merging. The pass preserves
syntax, captures and sources, records input/output occurrence provenance, and
independently verifies all SSA after transformation. `StructuredQuantumOptions`
separates work, storage, compile and linear-synthesis limits. The explicit synthesis methods described below operate separately. Numerical fusion
uses the bound finite-region representation.

## Exact region transformations and fusion

`QuantumRegionBuilder` retains exact rational angles and symbolic parameters until binding. Finite passes are compiler extension traits imported through `quest_compile::prelude::*`. Bind original obligations and import the result into `Program<Constructed>` for execution. `ProgramBuilder` can embed the same finite capability beside classical computation. Compilation passes are explicit; construction does not run an optimization pipeline.

```rust
{{#include ../../../crates/quest-compile/tests/tutorials.rs:ideal_optimization}}
```

Exact local rewrites cancel inverse operations and merge compatible exact rotations across dependencies only when the movement is justified. Global phase and signed controls remain part of the operation. Arbitrary floating angles do not gain exact identities because two decimals look opposite.

CNOT synthesis acts on a bounded linear reversible map over GF(2). Gaussian and PMH candidates use the same ordered wire interface; selection does not change the basis permutation. The exact affine parity pass tracks X offsets and parity-dependent phases, combines equal parities, and retains scalar phase. A coherent control turns a scalar into relative phase, so discarding it would invalidate the transformation.

Numerical fusion happens after binding. It composes matrices in execution order over a bounded union of ordered targets and controls. Matrix dimension, workspace, and operation-count limits guard construction. Products use sequential faer evaluation. The report explicitly states that rounding changed; fusion does not manufacture an exact inverse or a certified approximation bound. Simulator cost reports are policy estimates, not measured speedup claims.

## Certified generation boundaries

The default `synthesis` feature exposes `NativeSynthesis`, a portable in-process Rust Ross–Selinger generator. Pass it explicitly to `synthesize_rotations`; ordinary simulation uses native arbitrary-angle gates. Its policy bounds work, precision and storage and supports cancellation. The optional `workers` feature adds process clients and QuiZX integration. Every generator candidate is independently checked against the requested target, interface and error budget; cancellation, exhausted work, unresolved precision and certificate rejection remain distinct failures.

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:certified_synthesis}}
```

This tutorial synthesizes a Z rotation from an exact dyadic representation of the requested floating angle. The rotation backend is a candidate generator; the parent checks the approximation against the declared target and budget. A per-rotation bound is local. A whole-circuit bound needs an explicit composition argument and must not be inferred by displaying the same epsilon beside every rotation.

The QuiZX adapter uses bounded Clifford+T regions with at most four qubits and 128 input gates. It neither adds ancillas nor changes the interface. Extraction can ignore global scalar phase, so the parent recovers and verifies an exact eighth-root scalar against the original full operator. Phase-insensitive equality is insufficient. Unsupported signed controls or gates produce capability errors instead of an approximate translation.

Certificates retain both the target and the accepted candidate. Reports distinguish exact replacements, local approximation evidence, numerical rounding changes, and resource failures. Source provenance and research references are documented in the repository's `docs/openqasm-provenance.md`.


## Synthesis on structured SSA

`Program<Verified>::synthesize_rotations` replaces bound
`rx`, `ry` and `rz` operations on static, nonaliased scalar operands. Run
`optimize_classical` first to expose evaluated parameter expressions; once-evaluated
Rust captures are also available to the compiler analysis. Runtime input angles,
dynamic targets and reference-parameter interfaces fail an explicit synthesis
request with the original occurrence attached.

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:structured_certified_loop}}
```

Every accepted replacement has a project-owned controlled certificate, its ordered
SSA places, seed, source occurrence and fresh result IDs. An additional exact
check proves the adapter from certified gates into SSA, including every scalar
`W` factor. The complete replacement program passes independent SSA verification.
Original source, definitions and captures remain available through source export. Compiled export retains the accepted SSA and serialized certificate evidence, including the generator identity, seed, target, candidate, limits and publication identities. Loading independently rechecks these rotation certificates.

The example captures each Rust expression once, replaces one static rotation
occurrence, and proves that the occurrence executes twice. Before transformation,
a bounded classical interpreter traces the original verified SSA with its actual
captures. Quantum gates only increment the rotation count; measurement and reset
abort this analysis. A completed trace therefore proves a count for every quantum
input, including finite loops and powered calls. This is a deterministic control
flow proof, not sampling of quantum states. Analysis uses at most one million
interpreter steps, 64 call frames and 64 MiB of storage.

If that trace cannot complete, the report instead composes sequential costs and
takes the maximum across exclusive branches through acyclic control flow and
nonrecursive calls. Input-dependent or unproved loops and measurement/reset paths
retain local certificates and return no whole-program bound. Exhausting an
analysis budget never establishes a termination claim. The resulting rational
bound counts executed replacements times the requested local epsilon.

`Program<Verified>::optimize_zx` scans bounded static Clifford+T windows
within each block. Dynamic operands, parameters, calls and effects stop a window.
Failure or lack of a native-call improvement retains the original instructions;
accepted and skipped reports identify their input occurrences. Both the engine
candidate and its SSA adapter are checked for complete exact matrix equality.
These transformations can change resource consumption, so a successful
mathematical equivalence claim does not promise the same point of interpreter
budget exhaustion.

## Source and compiled artifacts

`Program<Executable>::export_source` serializes the immutable original QASM syntax. It does not claim to contain compiler rewrites, Rust capture expressions, or native matrix payloads. Unsupported textual payloads produce an explicit export error.

`export_compiled(ArtifactLimits)` serializes the optimized SSA, scalar capture bits, exact targets, numerical payloads, oracle banks and numerical admission tolerances, source locations, original finite binding/provenance records, conventions, publication identity and resource limits. Its SHA-256 envelope detects corruption; it is not a mathematical certificate.

`Program<Executable>::load_compiled` checks the version and conventions, checks the digest, applies caller resource limits, independently verifies the executable SSA and source syntax, validates captures and payload interfaces, and reconstructs numerical/oracle admissions. It retains the serialized executable graph even when it differs from source syntax. Native resources and prepared dispatch records are rebuilt. Incompatible versions require rebuilding from source.

Persisted rotation records have the explicit scope `HistoricalLocalCertificate`. They retain original input/output identities, occurrences, interfaces and emitted operation identities, and their local target/candidate relation is independently checked on load. They grant no current-executable equivalence or whole-program error capability after later rewrites. Finite-region synthesis attaches these records to immutable provenance, so binding, normal common-program import and compiled export retain them automatically.

Artifact admission uses conservative decoded-storage bounds before deserialization, aggregates matrix/oracle reconstruction storage, and reserves only remaining storage for certificate verification. These bounds can reject an artifact whose encoded text fits the byte limit; serialized size alone is not a heap budget. Duplicate serialized payload values may reconstruct as separate allocations and are charged separately.
