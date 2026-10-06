# Distributed matching execution contract

The reusable entry points are
`CollectiveEnvironment::prepare_matching` and
`CollectiveEnvironment::prepare_matching_transform`. They accept an owned
`MatchingShard`; the transform additionally accepts a compact `MatchingSchedule`.
Neither prepared object retains a complete sparse matrix or portable gate stream.

## Source preparation

A producer must supply a common `MatchingHeader` and disjoint owned
`MatchingColumn` records through `MatchingShard::from_parts`. Column ownership is
`source % communicator_size == rank`, including permutation-completion columns.
Untouched columns and dummy colors have implicit zero-success identity records.

The header binds dimensions, normalization, source identity, record count and a
commutative payload digest. Every coefficient rotation, complex phase, color,
source and destination participates in the digest. The digest detects accidental
source/payload disagreement; it is not cryptographic authentication. All ranks
must use the same complete source manifest. Replacing that manifest changes the
source and invalidates an earlier schedule's source binding.

`MatchingShard::from_encoding` is a convenience for bounded reference tests.
It begins with a complete matching encoding and is restricted to bounded
references. Production callers use `matching::preprocess::produce_matching`
or `produce_matching_with_store`, supplying COO records local from their creation.
Each input record includes a stable ordinal. Duplicate accumulation follows
ordinal order, with canonical zero removal after summation; arrival order and
rank count do not determine floating-point summation order. Local CSR iterators
can supply the same interface.

The producer uses synchronous bipartite edge-coloring rounds. Row and column
owners arbitrate a frozen round before either endpoint commits a color. Separate
limits bound probes, rounds, vertex degree, endpoint records, bytes and work.
Distributed path completion creates source-owned forward and destination-owned
reverse records. A high-degree vertex can still exceed a rank's admitted endpoint
storage. This is an explicit rejection, not permission to gather the source.

`quest-numerics::sparse_stream` owns numerical canonicalization and merge
semantics. `quest-qsvt-io::sparse_stream::FileRunStore` implements bounded external
merge storage. The filesystem format stays in the IO layer; the producer accepts
the store as a parameter and does not require a global CSR.

## Immutable persistence

`quest_qsvt_io::sharded_matching` stores serial-HDF5 files by logical source
bucket. Bucket identifiers are independent of producer rank identifiers. A
versioned JSON manifest records coverage, file SHA-256 and a separate canonical
semantic SHA-256. Frozen gate angles accompany coefficient rotations so loading
does not reconstruct a different portable unitary through another transcendental
evaluation. Completion-only columns and dummy labels preserve the same failure
branches as the native executor.

Files and manifests publish without replacing existing files. The coordinating
rank publishes the manifest only after the application agrees collectively that
all owners succeeded. It does not reread all remote files. Compatible execution
partitions assign complete logical buckets to ranks; incompatible bucket layouts
require an explicit streaming repartition and are rejected by the direct loader.

Opening a bucket hashes a bounded copy into a private disk snapshot, then validates
its fixed schema. External links, external raw storage, virtual datasets and
filters are rejected. Subsequent replay reads the admitted snapshot. Forward and
reverse record visits retain only one chunk. The resource-backed gate-stream adapter
uses those immutable resources for bounded portable forward/adjoint replay, with
its permutation-walk work separately admitted. It does not retain the full gate
stream. Native and portable consumers remain independently checked; record
iteration alone is not evidence of a unitary circuit.

Manifest admission counts the encoded input concurrently with decoded receipts
and strings. Chunk admission includes disk records, decoded records and transfer
buffers. The writer reserves a conservative fixed-schema allowance before each
HDF5 growth operation and audits actual logical file size after flush. This
reservation is not a filesystem quota. HDF5 metadata caches, allocator bookkeeping
and filesystem allocation remain measured capacity costs; process RSS and temporary
disk occupancy must be recorded in capacity campaigns.

## Native ownership and execution

Use one `MpiRuntime`, a world or split communicator, and a borrowing
`CollectiveEnvironment`. Admit the source and register layout before execution.
The prepared resource borrows the environment, owns its shard and reuses native
scratch and routing buffers. A compact schedule contains phases and source
metadata, not matrix entries.

Use `state_vector_local` for admission based on the actual local QuEST partition.
The existing `state_vector` factory keeps its conservative full-state accounting.
Initialize through bounded `write_local_amplitudes` chunks and inspect/reduce
observables from bounded `read_local_amplitudes` chunks. This path does not call
global state initialization or broadcast a dense isometry.

All ranks must enter collective operations in the same order. Local file parsing
and RHS generation failures must be agreed collectively before any rank proceeds
to native execution. Production preparation collectively compares common
semantic metadata and local admission outcomes, checks payload coverage and
permutation bijectivity, then allocates prepared resources. A rank-local
unexpected error or panic after execution begins uses the existing fatal MPI
abort boundary. This is job termination, not transactional recovery of the state.

The portable gate and native unitary act on all flag/color sectors, including
unsuccessful branches, spectators and inactive signed controls. Forward and
adjoint calls preserve the same operand mapping. CPU statevectors are admitted;
GPU and density-matrix execution of this fused route are explicitly rejected.

