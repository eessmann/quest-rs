# Accurate CLI trace and lifecycle boundaries

The CLI outer trace is now `Stage::ApplicationDispatch` / `application_dispatch`.
It starts after runtime and worker admission and ends before owner teardown.
The general `EndToEnd` stage remains available for callers that actually wrap an
entire operation. CLI help and README distinguish these scopes. Failed runtime
or worker preflight has no trace; failures/retries inside admitted dispatch do.

The final JSON total is written only after local pool ownership ends, or after
distributed pool, communicator and MPI runtime ownership all end. Separate
`worker_pool_teardown` and `mpi_and_worker_teardown` measurements cover those
scope exits. Final totals include optional trace serialization/file writing,
but exclude final JSON printing. No numerical stage or retry timing changed.

The new trace regression failed against the old `end_to_end` event and passes
after the rename. Three focused no-default-feature trace tests pass, covering
successful event names, failed preflight without a file, and failed admitted
dispatch with a trace. Lifecycle assertions check that totals include the
separately reported admission/teardown durations. Strict all-target Clippy passes
with MPI/Rayon and without default features; touched-file formatting passes.

The two/four-rank embedded/overlap lifecycle integration rerun passes (0.51 s),
including admission/teardown timing assertions and prior scientific/count
regressions. Automatic escalation review initially timed out before launching
the command; its permitted retry was approved and completed successfully.
