# Memory boundary of the matching capacity experiment

As of the [2026-10-08 consolidation](../plans/2026-10-08-workspace-quality.md),
matching execution owns and admits a scratch register on every deployment.
The communication-array reuse described below is historical: QuEST does not
document that array as application-owned scratch. The current implementation
uses documented register cloning and amplitude access. Its two-register memory
bound applies again; earlier reduced-storage measurements do not describe the
current implementation.

The [Cirrus campaign](../verification/2026-10-06-cirrus-capacity.md) completed
distributed sparse execution but did not exceed any participating rank's memory
cap with its original stored input. Increasing that experiment's dimensions
cannot alone close the gate. The bound below concerns its controlled execution
profile, not every possible implementation of the matching unitary.

Let `E` be the number of canonical nonzero complex entries, `S` the padded system
dimension, `C` the padded number of matching labels, and `P` the MPI rank count.
Partial matchings have at most one entry per source column per label, hence
`E <= C*S`. The uncompressed COO32 source occupies `32*E` bytes. Derived encoding
records and dummy entries do not increase the counted original input.

The original experiment retains a flag qubit, the system and label registers, and one
external control qubit. Its whole-register state therefore has `D = 4*C*S`
complex amplitudes. Native QuEST 4.3.0 allocates a CPU amplitude array and an
equally sized communication array for each distributed CPU register. On a
one-rank communicator, native deployment disables distribution and retains only
the amplitude array. The original prepared matching implementation additionally owns a scratch
register of the same width for out-of-place permutation routing. At 16 bytes per
complex amplitude, these four native arrays contain

```text
native array payload per rank = 4 * 16 * D / P = 256*C*S/P bytes.
```

For `2 <= P <= 8`, the minimum occurs at `P=8` and is `32*C*S >= 32*E`:
the native array payload on each rank is
already at least the size of the entire original input. Coefficient records,
routing buffers, MPI internals, OpenMP stacks and allocator overhead add to it.
Consequently this controlled, two-register profile cannot complete with original
input larger than its enforced per-rank address-space cap on eight or fewer
nodes. A one-rank run has two arrays in total, occupying `128*C*S` bytes, and
also fails this inequality. Reducing an overconservative reservation cannot defeat this physical
storage bound.

`RegisterDeployment::host_array_bytes()` and `device_array_bytes()` report the
array payload sampled by the checked C++ adapter from allocated native
descriptors. They expose no native pointers and exclude allocator padding,
object metadata, temporary workspace, MPI internals and worker stacks. The
runtime's conservative generic register policy remains separate and unchanged;
removing an owned register reduces the total reservation. A payload measurement
is not a measurement of process RSS or address-space peak.

Preprocessing had an independent issue: its original reservation ledger retained
charges for completed phases, including canonicalization, endpoint coloring and
completion buffers. Lifetime-aware accounting must release only destroyed
owners, charge actual vector capacities, and preserve peak evidence. Publication
must charge live producer ownership rather than adding a historical peak to its
new workspace. Such corrections can admit legitimate workloads; they cannot
establish oversized execution by themselves. The implemented correction and its
budget regression are recorded in the
[memory follow-up](../verification/2026-10-06-sparse-capacity-followup.md).

The loader has a separate candidate for scaling work. Its generic replay
admission traverses a collective record directory and dry-runs both gate streams.
Directory queries involve multiple checked collectives per record; permutation
cycles are traversed repeatedly. Stage timings and exact logical-query counters
are needed before attributing observed wall time to that protocol. Validating
already sharded records collectively must retain coefficient, inverse-directory,
permutation-closure, identity, workspace and replay-cost checks.

Further execution designs can remove the full scratch register through bounded
in-place cycle routing, or use local native partitions with explicitly managed
distributed operations to avoid a native full communication buffer. Both require
new whole-unitary, control, adjoint and failure-path evidence. A capacity profile
for `U` without an external control has a different state-memory equation and
must be reported separately from the existing controlled measurements. No
representation change, modeled bound, allocation or source-file generation
counts as the required completed oversized-input execution.

## Reuse of the allocated communication array

A source audit identifies a smaller first change than introducing a new cycle
router. A distributed CPU statevector already owns `cpuCommBuffer`, with the
same number of amplitudes as `cpuAmps`. In the current matching implementation,
the opening controlled Hadamard completes before routing begins. The routing
interval uses checked indexed accesses to `cpuAmps`, Rust coefficient rotations
and bounded packets on the separate MPI transport context. It invokes no native
QuEST gate or register-copy operation. The closing Hadamard runs only afterward.