## Bounds and evidence

Each global batch contains at most 64 flag pairs. Each rank retains at most 128
routed amplitude records; each wire send/receive payload is at most 5,120 bytes.
A separate duplicated MPI context carries checked count/data exchanges. Native
buffer accesses occur only inside checked C++ adapters. Prepared vectors are
reserved fallibly and reused; permutation admission uses a bounded reserved
vector rather than allocating a tree node for every incoming record.

`last_statistics` reports batch sizes, indexed native accesses, coordination
calls and sent/received application bytes. Byte counts include count messages
and exclude MPI protocol overhead and other collective/native communication.
Timing, memory and traffic are different measurements.

Weighted native composition and its owning QSVT transform now expose
[cumulative routing receipts](prepared-routing-telemetry.md) across all selected
children and source queries. A completed receipt sums event counters and takes
maxima for peak batch sizes; reaching an inherited counter limit marks it as
inexact lower bounds. Each apply attempt clears the previous receipt before
admission. The [persisted-resource bridge](persisted-matching-preparation.md)
can transfer exclusive loaded resources into this native execution path.
Its ownership/overlap checks do not retroactively admit preceding file loading.
These focused component checks and the separately planned persisted inverse
experiment have distinct evidence; neither extends the existing local capacity
receipt into a multi-host claim.

The prepared schedule scans only the current rank's state partition. Candidate
flag pairs are looked up at coefficient owners and routed in bounded frames.
Owner batches remain serialized and use O(P) peer rounds per batch. Local scanning
removes replicated basis enumeration, but bounded memory does not establish
scalable runtime; comparative timings and multi-host capacity remain required.
The [acceptance record](../verification/2026-10-04-quest-cfd.md) includes actual
1/2/4/8-rank and split-communicator tests, whole-register portable differentials,
payload rejection and a native mid-execution failure test. It does not establish
multi-host scaling, real allocator exhaustion or capacity beyond node RAM.

The newer [capped local pipeline receipt](../verification/2026-10-05-sparse-capacity.md)
runs actual source-owned generation, immutable publication, drop/reload and
repeated controlled native forward/adjoint pairs at 1/2/4/8 ranks. Its
[public numeric artifact](../verification/data/2026-10-05-sparse-capacity/results.json)
preserves successful staged/tighter-cap cases, an insufficient address-space-cap
failure and strict independent receipt revalidation. Linux RLIMIT_AS caps each
rank's address space; the launcher is outside that cap. Managed rank/node
envelopes, RSS high-water observations and process caps are separate quantities.
This small one-host experiment does not establish real allocator-failure recovery,
simultaneous physical-node RSS limits, multihost scaling or actual large-count
transport. Application byte counters exclude uninstrumented persistence traffic
and MPI/native protocol work as described in the receipt.

The [streamed CFD history adapter](../../crates/quest-cfd/docs/distributed-history.md)
connects generated temporal rows directly to preprocessing and
[collective coherent RHS preparation](coherent-rhs-preparation.md). Stateless
Carleman and full-coordinate KvN sources retain charged complete physical inputs
rather than full lifted matrices or RHS arrays. KvN-specific execution is a
reviewed 1–2-rank bounded smoke; generic history tests cover 1/2/4/8/split and
controls/padding/adjoints. None of these checks proves physical convergence or
free coherent access to coefficients.

Executable usage is exercised in
[`matching_collective.rs`](../../crates/quest/tests/matching_collective.rs) and
[`qsvt_matching_transform.rs`](../../crates/quest/tests/qsvt_matching_transform.rs).
Their full-matrix/state references are deliberately small test oracles; they are
not part of the prepared runtime's retained storage.

## Native matrix limits

Distributed QuEST state storage does not distribute arbitrary dense gates.
`Register::admit_native_matrix` describes the actual deployment using
`MatrixRequest` and reports rank/node peak estimates. In the inspected QuEST
4.3.0 implementation, `CompMatr` stores the complete dense matrix on every rank.
A dense operation with `k` targets on an `n`-qubit statevector over `P` ranks
requires `2^k <= 2^n/P`. Native controls do not enlarge the target matrix when
unitary semantics are established. General linear operators retain their separate
embedding and density-matrix semantics.

`DiagMatr` also replicates its `2^k` entries. `FullStateDiagMatr` supports distributed
statevector diagonals, but its density-matrix application gathers the full diagonal.
Density state storage has `4^n/P` local amplitudes, while native register creation
still requires `P <= 2^n`. Native signed index bounds and communication count limits
are additional admission constraints. See the upstream
[matrix constructors](https://quest-kit.github.io/QuEST/group__matrices__create.html)
and the checked adapter tests in
[`native_admission.rs`](../../crates/quest/tests/native_admission.rs).

`prepare_matching_with_capacity` admits both per-rank bytes and an explicit
maximum ranks-per-node placement. Its conservative node bound uses the maximum
rank peak multiplied by that placement. The same bound is rechecked before
execution, accounting for resources allocated after preparation. No automatic
physical host discovery or recovery from native allocation failure is promised.
