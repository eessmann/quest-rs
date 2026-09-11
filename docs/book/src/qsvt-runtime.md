# Native QSVT and postselection

Enable facade feature `qsvt`. Numerical encodings and transforms are pure Rust
owners; native registers, prepared matrices and scratch borrow an environment.
The following example runs with `cargo run -p quest-rs --example qsvt --features
qsvt --locked`. It uses a complex scalar block so losing a conjugation or global
phase changes the result.

```rust,ignore
{{#include ../../../crates/quest/examples/qsvt.rs:construct}}
```

The transform retains input/output projectors, every oracle invocation and any
bridge projection with its continuation. A projected transform cannot be used
as a coherent oracle fragment. All numerical work is binary64; construction
residuals do not grant exact inverse cancellation or theorem premises.

```rust,ignore
{{#include ../../../crates/quest/examples/qsvt.rs:execute}}
```

Execution follows input projection, main circuit, optional bridge projection
and continuation, then output projection. It preserves subnormalized vectors
and reports absolute mass at each projection. `release()` leaves that state
unchanged. `condition()` consumes the exclusive result, rejects zero success
and normalizes the retained state. Copying the mass cannot authorize a later
normalization after another operation has changed the register.

An owning snapshot may escape the environment scope. Registers and prepared
resources cannot: QuEST finalization permanently retires its runtime, and an
MPI world that QuEST owns cannot be restarted. Native postselection currently
uses state vectors; coherent oracle bodies also support density execution.

```rust,ignore
{{#include ../../../crates/quest/examples/qsvt.rs:overlap}}
```

Hadamard preparation stores the reference/input superposition and three native
registers. Repeated observations reuse those registers and native matrices.
Real and imaginary observations retain the complex overlap phase and an
absolute probability ledger. The native weighted-sum primitive may allocate
small internal argument vectors; the API makes no claim of zero native heap
allocations during a run.

`admit()` and `prepare()` are separate owning stages for timing and budget
inspection. Admission includes lowering, projector construction and input
packing. Preparation materializes native storage. The Criterion
`qsvt_execution` benchmark separately measures construction, admission, native
preparation, repeated execution, conditioning alone, execution plus conditioning, and the combined
construction-through-execution path. Register initialization is benchmark
setup; mathematical synthesis and certification are separate QSP benchmarks.

For distributed execution, enable both Cargo feature `mpi` and a QuEST install
configured with MPI and SUBCOMM. The public `quest::collective` module is absent
otherwise. Cargo resolves optional dependencies before the native capability
probe: explicitly enabling `mpi` still builds rsmpi even if a serial QuEST
package makes the public API disappear. Leave that feature disabled for a
toolchain without MPI. rsmpi supplies the caller-owned `MPI_THREAD_MULTIPLE` universe;
communicators borrow it and prepared resources borrow their collective QuEST
environment. The application documents root-only synthesis and serial IO in
[QSVT applications](qsvt-applications.md). Distributed snapshots/full-state
output and distributed solve are outside the initial supported profile.
