# Matching execution using the native communication array

Distributed CPU matching execution now reuses the input register's native
communication array for routing output. It no longer allocates a second full
distributed register. One-rank execution retains its owned scratch register.
Native QuEST source remains unchanged at revision
`503552065045eaf89baba85e6cd6aad728525554`.

Local and GNU/Cray multi-host execution validate the same controlled matching
unitary. They do not close strict capacity acceptance. The earlier snapshot
`8e02c49a7acf70a84f0c55aa40caba647024090a8cd72ffd3267bb72d8d3925b`
contains the previous two-register implementation; its results cannot validate
this change.

## Ownership and execution contract

Before opening matching-label Hadamards, ranks collectively admit the CPU
statevector deployment and communication array. Checked C++ adapters verify
distribution, non-null disjoint arrays and representable byte/index ranges.
The private Rust staging owner then exclusively borrows the input register.
It copies the entire local input into the communication array, performs the
existing bounded routing, commits that output, and releases the borrow before
the closing Hadamards. No native pointers, general register references or gate
operations escape through the staging interface. Native array ownership and
addresses remain unchanged.

Both scalar and batched routers use this interface. Copying all amplitudes
preserves dummy labels, padding, spectator and inactive-control sectors.
Rotations and permutations still act on both successful and unsuccessful flag
branches. Indexed writes preflight every index and coefficient before writing;
duplicate indices retain ordered last-write behavior. Native lifecycle and
thread guards remain in force. Copying itself currently uses `std::copy_n`;
no OpenMP speedup is claimed for that adapter.

Preparation charges an owned scratch only at one rank. Its descriptor, routing
buffers and validation peaks remain admitted at every rank. The generic input
register's conservative four-array reservation is unchanged.
`PreparedMatching::scratch_deployment()` returns `None` when execution borrows
the input's communication array; the input deployment already counts that
array. Version 5 capacity receipts preserve this distinction and continue to
validate historical versions under their original contracts.

## Semantic and failure evidence

The allocation regression first failed against the previous implementation:
increasing register width from 9 to 13 qubits added 245,760, 122,880 and 61,440
reserved scratch bytes per rank at 2, 4 and 8 ranks respectively. It now retains
bounded descriptor resources without an additional distributed register. At
one rank, scratch admission remains width-dependent and rejects an oversized
request. Descriptor preparation does not stand in for input-state admission.

Portable whole-unitary comparisons pass on arbitrary complex states at
1/2/4/8 ranks and split communicators. Tests include local and distributed
matching-label targets, both external-control polarities, repeated scalar and
batched forward/adjoint applications, padding and inactive sectors. The 64-state
fixture compares every amplitude with an independently extracted portable
unitary; the 512-state fixture exercises additional target layouts. Existing
persisted preparation and weighted-LCU/transform consumers also pass.

A test-only admission fault on one rank rejects collectively at 2 and 4 ranks,
preserves every local amplitude and reservation, and permits subsequent valid
execution. A separate fault occurs after opening Hadamards, staging and the
first output write, with another rank waiting. Durable witnesses establish that
sequence and confirm there is no return after failure. Both captured-stderr and
suppressed-stderr runs terminate within their deadlines (0.573 and 0.537 seconds
in the focused local run). Stderr wording and launcher exit codes alone are not
the acceptance criteria. Post-mutation failure retains the fatal policy; this
does not promise communicator recovery.

## Memory and local geometry

For the controlled profile, the state dimension remains `D = 4*C*S`. On more
than one rank, two native arrays now contain `128*C*S/P` bytes per rank, compared
with `256*C*S/P` for two registers. These are actual native array payloads,
excluding allocator overhead, MPI internals, temporary workspace and thread
stacks. They are separate from process-memory observations and reservations.

At dimension 64 with two ranks, the native array payload falls from 16,384 to
8,192 bytes per rank. The conservative native reservation falls from 70,008 to
53,624 bytes. At one rank the scratch fallback preserves 16,384 payload bytes.
All runs execute three controlled forward/adjoint pairs.

