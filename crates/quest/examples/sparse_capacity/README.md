# Sparse matching scaling and capacity experiment

This Linux CPU/MPI example creates disjoint COO input shards directly, constructs
and persists their matching encoding, loads local resources, prepares native
execution, and repeats controlled forward/adjoint applications. It needs the
`mpi,qsvt-io` features, serial HDF5, and a matching MPI/SUBCOMM-enabled QuEST.
All Rust in the example is checked with `forbid(unsafe_code)`.

## Explicit scaling mode

```text
sparse_capacity --scaling OUTPUT N REPETITIONS MANAGED_RANK_BYTES MANAGED_NODE_BYTES NODES RANKS_PER_NODE THREADS STACK_BYTES
```

Every argument is required. `OUTPUT` must be a fresh, existing shared directory.
`N` is a power of two, at least 16; repetitions are 1–8. The requested placement
must equal MPI's actual shared-memory topology, with at most 32 ranks in total.
Managed rank budgets multiplied by actual ranks per node must fit the managed
node budget. Source buffers, producer work, retained resources, native state and
routing retain their existing independent admission checks.

For a local two-rank example, build against the installed MPI and use its launcher:

```sh
export QUEST_ROOT=/path/to/quest
export MPICC=/path/to/matching/mpi/bin/mpicc
cargo build --locked --release -p quest-rs --features mpi,qsvt-io --example sparse_capacity
export OMP_NUM_THREADS=2 OMP_PLACES=cores OMP_PROC_BIND=close OMP_DYNAMIC=FALSE
export OMP_STACKSIZE=8388608B
mkdir /path/to/new-output
mpiexec -n 2 target/release/examples/sparse_capacity --scaling \
  /path/to/new-output 64 3 134217728 268435456 1 2 2 8388608
```

Use the launcher's documented core-binding options. The same positional contract
works with `srun` inside a scheduler allocation. On a one-rank-per-node run,
declare the actual node count and `RANKS_PER_NODE=1`. No example mode sets process
limits, changes the scheduler, or derives an artificial process limit from a
cgroup value.

Scaling mode observes the actual soft and hard `RLIMIT_AS` entries through
`/proc/self/limits`. Each is either `{"kind":"finite","bytes":...}` or
`{"kind":"unlimited"}`. A finite soft limit still must admit the checked sum of
MPI baseline address space, managed rank budget, and `(THREADS-1)*STACK_BYTES`.
Unlimited is an observation, not a promise that allocations will succeed.

Each `rank-N.json` has schema 6, `evidence_kind="scaling-only"` and
`capacity_closed=false`, even if a finite process limit was observed. It retains
the ordinary per-stage timings, communication counters, input hashes, local
state counts, native array payloads, norm and bounded amplitude sample error.
The receipt distinguishes:

- `model_rank_budget_bytes` and `model_node_budget_bytes`: caller-supplied,
  application-managed admission budgets, not OS enforcement.
- `modeled_rank_envelope_bytes`: baseline address space plus that rank's managed
  budget and configured worker stacks.
- `modeled_node_rank_envelope_bytes`: sum of those envelopes over the actual
  MPI ranks on the same shared-memory node. It includes baselines and stacks
  beyond the managed node budget; it is neither a measured peak nor a node cap.
- `process_address_space_limits_before` and `process_address_space_limits_after`:
  the actual process observations, preserved without replacing unlimited values.
- `node_memory_sampling`: sampled simultaneous sums for known MPI rank PIDs
  only. Launchers, helpers, filesystem cache and other node residents are outside
  this measurement; a sampled maximum can miss an instantaneous peak.

Whole-node enforcement, whole-node peak and persistence/load wire bytes remain
null; HugeTLB enforcement coverage is unmeasured. Native OpenMP team size is also
unmeasured: requesting threads and observing process threads does not establish
the number actively used by every kernel. These runs support scaling comparisons
only. They cannot close the separate requirement for input larger than each
node's enforced capacity.

## Existing finite-cap mode

The original CLI is unchanged:

```text
sparse_capacity OUTPUT N REPETITIONS AS_CAP MANAGED_RANK_BYTES MANAGED_NODE_BYTES [NODES RANKS_PER_NODE [THREADS [STACK_BYTES]]]
```

It requires equal, finite observed soft/hard `RLIMIT_AS` values equal to `AS_CAP`;
unlimited or mismatched limits are rejected. Its receipt schemas and
`process_address_space_cap_bytes` field remain unchanged. Placement-aware finite
receipts remain schema 5. Existing capacity evidence still needs its separate
whole-node enforcement and input-size checks; a finite rank limit alone is
insufficient.

## Focused verification

```sh
cargo test --locked -p quest-rs --all-features --example sparse_capacity
cargo clippy --locked -p quest-rs --all-features --example sparse_capacity -- -D warnings
```

Admission tests cover finite/unlimited limits, legacy mismatches, finite soft
limits with unlimited hard limits, overflow, placement/budget rejection, and the
mandatory scaling-only receipt status. Actual MPI runs additionally verify the
observed topology, unchanged limits, direct shard creation and the matching
roundtrip. These small runs are not oversized-input capacity evidence.
