# Typed builders and verified SSA

The typed Rust builder makes common category errors unrepresentable in its API. An integer local accepts an integer expression of its declared width; a loop requires a boolean expression; a quantum reference cannot initialize a classical local. Const width markers are still checked: unsupported widths do not become valid because they appear as Rust generic arguments.

```rust
{{#include ../../../crates/quest-compile/tests/tutorials.rs:typed_builder}}
```

The closure constructs a loop body once. It does not execute the quantum loop in Rust. Reads in its condition and body become language expressions referring to the same local; the interpreter evaluates them as execution proceeds. Handles carry builder ownership, and attempts to mix handles from separate builders return errors. Hygienic internal names preserve handle identity across lexical shadowing.

`ProgramBuilder::finish` uses the same semantic admission as parsed text. `TypedModule` and `VerifiedProgram` are published only after independent checking. A typed handle is useful early evidence, but it does not bypass independent validation of the completed program.

## Scalar SSA and effects

SSA assigns every classical value once. A branch join passes values as block arguments instead of mutating a hidden scalar slot. A loop header receives the initial scalar values from its entry edge and updated values from its back edge. These block arguments represent loop-carried state explicitly.

Arrays and aliases retain storage identities. Memory/effect token arguments order their reads and writes together with quantum effects. Measurement, reset, barriers, calls, and each quantum gate occurrence remain explicit instructions. Repeated gates are separate occurrences even when their operands are identical.

Terminating a block means installing its outgoing control-flow instruction. **Sealing** a block means declaring that all incoming edges are known. A loop can need a terminated entry block while its header remains unsealed until the back edge is built. Conflating these states loses predecessor information and can create incorrect joins.

## Independent verification

The verifier checks program-owned, distinct slot/value/block/region identities; region and block interfaces; edge argument count and types; dominance; terminators; complete sealed predecessor sets; returns; call signatures; slot/reference ownership; mutability; proven alias conflicts; resource bounds; and explicit effect chains. An SSA value from another program cannot pass solely because its numeric index matches.

Owned qubit allocation is hoisted into the entry block without making source declarations forward-visible. Qubit declarations in subroutines or nested blocks are rejected. Dynamic quantum and array indices remain executable operations with checked bounds. Unknown dynamic alias overlap is resolved at execution using actual storage locations.

`VerifiedProgram` exposes immutable IR for inspection and interpretation. Converting an untrusted IR into a verified program requires the verifier again. Optimizers publish transformed programs only after this independent verification succeeds.