Four-rank local geometry tests cover dimensions 8,192, 16,384 and 32,768 with
two requested OpenMP threads per rank, a 2 GiB hard address-space limit and a
128 MiB managed budget per rank. Shards are generated directly; independent
validation decodes all 114,688 canonical source records across these runs and
checks file hashes, ownership and values. The largest original input is 2 MiB,
all ranks share one physical host, and strict capacity remains open. The
[capacity evidence](data/2026-10-06-matching-buffer-reuse/capacity.json) records
phase measurements, native payloads and the limited three-point memory fits.

The first local geometry attempt timed out after 180 seconds. All four rank
masters had the same `Cpus_allowed_list=0,16` with the local launcher's default
affinity and OpenMP binding. A bounded diagnostic reproduced that overlap.
Changing only Hydra's binding to `-bind-to core:2` gave distinct rank core sets;
the identical diagnostic workload then completed in 1.16 seconds. The ordinary
capacity executable completed all three sizes with that recorded launcher.
Both failed attempts remain evidence. This diagnoses the local launch setup;
it is not evidence of a routing correction or a cluster performance improvement.

## Independent initializer correction

Auditing admission found a pre-existing issue in `init_pure_from_root`:
broadcast bytes, Rust bridge values and C++ complex conversion remain live
together on each rank. Its operation reservation now charges all three global
arrays. The native CPU endpoint copies the converted data directly into the
existing amplitude array; the audited path has no fourth global conversion.

An actual four-rank test with `2^20` amplitudes uses a 64 MiB budget, a 16 MiB
register reservation and a temporary 16 MiB external lease on rank 1. The old
two-array reservation incorrectly admitted initialization. The corrected
48 MiB operation reservation rejects on every rank without changing state or
accounting. Releasing the lease admits initialization; every resulting local
amplitude and the released reservation are checked. The seven existing
collective runtime tests also pass. This convenience initializer still needs
global input storage on each rank; scalable matching initialization uses local
shards instead.

## Acceptance boundaries

Current local verification passed all 1,704 all-feature Nextest tests (six
explicit skips), all 1,512 default tests (one skip), and all 65 all-feature and
55 default doctests (one ignored snippet per lane). Default workspace checking,
strict all-feature/all-target workspace Clippy, binding freshness, formatting
and whitespace checks passed. The previous source's unexplained installed-CMake
file-read failure did not recur; no causal fix for that failure is claimed.
The 72 current Python fixture checks and independent source/evidence reviews
also pass, including the later whole-node acceptance correction below.

An isolated [Linux Clang bridge lane](data/2026-10-06-matching-buffer-reuse/clang-bridge.json)
also passed all 50 `quest-sys` tests, both matching integration tests, the
four-rank root-initialization regression and strict scoped Clippy. Clang 22.1.8
compiled the CXX bridge against the existing GCC-built QuEST package and MPICH;
selected test binaries loaded GNU `libgomp`, with LLVM `libomp` absent. This
validates that mixed compiler configuration, not a fully Clang-built native
installation or Apple Silicon execution. No source change was needed.

With `QUEST_ROOT`, the matching `MPICC`/launcher and library paths configured,
the focused local regressions are runnable with:

```sh
cargo nextest run --locked --offline -p quest-sys --features mpi \
  --test communication_buffer --test-threads=1
cargo nextest run --locked --offline -p quest-rs --all-features \
  --test matching_collective --test collective_initialization_budget \
  --test-threads=1
```

The shared MPI supervisor launches their required rank counts. On Cirrus, one
allocation coordinator uses the maintained `test-rust.sh --mpi-step` adapter
and shared temporary storage; the parent test itself is not launched once per
rank. Capacity receipts record their separate launcher and affinity settings.

Source hashes and local test outcomes are recorded in the
[summary](data/2026-10-06-matching-buffer-reuse/summary.json). New-source cluster
execution must be read from separately identified records;
previous source receipts are not substituted for them.

The new immutable source snapshot is
`9adbef476110e8425bacee15882c006464a6d95b6a54ccb2269621e29af68233`
(2,559 files). Its Cirrus deployment passed verification before and after being
made read-only, and all code hashes in the local summary match its manifest.
Both focused GNU/Cray archived Nextest campaigns and all scaling runs completed:

