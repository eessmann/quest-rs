# Runtime limits, effects, and ownership

Structured execution interprets verified CFG blocks, passes block arguments, evaluates typed classical instructions, and issues checked gate/measurement/reset/barrier requests to a quantum backend. The pure `QuantumBackend` trait also supports frontend tests without loading QuEST. The native facade connects these requests to environment-bound registers.

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

`InterpreterLimits` defaults to 10,000,000 steps, 64 call frames, and 64 MiB of classical storage; callers can replace these limits for a run. The worker client separately defaults to 30 seconds, 512 MiB process memory, and 64 KiB output. If the platform cannot enforce its worker contract, it returns a capability error. A small iteration count in source can be a useful algorithm bound, but it does not replace interpreter step admission: an invoked body may itself contain substantial work.

## Partial execution is observable

Compilation and preparation do not execute the quantum algorithm. Once a run begins, earlier quantum operations are not rolled back if a later division, index, budget check, or backend request fails. Runtime errors retain the current instruction/source context, call context, and completed quantum prefix. Inspect the error before deciding whether to reuse or reinitialize a register.

Potentially trapping classical computations are observable too. An optimizer cannot delete an unused division merely because its result has no users. Short-circuit logical operators evaluate only the selected operand; branch simplification must preserve that behavior.

## Native lifetime and numerical policy

An environment is unique per process and confined to its owner thread. Registers and prepared programs borrow it. Drop these resources before calling `close`; failed explicit closure retains ownership so the caller can inspect the error. Native handle leaks still prevent finalization.

The default environment selects CPU execution without native multithreading. GPU execution and native threading require explicit policy choices. Preparation constructs caches transactionally; run checks native numerical admission before mutation. Direct low-level bridge calls do not enter the facade's memory accounting automatically.

State vectors and density matrices have different effect semantics. Measurement and reset use trajectories for state vectors; density reset applies the complete reset channel. General channels require density registers. Sampling APIs on ideal prepared programs explicitly seed QuEST's process-wide RNG and initialize a fresh zero state per shot. The native tutorials avoid assuming a particular random measurement sequence.
