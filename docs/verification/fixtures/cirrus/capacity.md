# Original-input capacity campaign

Build `sparse_capacity` with `qsvt-io,mpi` and the same MPI ABI as native QuEST.
The Cirrus build must load the centrally maintained serial HDF5 module selected
by the build driver. The runtime does not build or download HDF5. Keep the binary,
Python fixtures, source shards and output directory on EPCCFS.

Within an exclusive Slurm allocation of eight nodes, one MPI rank per node and
288 OpenMP threads per rank:

```bash
python3 docs/verification/fixtures/cirrus/capacity.py \
  --launcher srun --nodes 8 --ranks-per-node 1 --threads 288 \
  --executable /path/to/sparse_capacity --output /path/to/new-capacity-run \
  --start-dimension 1024 --max-dimension 536870912 \
  --process-as-mib 8192 --model-rank-mib 4096 --timeout 3600
```

The coordinator requests `--exclusive --ntasks-per-node=1 --hint=nomultithread
--distribution=block:block --cpu-bind=cores`, with `--cpus-per-task` equal to the
requested thread count. Slurm mode rejects multiple MPI ranks per node. The
coordinator starts one `srun` step at a time and doubles the power-of-two
matrix dimension after each completed case. It stops after capacity closure,
the configured maximum dimension/case count, or the first failure. It requests
no Slurm `--mem` and imposes no localhost placement. For a local smoke test use
`--launcher /path/to/mpiexec --nodes 1 --ranks-per-node 2
--start-dimension 64 --max-dimension 64 --model-rank-mib 16`.
Single-node runs can validate the machinery but never close multi-node capacity.

Every rank writes only its owned columns directly to a new source shard, then
reads that shard through a fixed-size buffer into the production sparse producer.
Each nonzero is exactly 32 little-endian bytes: row and column as `u64`, then real
and imaginary coefficients as `f64`. The two nonzeros per column are nonzero;
there is no ordinal field, header, compression, zero padding or dense matrix.
The canonical original input therefore occupies exactly `64 * dimension` bytes.
SHA-256, file size and allocated-block checks bind the persisted input shards to
the successful native receipts. HDF5 encoding bytes are reported separately and
never contribute to the capacity threshold.

A capacity certificate requires all stages, controlled forward/adjoint native
execution and numerical validation to finish, all rank receipts to validate,
complete MPI shared-memory rank groups with distinct consistent processor names,
and canonical original input **strictly larger** than every participating node's
enforced envelope. A completed campaign may still report `capacity_closed=false`.
A failed case preserves its bounded launcher log, completed earlier cases and an
incomplete `completion.json`; construction or rank counts alone cannot close it.

The enforced node envelope is the sum of equal hard/soft `RLIMIT_AS` limits of its
participating ranks, verified inside each executable. This is a conservative
bound on aggregate rank-process address space, including MPI, shared-library
mappings, source buffers, state, encoding records, routing scratch, allocator
reservations and observer threads. MPI baseline plus the application allowance
and configured OpenMP worker-stack allowance must fit the process cap before
source generation. Each additional worker reserves the explicit `--omp-stack-mib`
allowance (default 8 MiB), with checked multiplication. This allowance is separate
from actual measured address space; opaque thread/runtime allocations also remain
inside the hard process cap. This is **not a physical-node
cgroup cap**: the launcher, coordinator, filesystem cache and unrelated processes
are outside the rank-process envelope. Managed resource accounting is reported
separately; real cap failures remain failures even if the managed model fits.

The MPI node leader samples `/proc` for exactly its shared-memory group's PIDs
every 20 milliseconds. The maximum sampled sum of rank RSS is an observation,
not a true instantaneous peak or unique physical-page count. The largest sampling
span is recorded. The sum of rank RSS high-water marks is a separate upper bound.
All observations include the real MPI/native execution process. The source-write
time, five pipeline stage times, each repeated roundtrip, producer payload and
native routing counters are preserved. Publication/load wire bytes remain
explicitly uninstrumented; recipe communication bounds are not measurements.

Limits derive from checked dimensions, rank-local record counts and configured
budgets. In-memory sorting retains only the local source shard, and every vector
is bounded by local records, rank count, qubit count or repetitions. The old 4096
dimension ceiling is gone. Removing that ceiling does not bypass the producer,
loader, native state or routing admission guards.

Local debug observations at dimensions 8192 and 16384 on four ranks confirmed
that larger dimensions and geometric progression execute. At dimension 16384,
rank zero reported 4,150,272 bytes of producer managed peak admission and
4,686,776 bytes of native live reservations for 8192 local nonzeros. These local
ratios are not an eight-node acceptance result. They suggest that current
producer/native memory admission can exceed the original coefficient bytes even
with eight nodes; increasing dimension alone may never satisfy the strict
capacity inequality. Retain this as an open capacity boundary, and use the actual
Cirrus receipts to decide whether any admitted cap/dimension pair can close it.

The launcher supervisor binds a fresh nonce to rank zero's actual Slurm job.step
identity. On failure it cancels only that step and checks `squeue` for its
disappearance within a fixed deadline. It never falls back to cancelling the
allocation. `<launcher.log>.supervision.json` preserves cancellation and scheduler
confirmation; a missing step identity or unsuccessful confirmation is explicitly
reported as unverified remote cleanup, and the campaign stops.

Focused validation:

```bash
python3 docs/verification/fixtures/cirrus/capacity_test.py
python3 docs/verification/fixtures/cirrus/capacity_supervision_test.py
python3 docs/verification/fixtures/sparse-capacity/test_run.py
```

The legacy six-argument example invocation and exact legacy receipt schema remain
supported for the previously recorded single-host evidence. Version-two receipts
record placement and source shards. The new driver adds thread/stack arguments and
requires version-three threading evidence. Both earlier formats remain readable
through their respective validator entry points.

## Threading evidence and boundaries

Slurm thread count defaults to `SLURM_CPUS_PER_TASK`, or 288 if no allocation value
is available; the local default is one thread. An explicit `--threads` overrides
that choice. The wrapper exports `OMP_NUM_THREADS`, `OMP_PLACES=cores`,
`OMP_PROC_BIND=close`, `OMP_DYNAMIC=FALSE`, an explicit `OMP_STACKSIZE`, and
`SRUN_CPUS_PER_TASK=SLURM_CPUS_PER_TASK` when allocated. The executable verifies the
requested OpenMP environment before MPI initialization and calls
`CollectiveEnvironmentBuilder::with_multithreading()` when threads exceed one.
Both native environment capability and actual register deployment flags must
agree with that request; merely setting an environment variable is insufficient.

Receipts contain the requested thread count, configured worker-stack reservation,
actual native threading flags, and Linux process thread counts before preparation,
after native plus-state initialization, and after execution. A process thread
count includes MPI and observation threads and is not an OpenMP team-size
measurement. The exact native OpenMP team size remains explicitly uninstrumented.

Native QuEST operations such as plus-state initialization, Hadamards and cloning
can use OpenMP. Source generation, sparse preprocessing, persistence loading, Rust
matching pair arithmetic, packet iteration and MPI routing remain serial. Manual
C++ indexed amplitude-copy loops are also serial. No sparse-kernel OpenMP speedup
is claimed. MPI communication remains on the owning thread; the memory sampler
only reads process counters.

Local native validation with two OpenMP threads passed for both one and two MPI
ranks. Both cases completed controlled forward/adjoint execution, verified native
multithreading flags, and remained capacity-open. The local multi-rank mode exists
for compatibility tests; it is not the Cirrus placement profile.
