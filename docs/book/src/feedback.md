# Measurement, teleportation, and feedback

Measurement produces a classical `bit`; conditions require a `bool`. The explicit `bool(result)` conversion makes that boundary visible. Measurement and reset are effects: they cannot disappear because their classical result appears unused.

## Teleportation

This example prepares a real input state with `ry(0.7)`, creates an entangled pair, performs Bell measurement, and applies corrections to qubit 2. Resetting the measured qubits isolates the recovered state on qubit 2. The returned fidelity is one within floating-point tolerance for either measurement outcome.

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:teleportation}}
```

`gate entangle` describes unitary quantum operations. The main program performs measurement and conditional correction. This separation also permits gate modifiers to have a well-defined adjoint.

## Feedback

A random measurement followed by a conditional X returns the qubit to zero. The output retains the original measurement result; the quantum state reflects the correction.

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:feedback}}
```

## Repeat until success

A runtime loop carries its success flag and attempt count through SSA block arguments. The tutorial deliberately bounds attempts at two. The first attempt succeeds randomly; the second prepares `|1⟩`, so the test can assert success without depending on a particular random stream.

```rust
{{#include ../../../crates/quest/examples/tutorials.rs:repeat_until_success}}
```

A real probabilistic algorithm may reach its attempt limit without success. Treat that as an algorithm result. Interpreter step exhaustion is a separate resource error, and quantum operations completed before the error remain observable.
