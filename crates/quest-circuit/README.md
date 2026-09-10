# quest-circuit

Native-independent structured programs and ideal circuit graphs. This crate does
not discover, link, or initialize QuEST. The `quest` facade adds native preparation
and execution. The [guide](../../docs/book/src/index.md) provides an
[interface matrix](../../docs/book/src/interfaces.md) and tested tutorials.

## Structured frontend

The default `macros` feature exports primary **`circuit!`** and compiler-tracked
**`circuit_file!`**. They produce `StructuredProgram`, using the shared language
parser/admission/verifier pipeline. Text import with explicit include resolution
and canonical export lives in `quest-qasm`. The `language` reexport exposes typed
Rust builders, owned diagnostics, immutable sources, scalar values, SSA, and the
native-independent interpreter.

Supported structured features include scalar widths and casts, scoped
initialization, arrays, readonly/mutable references, nonrecursive `gate` and `def`
regions, runtime indices, gate broadcasting, modifiers, `if`/`switch`, inclusive
`for` ranges, `while`, loop exits, return, measurement, reset, and feedback.
Unsupported capabilities are diagnosed; the profile does not claim timed/pulse
hardware execution or arbitrary-width arithmetic.

Rust `${ ... }` captures evaluate once during construction and carry finite
binary64 values. Language `pi` is floating-point, integer `1/2` is zero, and stored
`angle` values wrap modulo a turn. `U` uses the OpenQASM 3.1 scalar phase
convention. Read [numeric semantics](../../docs/book/src/language.md) when
migrating old macro examples.

`StructuredProgram::verify` consumes the admitted program into independently
verified SSA. Lowering and planning retain frozen syntax for export. The explicit
`optimize_classical` pass folds constants, simplifies branches and removes proven
safe pure work, then verifies the result again. Potentially trapping evaluations
and all effects remain observable. `optimize_quantum` handles static scalar-place
windows within each SSA block, with guarded inverse cancellation and CNOT/Clifford+T
parity resynthesis. Dynamic operands, calls and effects end these windows; the
result is independently verified and retains original syntax and sources.

## Ideal builder and migration macro

`ProgramBuilder` allocates program-owned logical qubits and bits, preserving
ordered operands, signed controls, explicit effects, immutable definitions, and
fresh occurrence identities. `legacy_circuit!` retains the older static ideal DSL
for deliberate migration; it is not the primary structured macro under an alias.

```rust
use quest_circuit::{Angle, Gate, ProgramBuilder};
let mut builder = ProgramBuilder::new(1, 0)?;
let q = builder.qubit(0)?;
let theta = builder.parameter("theta")?;
builder.gate(Gate::Rx(Angle::parameter(theta)), &[q], &[])?;
let plan = builder.finish()?.bind(&[(theta, 0.25)])?.lower()?.plan()?;
# Ok::<(), quest_circuit::Error>(())
```

Exact rational angles and symbolic parameters remain distinct from numerical
angles. The DAG orders shared wires, classical hazards, barriers and stochastic
effects; scheduling is deterministic. Exact symbolic unitary circuits support
adjoint and coherent control. Global phase remains explicit: `Rz(2*pi) = -I`.

Explicit ideal passes include exact local rewrites, bounded Gaussian/PMH CNOT
synthesis, affine parity-phase folding, and bounded numerical matrix fusion after
binding. The ideal APIs cover exact symbolic and bound numerical representations beyond the
static scalar-place quantum windows available in structured SSA. Reports retain
rewrites and resource/cost evidence. Numerical fusion changes rounding and grants
no exact inverse or certified approximation guarantee.

Numerical operators own immutable faer matrices and act as `A|psi>` or
`A rho A†`. The first target is the least significant local matrix bit. Products
use sequential faer execution; raw matrix admission is distinct from exact
symbolic-unitary authority. General programs can retain measurement, reset,
classical conditions, numerical operators and complete Kraus channels.

## Optional features and checks

- `macros` (default): structured and migration macro frontends.
- `codespan-reporting`: rendering of owned source diagnostics.
- `serde`: serialization of owned diagnostics, source snapshots, typed
  occurrences, and interpreter context.
- `workers`: explicit-path optimizer client and exact certificate APIs, reexported
  as `optimizer` and `certified`; optional engines remain outside the trusted core.

```sh
cargo test -p quest-circuit --test tutorials --locked
cargo test -p quest-circuit --no-default-features --locked
cargo test -p quest-circuit --doc --locked
```

The [optimization chapter](../../docs/book/src/optimization.md) distinguishes
exact transformations, local synthesis certificates, scalar phase recovery, and
numerical fusion. Compiler budgets, runtime budgets and worker budgets are
separate checked contracts. Dated architecture and audit documents remain
historical evidence; the approved implementation plan records current progress.

`LanguageError::report` produces one owned diagnostic across structured compiler
stages. Errors that already carry a diagnostic retain its original stage and
snapshots. Rust macro occurrences without text use their compiler file, line, and
column as a location note; text-backed occurrences remain normal validated labels.
