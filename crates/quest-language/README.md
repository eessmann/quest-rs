# quest-language

Native-independent language contracts shared by the Rust token frontend, text
frontend, and circuit representation. This crate has no circuit, macro, native
QuEST, filesystem-resolution, or global diagnostic-hook dependency. Unsafe code
is forbidden and the workspace's strict Clippy policy applies.

`GateKind::lookup(name)` resolves built-in spellings. `GateKind::definition()`
provides canonical name, Rust adapter variant, parameter and target counts,
intrinsic controls, effect classification, adjoint parameter mapping, and an
exact decomposition. These definitions are emitted from one registry. Controls
are separate from targets; `ccx`, for example, has two controls and one target.
Decomposition sequences are in execution order and preserve global phase,
including when the gate is controlled. Gate parameters are unwrapped real
angles. `U(theta, phi, lambda)` follows OpenQASM 3.1:

```text
gphase((theta + phi + lambda) / 2);
rz(lambda); ry(theta); rz(phi);
```

`SourceSnapshot` owns immutable UTF-8 text and a display filename. Callers assign
`SourceId` identities within a compilation; filenames are not identities and a
new snapshot revision needs a fresh ID. `SourceMap` rejects identity reuse
without replacing an entry. Spans are half-open byte ranges checked against
snapshot length and character boundaries. Consumers revalidate serialized spans
against the owned snapshot; deserialization alone cannot establish text bounds.

`Diagnostic` owns typed causes, stable `QL0001`–`QL0011` codes, stages, resource
quantities, sources, multiple labels, entity provenance, definition/call/include
traces, typed interpreter call frames, notes, and replacement suggestions. An
occurrence remains available when a macro has compiler coordinates but no source
text snapshot; it becomes a validated label only when that snapshot exists.
Collections may be populated during construction; `validate_sources()` checks
every field backed by available source text. Diagnostic codes derive from causes
and cannot drift out of sync with them. Parser, semantic, and runtime errors expose
conversion helpers that preserve real resource quantities and use a typed generic
cause when an error has no structured expected/found fields.

Optional features:

- `serde`: serialization of diagnostic/source data and semantic gate identities;
  deserialization rejects reversed spans and duplicate source identities.
- `codespan-reporting`: `render_plain` returns text using owned snapshots, with
  labels, traces, notes, and suggestions. It never reads display filenames.

`semantic::admit(Module, CompileLimits)` performs scoped type and definite
assignment checks before constructing private `TypedModule` admission. The
structured syntax remains the export authority. `into_verified_parts()` moves
that syntax and independently verified executable SSA without cloning either.
The SSA has program-owned distinct slot/value/block/region identities, scalar
block arguments at joins and loop backedges, memory tokens for effects and
references, typed instructions and explicit region calls. Candidate IR can be
mutated for transformations, but execution accepts only `VerifiedProgram` after
its ownership, dominance, interfaces, aliases, effects, CFG, and budgets pass.

Fixed arrays and bit registers support proven partial initialization; uncertain
indices require the relevant subtree to be initialized. Proven overlapping
mutable references are rejected statically, and unresolved dynamic overlap is
checked at execution. Quantum declarations are global. Their allocations hoist
to the entry block without making names visible before their declarations.
Named and indexed aliases are supported; unsupported alias forms are capability
errors in the selected simulator profile.

`semantic::builder` offers `Expr<T>` and `Local<T>` with sealed `Bool`, `Int<W>`,
`Uint<W>`, `Bit<W>`, `Angle<W>`, and `Float<W>` markers, plus distinct quantum
references. Constructors check widths and builder ownership. Arithmetic,
assignment, conditions, and registry gates construct the same syntax admitted
by the text frontend. Hygienic declaration names preserve handle identity across
nested scopes. `finish()` applies the shared semantic admission and budgets.

`TypedModule::retained_bytes()` and `VerifiedProgram::retained_bytes()` count
inline records, nested vector capacities, string capacities, and boxed syntax
with checked arithmetic. Allocator bookkeeping, externally owned source
snapshots, and host capture values are excluded explicitly and must be counted
by the owning planning boundary. Compilation checks syntax storage before
recursive lowering and verifier working storage before graph analyses.

`Block::quantum_dag()` preserves every gate/call/measurement/reset/barrier
occurrence and records conservative storage-root dependencies independently of
classical CFG backedges. Pure arithmetic remains potentially trapping, so
transformations must preserve overflow, division, conversion, and bounds errors.

`ScalarType::can_implicitly_cast_to` distinguishes standard Boolean/integer/float
conversions from explicit Bit and Angle reinterpretations. Angle precision
changes and Float-to-Angle assignments are admitted; Bit copies require matching
widths. Registry and user gate parameters share real-value admission, with Angle
values interpreted as radians and Bit values requiring an explicit cast.

`ssa::optimization::optimize(VerifiedProgram, OptimizationLimits)` consumes its
input without cloning the complete program. A monotone constant lattice follows
executable edges and merges scalar block arguments, including loop-carried
values. Successful scalar evaluations fold exactly; failed evaluations retain
their runtime instruction and source span. Constant branches simplify and
unreachable blocks are removed. Common scalar expressions are reused within
blocks only after the first evaluation, preserving its potential trap. Dead
computation removal is limited to proven nontrapping instructions; memory and
quantum effects remain ordered. Floating constant identity includes the sign of
zero, and no approximate numeric identities are applied. Work, iteration, and
storage budgets bound optimization. Every returned program passes independent
verification; failure exposes no partially transformed candidate. The outer
program wrapper retains frozen export syntax, source snapshots, and host captures.