| Compiler | Scoped build | Eight-node focused tests | Scaling jobs, 2 / 4 / 8 nodes |
| --- | --- | --- | --- |
| GNU | 537488 | 537489 | 537490 / 537491 / 537492 |
| Cray | 537493 | 537494 | 537615 / 537616 / 537617 |

These allocations were serialized after the earlier campaign and retained exclusive
nodes, one MPI rank per node, 288 requested OpenMP threads and central serial
HDF5. Builds use one node and eight CPUs. GNU's two-node scaling job uses `short`;
the other jobs use `lowpriority`. The first Cray two-node `short` submission was
rejected while both allowed short-QoS submission slots were occupied; its
replacement uses `lowpriority`. The original rejection stderr was not retained,
so the recorded queue-limit diagnosis is based on the subsequent live queue and
QoS query.

Each compiler's scoped build produced a hashed Nextest archive and capacity
executable. Runtime admitted the exact successful build-job receipt, archive,
source, profile and adapter hashes. Each lane passed 16 selected parent tests
and 63 MPI launches using shared extracted binaries and bounded step supervision.
This includes eight expected fatal child exits across both lanes; durable
semantic witnesses and termination deadlines passed, independently of their
Slurm exit status. The selected tests cover 1/2/4/8 ranks and split communicators,
plus a three-rank admission check. These are focused cluster tests, not a new
full-workspace Cirrus run. Ten adapter tests and a real local archived P1/P2 test
also validate the submission machinery.

The [cluster record](data/2026-10-06-matching-buffer-reuse/cluster.json) preserves
all six completed cases, 28 rank receipts, source/build/runtime identities,
phase times, logical communication and memory observations. Independent audits
decoded all 786,432 original coefficients and checked their ownership, values,
file hashes and manifests. Every case completed three controlled forward/adjoint
pairs; maximum sampled amplitude error was below `8.68e-19` and norms passed.

| Nodes | Previous native arrays per rank | New native arrays per rank | GNU median roundtrip, seconds | Cray median roundtrip, seconds |
| ---: | ---: | ---: | ---: | ---: |
| 2 | 16 MiB | 8 MiB | 0.317 | 0.328 |
| 4 | 8 MiB | 4 MiB | 0.454 | 0.471 |
| 8 | 4 MiB | 2 MiB | 0.952 | 1.041 |

The previous arrays come from the separately identified `8e02c49a` version 4
runs with identical stored inputs. The reduction concerns actual native arrays,
not total process memory. Fixed-workload roundtrip time increases with node
count; these measurements do not establish a scaling speedup. Source generation,
loading, coefficient arithmetic and routing remain serial within each rank;
native multithreading is enabled, with 288 threads requested and actual team
sizes unmeasured.

These fixed scaling runs use dimension 65,536, three roundtrips and an 8 GiB
address-space limit per MPI process. Original input is only 4 MiB. Such a limit
does not also cap coordinator, launcher or auxiliary processes on the node.
The separately recorded [acceptance correction](2026-10-06-sparse-capacity-followup.md#acceptance-correction-after-the-frozen-snapshots)
therefore keeps strict closure unavailable without verified whole-node
enforcement. That correction postdates this immutable snapshot; its fixed
small-input runs already remain below the rank limits and cannot close capacity.
Later reporting and separately identified corrections do not alter the frozen
tree.

The later [two-node enforcement diagnostic](2026-10-06-node-enforcement.md)
observed a site job `memory.max` of 780,945,850,368 bytes per node, without
writable delegation inside the inspected Slurm hierarchy. Both MPI ranks used
16 MiB of HugeTLB memory; its coverage by a job-level memory cap remains
unestablished. This read-only diagnostic neither installed a smaller cap nor
tested limit enforcement, and does not change the scaling receipts' status.

The native array reduction is necessary progress toward the
[controlled-profile capacity bound](../research/distributed-capacity-memory.md).
The current generic reservation alone still equals the complete fully populated
COO32 input at eight ranks. Producer peaks, resident coefficients and native
workspace require independent admission. A subsequent operation-specific policy
must justify any reduction in that reservation. Completed multi-host execution
with original input exceeding every enforced node cap, Apple Silicon execution
and loader-isolated Cirrus consumers remain open acceptance gates.