That interval now exclusively borrows the communication array as the existing
router's output staging area: initialize it from the input amplitudes, route
updates into it, copy the completed result back and release the borrow before
the closing Hadamard. This preserves unchanged and inactive-control sectors
without allocating the second full register. Checked adapters require CPU,
statevector, distributed, non-null and equal-length preconditions; the Rust
interface must hold exclusive register access throughout the interval and expose
no pointers. All outstanding private transport operations must finish before
commit. Native array pointers and their ownership remain unchanged. Errors after
the opening gate follow the existing fatal post-mutation
policy. One-rank execution needs the existing scratch fallback because native
deployment allocates no communication array there.

The audit is against native QuEST revision
`503552065045eaf89baba85e6cd6aad728525554`: allocation is in
`quest/src/api/qureg.cpp`, native Hadamard localization in
`quest/src/core/localiser.cpp`, and completion of exchanged arrays in
`quest/src/comm/comm_routines.cpp`. In this workspace, the relevant interval is
in `crates/quest/src/qsvt/matching/collective.rs` and `matching/batched.rs`;
checked indexed adapters are in
`crates/quest-sys/src/cxx_bindings/quest_bindings.cpp`. These references describe
the audited implementation, not a public scratch-ownership contract. Local and GNU/Cray multi-host evidence is recorded in the
[buffer-reuse validation](../verification/2026-10-06-matching-buffer-reuse.md).

For distributed execution this reduces native array payload from
`256*C*S/P` to `128*C*S/P` bytes. The first implementation retains the
conservative generic reservation to test that physical allocation change independently.
The existing four-array reservation per register still equals the complete
COO32 input at eight ranks in the fully populated case. A subsequent,
operation-specific admission contract therefore needs its own justified peak
storage analysis; releasing a modeled reservation alone is not the backend fix.

Acceptance for this stage requires portable and existing fused whole-unitary
comparisons on arbitrary states, both adjoints, failure flags, padding,
spectators and inactive controls; 1/2/4/8 ranks and split communicators; measured
native arrays; and bounded fatal termination after injected failures within the
borrow interval. Include a matching-label target in the distributed prefix,
consecutive applications after native buffer reuse, missing-buffer rejection
before the opening gate, and a witnessed failure after the first staging write
while another rank is waiting. Follow with the same frozen-input cluster comparisons and
actual process-memory measurements. Local and GNU/Cray focused tests now cover
these semantic and failure assertions. This implementation is absent from the
`8e02c49a` snapshot; its own completed multi-host evidence uses `9adbef47` and
remains separately identified.

## Admission still requires an operation contract

The generic four-array CPU reservation is a conservative policy, not a native
allocation invariant. QuEST's persistent distributed CPU payload is two arrays.
That fact alone does not bound every operation exposed by a general register.
For example, native dense multi-target application allocates per-OpenMP-thread
workspace, while root-supplied initialization creates three simultaneous global
conversion/broadcast arrays. The latter now has a corrected three-array
operation reservation and an actual four-rank rejection/recovery regression.

A future reduction must either reserve each operation's additional peak before
mutation or expose a restricted register whose allowed operations have audited
workspace bounds. The admission contract must account for concurrency, temporary
overlap, opaque MPI/native overhead and the configured OpenMP team independently.
An array-payload ledger is not a hard process-memory guarantee. The corrected
root initializer remains a convenience path with global storage on every rank;
the sparse capacity route continues to initialize local shards instead.

The deferred [restricted matching-state design](../superpowers/plans/2026-10-06-matching-state-admission.md)
describes a possible later stage without modifying native source. It starts with a
conservative native request-metadata bound, checked MPI layout/count limits and
reservation-owning amplitude buffers. The existing general register policy stays
unchanged; the restricted owner must not escape into arbitrary program execution.

The current Cirrus campaign follows the user's later requirement to use only
the documented environment and scheduler workflow. Unsupported requirements
are reported and left open; this design does not authorize private native
interfaces, attestation infrastructure, or custom limits as campaign workarounds.

## Compute-node enforcement observations

The [two-node Cirrus diagnostic](../verification/2026-10-06-node-enforcement.md)
observed the coordinator, launcher and both MPI ranks within the site's Slurm
job hierarchy. Its configured per-node `memory.max` was 780,945,850,368 bytes
(727.3125 GiB), with swap disabled. All inspected Slurm ancestors and control
files were root-owned and not writable by the submitting user. A smaller
delegated job subtree was not available in these observations.

Each MPI rank also reported 16 MiB of HugeTLB memory. The fixed HugeTLB control
files were absent from the job hierarchy; the diagnostic did not establish
whether a mount option includes that memory in the general memory controller.
These observations do not prove an all-memory cap, its response to exhaustion,
or coverage of every helper process. The existing 8 GiB rank address-space
limits still cannot be treated as per-node caps. A strict capacity experiment
needs a separately verified enforcement contract covering its actual process
and memory domains; reducing managed reservations cannot supply that contract.
