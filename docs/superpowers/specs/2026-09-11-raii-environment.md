# RAII-only QuEST environment lifecycle

Status: approved by the user on 2026-09-11. This supersedes the explicit
`Environment::close()` / `CloseError` recovery design in the
[2026-09-10 architecture](2026-09-10-quest-rust-design.md). Dated audits and
verification records continue to describe the versions they tested.

## Ownership and initialization

`Environment` is the unique, thread-confined owner of a native runtime whose
initialization can be entered at most once per process. Pure configuration
validation before native entry does not consume that attempt. The bridge owns
the shared lifecycle, including admission from safe low-level callers; the
facade adds no independent restartable state or `active` flag.

Registers and prepared programs borrow their environment. Native dense,
diagonal, channel, and structured-reset resources are destroyed before their
reservation accounting is released and before environment finalization.
Independent Rust matrix payloads, owned snapshots, and owned diagnostics can
escape that scope. `Environment` and borrowing resources remain `!Send`/`!Sync`.

## Automatic terminal cleanup

`Environment::Drop` unconditionally invokes the documentation-hidden low-level
`finalize_quest_env_on_drop() -> ()` helper. The facade exports no shutdown
method. The helper retains owner-thread and live-resource checks, catches
native exceptions, and never panics. Successful cleanup finalizes the native
runtime permanently; QuEST may own MPI, whose world model cannot restart after
finalization.

If cleanup cannot finish safely, QuEST is permanently retired while the process
continues. A one-way retirement latch rejects initialization and native
operation admission even if acquiring the lifecycle lock fails. Unsafe native
storage is retained rather than destroyed. This is terminal for QuEST use and
does not produce a recoverable environment.

The ordinary low-level `finalize_quest_env()` remains available for bridge
consumers and tests. Its preflight live-resource rejection keeps the runtime
active for low-level cleanup; an exception after native finalization starts is
terminal. Successful finalization remains idempotent. These semantics do not
extend to the facade's automatic cleanup, which must retire an ownerless runtime
after any failed cleanup.

If native initialization succeeds but the builder's environment query fails,
the builder invokes the same terminal helper before returning its original
error. A rejected initialization attempt never invokes cleanup, so a duplicate
builder cannot retire another caller's active environment.

## Migration and validation boundaries

Remove high-level `close()` calls and `CloseError` handling. End lexical scopes
to release resources and finalize the environment. Explicit resource drops
remain appropriate when deliberately releasing an allocation for reuse or
checking accounting, but are unnecessary shutdown boilerplate.

Lifecycle tests run in separate processes. Successful cleanup is established
by inactive bridge state together with an idempotently successful ordinary
finalization; inactivity alone also describes failed retirement. Guarded
retirement is exercised with an independently retained low-level handle.

No production fault-injection interface is added. Actual GPU/MPI failures
partway through finalization remain a coverage limitation. Forgotten resources
may remain allocated until process exit. Abort, forced termination, or
deliberately forgetting the environment can prevent RAII cleanup.
