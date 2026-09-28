# Consolidation migration notes

This change intentionally tightens APIs while preserving the ideal circuit,
structured language, native runtime and independent certification boundaries.
The release remains tied to QuEST 4.3.x with binary64 and deprecated native APIs
disabled.

## Provenance and optimization admission

Instruction provenance is an opaque `ProvenanceId`, not an eagerly expanded
list of all source occurrences. Use the owning program/plan/report's
`ProvenanceGraph::node(id)` for immediate rewrite inputs or
`source_leaves(id, ExpansionLimits)` for a bounded, sorted unique source list.
Shared ancestor provenance IDs remain valid across cloned histories. A newly
appended node from a divergent or foreign history is rejected even if its numeric
position matches. Reports and plans share immutable history storage; this node
identity rule differs from whole-publication SSA snapshot handles below.

`optimize_exact()` uses bounded defaults. Use
`optimize_exact_with_options(ExactOptions)` when application limits differ.
Numerical fusion separately limits provenance work/storage and matrix work.
A budget failure does not publish a partially transformed program. Limits are
checked storage/work admission, not a process-RSS or wall-clock guarantee.

Structured synthesis reports retain local certificates but return no global
operator bound when reachable numerical oracle calls lack compositional
unitarity/norm evidence. Treat `None` as unavailable evidence, never as zero error.

## Language construction and snapshots

`TypedModule::into_ssa()` and `into_verified_parts()` retain their `Result`
return types but now transfer the already verified immutable representation.
Edits and untrusted `Program::verify()` calls still verify independently.

Each verified publication has a `SnapshotId`. Cloning a publication keeps it;
verifying an edited or copied unverified program creates a new identity. Use
`block_handle` and `block` when retaining block references across API calls;
a handle from another publication will not resolve. Optimization reports record
their input and output snapshot identities.

Typed `Expr` clones share immutable construction nodes. `Builder::with_limits`
bounds expression construction and expansion; `finish(limits)` independently
admits the complete program. Very deep or exponentially expanding expressions
now fail with a resource diagnostic before syntax materialization. Shared source
expressions still materialize separate execution occurrences and preserve the
language's evaluation order, checked arithmetic and short-circuit behavior.
`Builder::boolean` is now fallible (`Result<Expr<Bool>, SemanticError>`) so even
a literal obeys construction budgets. It is no longer a `const fn`.

## Native failures

The safe multi-qubit probability adapter validates the target count, indices,
duplicates and representable output size before allocating exponential output.
Invalid requests now return bridge errors before reaching the native vector
overload. Native validation remains active for backend and register constraints.

Binding-generator tests may skip automatic discovery when no native package was
selected. An explicitly configured but broken package now fails those tests.
Fix the selected `QUEST_ROOT`/CMake package path instead of interpreting a skipped
fixture as evidence that generation passed.

## Ideal execution stages and operands

Replace `bound.lower()?.plan()?` with `bound.plan()?`. `BoundProgram::plan()`
consumes the program and retains the native-index checks; the empty ideal
`LoweredProgram` type is removed. Structured programs still use
`verify()?.lower()?.plan()?`, because their lowering performs substantive work.

Published operation operand and Kraus collections use immutable `Arc<[T]>`
storage. Borrow them as slices; use `.into()` when constructing an operation
from a `Vec`. Sharing storage does not merge instruction occurrences or their
certificates. `Operation::operands()` and `qubits()` provide shared traversal;
`operand_storage_bytes()` reports retained operand storage including conservative
Arc overhead, separately from numeric matrix payloads and native scratch.

Use `BoundGate::from_kind(kind, parameters)` for checked adaptation from the gate
registry after intrinsic controls are separated. It rejects extra/missing or
nonfinite parameters. Global phase remains a scalar operation. Exact rational,
symbolic, floating-radian and modular classical-angle semantics remain distinct.

## Numerical input and continuation

`QspInput::GeneralizedAngles` now wraps `GeneralizedAngleInput`. Construct it with
`GeneralizedAngleInput::new(psi, phi)?`; access source angles and execution
controls through shared getters. Source export remains angle-based. Use
`write_qsp_execution_json` / `read_qsp_execution_json` for frozen execution
transport: `gqsp-matrix-words-v1` stores matrix components and angle provenance as
exact `u64` words. Workers decode those words without trigonometric rebuilding.
The execution reader rejects competing decimal/word authorities.
Imported angle words describe provenance; they do not certify trigonometric
consistency with the frozen controls. Source re-export is therefore not an
execution-preserving substitute for frozen execution export.

`ValidatedTransform::continuation_stage()` returns `TransformContinuation`:
`Direct` or `Projected { bridge, program }`. Existing `bridge()` and
`continuation()` are derived views of this single immutable state. Admitted and
prepared native transforms, including Hadamard overlap experiments, likewise
own their projection and continuation together.

Remez `max_bytes` admission now includes solver scratch and subdivision-stack
storage together. Enclosure `max_work` accumulates checked AST/coefficient-scaled
evaluation work across precision retries. Policies accepted by the previous
incomplete forecast may now return a typed budget error; raise limits explicitly
only when the application can admit that cost. Scratch allocation failures also
return typed errors.

CLI computation plus trace-export failures return `DispatchAndTrace`; the
primary computation error is the error-chain source and both causes remain
available. This does not turn a failed computation into success.
