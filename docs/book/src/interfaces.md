# Choose an interface

`circuit!` is the structured frontend. Older examples that use this name for the ideal static DSL must migrate to `legacy_circuit!` or to `ProgramBuilder`. The names identify different semantics; changing a macro name should be a deliberate migration.

| Feature | Text import | `circuit!` / `circuit_file!` | Typed `Builder` | Ideal `ProgramBuilder` | Structured export |
|---|---|---|---|---|---|
| Runtime scalar expressions and widths | Yes | Yes | Typed scalar subset | Host bindings and exact angles | Preserved |
| `if`, `switch`, `for`, `while`, exits | Yes | Yes | `if_else`, `while_loop` | Finite circuit with conditions | Preserved |
| Gates and nonrecursive `def` calls | Yes | Yes | Primitive gate calls | Explicit circuit definitions/calls | Preserved |
| Classical arrays and reference arguments | Yes | Yes | No convenience API yet | No structured array language | Preserved |
| Measurement, reset, feedback | Yes | Yes | No convenience API yet | Explicit effect operations | Preserved |
| Rust interpolation evaluated once | No | `circuit!`: `${ ... }` | Rust computes builder inputs | Rust computes builder inputs | No Rust expression serialization |
| Explicit include resolver | Yes | `circuit_file!` tracks files | Supply admitted syntax separately | Not an OpenQASM include frontend | Include edges retained on imported root |
| Exact rational multiples of π | Ordinary numeric language values | Ordinary numeric language values | Ordinary numeric language values | `Angle::pi` and symbolic parameters | Original structured arithmetic |
| Runtime loops in native execution | Yes | Yes | Yes | No runtime loop IR | Preserved |
| Exact CNOT/parity optimizers | Static scalar-place windows | Static scalar-place windows | Static scalar-place windows | Yes, explicit passes | Does not emit rewritten SSA |
| Numerical matrix fusion | Separate ideal API | Separate ideal API | Separate ideal API | After binding | No fabricated gate definition |
| Certified external workers | Separate ideal/certificate APIs | Separate ideal/certificate APIs | Separate ideal/certificate APIs | Optional `workers` feature | Certificates are separate artifacts |

The typed Rust builder provides useful category checking: `Expr<Bool>`, `Expr<Int<32>>`, `Local<T>`, and quantum references are distinct Rust types. Its convenience surface is intentionally smaller than the admitted text language. Program identity, scope, dimensions, initialization, and resource bounds still require runtime checks.

Pure frontend work uses `quest-circuit` and its `language` reexport without loading QuEST. The `quest` facade reexports those APIs and adds native environments, registers, preparation, and execution. `quest-qasm` owns explicit include resolution and canonical structured text export.

```rust
{{#include ../../../crates/quest-circuit/tests/tutorials.rs:macro_bell}}
```
