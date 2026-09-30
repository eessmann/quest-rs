# Runtime limits, effects, and ownership

Preparation resolves static gate parameters, operands and signed controls once into reusable native dispatch records. Execution interprets verified CFG blocks, passes block arguments, evaluates typed classical instructions, and issues checked gate/measurement/reset/barrier requests to a quantum backend. The pure `QuantumBackend` trait also supports frontend tests without loading QuEST. The native facade connects these requests to environment-bound registers.

## Separate budgets

| Boundary | Representative limits |
|---|---|
| Text and includes | Source bytes, tokens, nesting, include depth/count |
| Semantic admission and SSA | Nodes, blocks, slots, qubits, owned storage |
| Optimization | Work, rounds, retained storage, output IR limits |
| Native planning/preparation | Register, host/device, scratch and cache resources |
| Interpretation | Steps, call frames, classical storage |
| Worker process | Input/output bytes, elapsed time, bounded engine problem size |

Limits are checked quantities, not timing promises. Checked retained-size accounting includes owned strings and nested collections; a small top-level struct does not stand in for all of its heap storage. Allocation and backend failures can still occur within an admitted budget.

`InterpreterLimits` defaults to 10,000,000 steps, 64 call frames, and 64 MiB of classical storage; callers can replace these limits for a run. Native storage is capped by the environment's remaining budget, and a program without calls needs only one frame. The worker client separately defaults to 30 seconds, 512 MiB process memory, and 64 KiB output. If the platform cannot enforce its worker contract, it returns a capability error. A small iteration count in source can be a useful algorithm bound, but it does not replace interpreter step admission: an invoked body may itself contain substantial work.

## Partial execution is observable

Compilation and preparation do not execute the quantum algorithm. Once a run begins, earlier quantum operations are not rolled back if a later division, index, budget check, or backend request fails. Runtime errors retain the current instruction/source context, call context, and completed quantum prefix. Inspect the error before deciding whether to reuse or reinitialize a register.

Potentially trapping classical computations are observable too. An optimizer cannot delete an unused division merely because its result has no users. Short-circuit logical operators evaluate only the selected operand; branch simplification must preserve that behavior.

## Native lifetime and numerical policy

`Environment` uniquely owns a runtime whose native initialization may be entered
at most once per process. It is confined to its creating thread and implements
neither `Send` nor `Sync`. Registers and `PreparedProgram` borrow it, so their native handles are destroyed
before its scope ends. `Drop` then finalizes the runtime automatically, including
on an early `?` return or during Rust unwinding. The facade has no explicit
shutdown method.

The minimal example returns an owned snapshot from the environment's scope:

```rust,no_run
{{#include ../../../crates/quest/examples/minimal.rs}}
```

This is a process lifetime, not a restartable session. QuEST may own an MPI world;
MPI cannot be initialized again after its world is finalized. Both facade and
low-level initialization reject attempts after the environment is dropped.
Pure configuration validation before entering native initialization does not
consume the attempt. A rejected duplicate initialization leaves the existing
owner usable.

`Drop` never panics. If native finalization fails or an independently retained
low-level handle prevents safe cleanup, the bridge permanently retires QuEST.
The process continues, but subsequent native operations and initialization fail.
Native storage that cannot safely be destroyed remains allocated until process
exit. Retirement does not report successful finalization or return a recoverable
environment. Deliberately forgetting the environment, aborting, or forcibly
terminating the process can prevent RAII cleanup altogether.

Native dense/diagonal matrices and channels cached in prepared programs belong
to this runtime. Pure Rust `NumericalOperator` matrix payloads, compiled circuit
descriptions, owned faer snapshots, and owned diagnostics have independent
lifetimes and may outlive the environment.

The default environment selects CPU execution without native multithreading. GPU execution and native threading require explicit policy choices. Preparation constructs caches transactionally; run checks native numerical admission before mutation. Direct low-level bridge calls do not enter the facade's memory accounting automatically.

State vectors and density matrices have different effect semantics. Measurement and reset use trajectories for state vectors; density reset applies the complete reset channel. General channels require density registers. `PreparedProgram::sample_zeroed(shots, seeds, inputs)` seeds QuEST's process-wide RNG and initializes a zero state per shot. Each shot returns an ordinary `RunOutput`, preserving named outputs and execution counts. The native tutorials avoid assuming a particular random measurement sequence.
