# quest-circuit

Public construction and compilation facade for the common QuEST program model.
It does not discover, link or initialize QuEST. `quest-language` owns semantic
values, finite quantum regions, structured SSA and verification; `quest-compile`
owns passes and compiled artifacts; `quest` owns native preparation and execution.

Text import, `circuit!`, `circuit_file!` and `ProgramBuilder` converge on:

```text
Program<Constructed> -> verify -> Program<Verified>
                    -> lower -> Program<Lowered>
                    -> plan -> Program<Executable>
```

The macro emits a checked template cached once per expansion. Each invocation
evaluates its Rust captures once in source order. `${Angle::pi(1,4)?}` preserves
an explicit exact target. `${0.25_f64}` and ordinary QASM `pi/4` remain floating
language values. There is no legacy macro or alternative executable lifecycle.

`ProgramBuilder` provides typed scalar expressions, inputs/outputs, arrays and
reference calls, gate definitions, modifiers, measurement/reset, control flow,
and matrix/channel/oracle payloads. `QuantumRegionBuilder` builds finite
capabilities for symbolic construction and compiler passes. Original bindings
are checked before `Program::from_region` or `ProgramBuilder::region` embeds
these capabilities in the common program.

Import `quest_circuit::prelude::*` for finite compiler extension traits. Passes
retain full phase and signed controls, preserve source traps and effects, and
independently verify transformed publications. Numerical fusion changes rounding
and grants no exact cancellation authority. Optional candidate generators remain
separate from independent certification.

`export_source` retains original QASM syntax. `export_compiled` persists optimized
SSA, captures, exact targets, payloads and source evidence in a versioned digest
protected envelope. `load_compiled` applies caller limits and recreates checked
publications and dispatch recipes. Host-only payloads cannot silently become
QASM gate definitions.

Features: `macros` (default), `codespan-reporting`, `serde`, and optional `workers`.
See the [guide](../../docs/book/src/index.md) and [interface table](../../docs/book/src/interfaces.md).
