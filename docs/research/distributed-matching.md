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
It begins with a complete matching encoding. It is **not** distributed sparse
coloring or a loader for a matrix larger than a node. That producer remains open
work; callers with externally prepared matchings can already load their distinct
owned records directly without retaining a global CSR.

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

The current schedule scans global basis indices on every rank and uses O(P)
peer rounds per batch. Bounded memory therefore does not imply scalable runtime.
The [acceptance record](../verification/2026-10-04-quest-cfd.md) includes actual
1/2/4/8-rank and split-communicator tests, whole-register portable differentials,
payload rejection and a native mid-execution failure test. It does not establish
multi-host scaling, real allocator exhaustion or capacity beyond node RAM.

Executable usage is exercised in
[`matching_collective.rs`](../../crates/quest/tests/matching_collective.rs) and
[`qsvt_matching_transform.rs`](../../crates/quest/tests/qsvt_matching_transform.rs).
Their full-matrix/state references are deliberately small test oracles; they are
not part of the prepared runtime's retained storage.
