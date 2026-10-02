# quest-compile

Canonical native-independent construction and compilation crate for the common QuEST program model.
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

Import `quest_compile::prelude::*` for finite compiler extension traits. Passes
retain full phase and signed controls, preserve source traps and effects, and
independently verify transformed publications. Numerical fusion changes rounding
and grants no exact cancellation authority. Optional candidate generators remain
separate from independent certification.

`export_source` retains original QASM syntax. `export_compiled` persists optimized
SSA, captures, exact targets, payloads and source evidence in a versioned digest
protected envelope. `load_compiled` applies caller limits and recreates checked
publications and dispatch recipes. Host-only payloads cannot silently become
QASM gate definitions.

Features: `macros` and `synthesis` (default), `codespan-reporting`, `serde`, and optional `workers`. Native `synthesis` calls `quest-synthesis` directly and never enables process workers. `workers` independently enables the optional process client.

Compiled artifacts and optimizer messages use format version 3 with canonical decimal exact-number encodings. Frontend templates retain format version 2 because their representation is unchanged. Unsupported versions are rejected and must be rebuilt. Historical certificate records carry typed target, candidate, limits, identities and interfaces; loading independently recertifies their mathematical content. These local records never certify the current executable.

Finite region insertion uses checked semantic operations and SSA operand mapping. It preserves exact-angle captures, effects and original provenance without reconstructing gate syntax or adding placeholder scalar captures for oracle bodies.
See the [guide](../../docs/book/src/index.md) and [interface table](../../docs/book/src/interfaces.md).
