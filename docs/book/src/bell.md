# Bell states and native execution

A Hadamard on qubit 0 followed by a controlled X produces `( |00⟩ + |11⟩ ) / √2`. The example constructs a structured program, prepares it in an environment, allocates a two-qubit state vector, and runs it with empty classical inputs.

The shared example module imports the facade and uses one fallible result type:

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:native_prelude}}
```

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:bell}}
```

The return values are probabilities for basis states 0 and 3, each approximately 0.5. Register qubit 0 is the least significant computational-basis bit. For multi-target matrices, the first target is also the least significant local matrix bit; reversing targets changes the operation.

`prepare_structured` performs the checked compilation/planning stages needed by the facade. Keep a prepared program when executing the same structure repeatedly. A new state vector starts in the zero state. Running a prepared program on an existing register acts on its current state.

The pure frontend Bell example can be checked without native libraries. Native execution requires the installed QuEST configuration described in [validation](validation.md). Registers and prepared objects borrow their environment, so their handles must be dropped before explicit environment closure.
