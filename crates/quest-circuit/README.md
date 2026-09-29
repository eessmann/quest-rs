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
builder.gate(Gate::Rx(Angle::parameter(theta)?), &[q], &[])?;
let plan = builder.finish()?.bind(&[(theta, 0.25)])?.plan()?;
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

`Optimizer::search_with_workers` uses a bounded beam. Its exact MITM generator
uses depth 6 for both one- and two-qubit windows; the standalone one-qubit
adapter permits depth 12. A `Complete` beam status means its configured generators
and caps completed, not that the full standalone MITM search space was exhausted.
Structured search reports worker search as a skipped stage.

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
separate checked contracts. See the [contributor guide](../../CONTRIBUTING.md)
for development checks and the [verification index](../../docs/verification/README.md)
for tested configurations and results.

`LanguageError::report` produces one owned diagnostic across structured compiler
stages. Errors that already carry a diagnostic retain its original stage and
snapshots. Rust macro occurrences without text use their compiler file, line, and
column as a location note; text-backed occurrences remain normal validated labels.

## Shared numerical oracles

Freeze a bound coherent body through the consuming oracle builder. Matrix
admission is explicit and applies to each numerical operator separately; it does
not certify the unitarity error of the composed fragment.

```rust
use quest_circuit::{Gate, OracleFragment, ProgramBuilder};
let mut body = ProgramBuilder::new(2, 0)?;
body.gate(Gate::H, &[body.qubit(0)?], &[])?;
let fragment = OracleFragment::builder(body.finish()?.bind(&[])?)
    .matrix_tolerance(1e-12)?
    .build()?;
let mut program = ProgramBuilder::new(3, 0)?;
let targets = [program.qubit(2)?, program.qubit(0)?];
program.oracle(&fragment, &targets, &[])?;
program.oracle(&fragment.adjoint(), &targets, &[])?;
let plan = program.finish()?.bind(&[])?.plan()?;
# Ok::<(), quest_circuit::Error>(())
```

Each invocation remains an ordered `Operation::Oracle` occurrence in the plan.
Cloned fragments and adjoint views share immutable body storage. Local target
zero remains the least-significant basis bit, including nonsorted argument lists.
`decompose` remaps one body layer, preserving nested oracle calls and controlled
global phase. It is intended for preparation, where the backend can cache each
shared body; it does not flatten the retained plan during lowering.

The Rust frontend accepts `oracle block[2] = ${fragment};` and invokes it using
`block q[2], q[0];` or `adjoint @ ctrl @ block q[1], q[2], q[0];`. Captures execute
once during construction, even when calls occur inside loops. Shared language
SSA holds only the local capture identity and qubit signature; the circuit plan
owns the fragment payload. A numerical oracle has an adjoint but no exact inverse
capability. `inv` and negative powers require exact semantics and are rejected for
numerical oracle calls, including indirectly invoked calls at execution preflight.

`OracleFragment::export_qasm` exports an explicit portable built-in decomposition.
Numerical matrices and unresolved Rust captures return a structured unsupported
capability diagnostic. A staged QSVT transform is not an oracle fragment and
cannot be supplied as an oracle capture implicitly.
