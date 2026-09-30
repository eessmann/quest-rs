# Parameters, captures, and subroutines

## Captures are evaluated once

A `${ ... }` expression captures a Rust `f64` or an explicit exact `Angle` when `circuit!` constructs the program. It is evaluated once per lexical capture, even if its gate occurs in a runtime loop or a called region. `f64` captures remain finite floating values. `${Angle::pi(1,4)?}` retains the exact mathematical target alongside its native floating realization; QASM `pi/4` retains ordinary floating language semantics. A capture is not a callback into Rust during interpretation.

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:captures_once}}
```

This test expects one push into the Rust vector and four quantum-loop iterations. The range `[0:3]` includes both endpoints. A `for` loop's range expressions are evaluated on entry; a `while` condition is evaluated on every iteration.

OpenQASM `input` declarations instead name classical values supplied through `RunInputs` for each execution. `output` declarations become named entries in `RunOutput`. These values have language types and are checked at the runtime boundary. Neither mechanism changes the program's quantum register interface.

## `gate` versus `def`

A `gate` definition has real-valued gate parameters and quantum operands. Its body must remain unitary: calls to other admitted gates and global phase are supported; measurement, reset, classical side effects, or an effectful subroutine do not become valid because they are hidden inside a definition. Definitions can be referenced before their source declaration. Recursive call graphs are rejected.

A `def` subroutine has typed classical and quantum arguments, may return a typed classical value, and may perform effects permitted by its interface. Control flow must supply a return on every reachable path when a return value is required. Qubits are allocated only at global module scope; quantum arguments are references to those existing qubits.

## Arrays and references

Mutable arrays are passed as references, so a subroutine can update the caller's storage. A readonly formal grants observation without mutation. The checker tracks initialization and rejects proven overlapping mutable references; unresolved dynamic overlap is checked against actual indices at execution.

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:array_arguments}}
```

Reading an uninitialized scalar or array element is invalid. Assignment to a statically known element records that element's initialization; uncertain dynamic writes do not prove every element initialized. Aliases created with `let` retain the original storage identity and mutability. Unsupported range/concatenation reference forms are capability errors rather than a different interpretation of the source.
