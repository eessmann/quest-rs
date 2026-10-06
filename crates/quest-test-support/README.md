# MPI test supervision

This unpublished development crate runs one test coordinator outside MPI and
launches each child with an argument vector. It replaces platform-specific
`timeout` commands. Tests retain their own numerical and failure assertions.
The crate forbids unsafe Rust. Unix pipe configuration borrows live descriptors
through `rustix`; process-group termination uses its typed, safe process API.
The child and its pipes remain owned by the supervisor's RAII lifetime.

```rust,ignore
use quest_test_support::mpi::{MpiTest, assert_rank_count};
use std::time::Duration;

// Coordinator branch; current_exe() is the default child executable.
let result = MpiTest::new(4, Duration::from_secs(30))?
    .args(["--exact", "distributed_case", "--nocapture"])
    .env("MY_MPI_CASE_CHILD", "1")
    .output()?;
assert!(result.status.success());

// Child branch, after initializing the admitted MPI runtime.
assert_rank_count(communicator.size()?)?;
```

`QUEST_MPI_LAUNCHER=local` selects `mpiexec`; `slurm` selects `srun` inside an
existing allocation. `QUEST_MPI_LAUNCHER_EXECUTABLE` overrides the executable,
and `QUEST_MPI_LAUNCHER_ARGS` is a JSON array of extra arguments. Shell strings
are never evaluated. MPI rank environment markers reject nested launches.
The [Cirrus coordinator](../../docs/verification/fixtures/cirrus/test-rust.sh)
sets one process per node and physical-core placement for each requested count.
Set `TMPDIR` to shared storage when witnesses or test inputs cross nodes.

The supervisor concurrently drains stdout and stderr, retaining at most one
MiB each by default and reporting truncation. `status()` forwards retained
output even on success; `output()` leaves assertions to the caller. A deadline
also covers descendants holding output pipes after the launcher has exited.

Slurm rank zero emits a unique launch token and its job/step identity. On timeout
only that owned step may be cancelled. Missing identity never triggers a
whole-allocation cancellation. The receipt distinguishes a successful `scancel`
request from `termination_confirmed`, which requires a bounded successful
`squeue` query no longer listing the step. Unavailable scheduler tools or a
remaining step leave termination unconfirmed. Cancellation and scheduler checks
add bounded cleanup time beyond the requested execution deadline.

A launcher failure or nonzero exit does not prove the intended fatal path ran.
Failure fixtures write and synchronize durable witnesses before triggering the
fault, then assert the witnesses, absence of return, and absence of timeout.
