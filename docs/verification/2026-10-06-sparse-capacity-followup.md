# Sparse memory accounting and native resource loading

This follow-up changes producer memory accounting, exposes allocated native array
payloads and gives native execution a loader that does not first admit a portable
gate recipe. The earlier [thirteen Cirrus cases](2026-10-06-cirrus-capacity.md)
retain their original source identities and measurements. None of these changes
closes the beyond-node-memory capacity gate.

## Producer ownership and memory

The producer now charges live owners instead of retaining every completed
phase's allocation charge. Exchanges release their workspace only after it has
been destroyed. Canonical stream storage, coloring endpoint lists and completion
path records have explicit lifetimes. Vector growth charges both old and new
allocations during their overlap, then releases the old capacity after dropping
its allocation. Nested vertex vectors are destroyed before their containing
vectors receive credit.

`ProducedMatching::retained_bytes()` counts the returned container and actual
capacities of its owned arrays once. Publication uses that live retained amount
alongside its own workspace. `peak_managed_bytes` remains historical peak
evidence; it is no longer misused as an additional current allocation.

The regression generates 256 columns and 512 nonzeros per rank directly as
shards. Its 160 KiB admission failed before the correction with a 166,144-byte
request against 163,840 available bytes. It now completes on 1/2/4/8 ranks and
matches the larger-budget header, coefficients and inverse records exactly.
A 28 KiB limit and malformed input still reject collectively, followed by a
successful valid operation. Existing rank-independent coloring, completion,
split-communicator and persistence tests also pass.

These numbers bound explicit managed payload, including conservative allowances.
They do not bound allocator overhead, HDF5/MPI internals or process memory. An
allocator can return excess capacity before that capacity can be inspected;
rejection then drops it. A hard process limit remains a separate requirement.

## Actual native array payload

The checked C++ adapter samples QuEST's allocated array descriptors and returns
checked byte counts through `RegisterDeployment::host_array_bytes()` and
`device_array_bytes()`. No native pointer is exposed. A prepared matching owner
also exposes the immutable deployment metadata of its scratch register.

For a distributed CPU register, host payload includes both the amplitude and
communication arrays. Native QuEST disables distribution at one rank, where
only the amplitude array is allocated. CPU statevector and density-matrix tests
check that distinction; MPI matching tests check actual deployments at
1/2/4/8 ranks and on split communicators. Accelerator execution was not tested.
The conservative runtime reservations remain unchanged.

Array payload excludes allocator padding, metadata, temporary workspaces, MPI
internals and OpenMP stacks. It is neither process RSS nor a reservation. The
[controlled-profile storage bound](../research/distributed-capacity-memory.md)
shows why increasing the current experiment's dimensions cannot close its strict
capacity gate on eight or fewer nodes.

## Capacity receipt version 4

Placement-aware runs load the validated immutable resource with
`load_matching_resource`, then construct the same controlled native matching
unitary. All flag, label and external-control qubits remain. Resource validation
and portable gate admission have independent limits; the compatibility loader
continues to perform both. The measured loader work is documented separately in
the [phase measurements](2026-10-06-matching-load-phases.md) and
[loader contract](../research/persisted-matching-loading.md).

Version 4 receipts identify `resource-only-native`, mark portable replay admission
as `unrun`, and represent its unavailable communication bound as JSON `null`.
They record rank-local load phase times and logical broadcast counters, plus
separate input/scratch array payloads and checked totals. Logical call counts are
not measured MPI wire bytes. Historical receipt versions remain accepted with
their original contracts. Legacy invocations without placement arguments retain
portable admission.

Local dimension-64 runs passed with one and two ranks, two requested OpenMP
threads and three controlled forward/adjoint roundtrips. Both report 8,192 host
payload bytes per native register, 16,384 combined: the one-rank state is twice
as large locally but has no distributed communication array. The two-rank test
uses one physical host, so it is local validation. Both runs explicitly leave
capacity open. A legacy one-rank invocation also passed.

The historical thirteen cluster completion records pass the updated validator.
The Cirrus Python fixture suite passes 58 tests, including rejection of false
portable-admission claims, inconsistent native payload sums and invalid loading
measurements. Current multi-host follow-up execution is recorded separately
when complete; these local results do not stand in for it.

## Combined local validation and source identity

The current source passed all 1,512 default-feature Nextest tests, with one
explicitly skipped test. The all-feature run passed 1,698 of 1,699 tests, with
six skipped campaigns. Its sole failure was the existing CMake paths-with-spaces
fixture: CMake could not open its installed `CMakeDetermineCompilerABI.cmake`
module. One isolated and eight concurrent unchanged traced executions then
passed and read all 15,124 module bytes. Those traces observed no relevant
resource, permission or IO errno. The original failure remains unexplained;
neither a production fix nor a completely passing all-feature run is claimed.

All 55 default and 65 all-feature doctests passed, with one ignored snippet in
each lane. Default workspace
checking, strict all-feature/all-target workspace Clippy, binding freshness,
formatting and whitespace checks passed. Independent reviews found no material
issues in producer ownership, native telemetry, resource-only validation or the
receipt migration.

The deployed immutable snapshot is
`8e02c49a7acf70a84f0c55aa40caba647024090a8cd72ffd3267bb72d8d3925b`,
containing 2,546 files. Verification passed before and after making its Cirrus
source tree read-only. The [summary](data/2026-10-06-sparse-capacity-followup/summary.json)
records exact relevant source hashes and raw local-evidence hashes. Subsequent
reporting changes are separate from that frozen source. Native QuEST source was
not modified; Apple Silicon execution and strict multi-host capacity remain open.

## Acceptance correction after the frozen snapshots

A subsequent read-only audit found that the capacity predicate compared original
input with the summed `RLIMIT_AS` limits of MPI ranks on each node. Those limits
cover the ranks and their threads; they do not cover the batch coordinator,
launchers or external helpers. Sampling likewise observes only the MPI rank PIDs.
Neither exclusive allocation nor that rank envelope verifies a whole-node cap.

A regression reproduced the false-positive boundary: 2,049 original bytes
exceeded each synthetic 2,048-byte rank envelope and incorrectly closed capacity.
The corrected runner always leaves strict capacity open without verified
whole-node enforcement, reports that enforcement as false and the unavailable
whole-node cap as `null`, and retains the separate
`original_input_exceeds_rank_process_envelope` comparison. Arbitrary asserted
whole-node fields cannot supply proof. No broader enforcement mechanism was
implemented by this correction.

All 72 Python fixture tests passed, including two added tests. Revalidation of
24 completed historical cases, including all thirteen earlier Cirrus cases,
preserved their numerical fields and open capacity outcomes. Independent review
found no material issue. The [correction record](data/2026-10-06-sparse-capacity-followup/node-enforcement-correction.json)
contains exact changed-source and test-evidence hashes. This Python-only change
postdates both frozen `8e02c49a` and `9adbef47` snapshots and is absent from their
remote runners. Their fixed 4 MiB cases remain open under either predicate;
completed execution does not establish a whole-node enforcement boundary.

## Completed version 4 multi-host execution

All six fixed-work capacity cases completed with source
`8e02c49a7acf70a84f0c55aa40caba647024090a8cd72ffd3267bb72d8d3925b`.
The independent GNU and Cray builds, jobs 537408 and 537399, used fresh targets
with optimization level 2, debug information disabled and incremental compilation
disabled. The frozen capacity executable SHA-256 values are
`09b60954556d489dc9d85d264e986ff08a9077589f970687bc8ac40ea9cc57be`
for GNU and
`2e425bfa2acf9cf2ec8ac5c6bc369f632a65cd48576375537109d9b621e50a6b`
for Cray. Both use their previously verified native package/compiler profiles.
Build receipts and live frozen binaries were independently cross-checked.

Each case used dimension 65,536, three controlled forward/adjoint pairs, one MPI
rank per exclusive node and 288 requested OpenMP threads. MPI shared-memory
groups and distinct processor names confirmed actual 2/4/8-node placement.
Native multithreading flags were enabled; the actual OpenMP team size was not
measured. Source generation, preprocessing, loading, matching pair arithmetic and
MPI routing remain serial within each rank.

All times below are seconds. Stage values are maxima over ranks. The roundtrip
column is the median of three rank-maximum forward/adjoint durations; launcher
time also includes initialization, storage, validation and shutdown.

| Compiler | Nodes | Job | Producer | Publication | Load | Prepare | Median roundtrip | Launcher |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cray | 2 | 537405 | 0.498 | 0.104 | 1.243 | 0.249 | 0.339 | 4.985 |
| Cray | 4 | 537406 | 0.576 | 0.090 | 1.981 | 0.315 | 0.490 | 5.907 |
| Cray | 8 | 537407 | 0.687 | 0.084 | 3.201 | 0.361 | 1.053 | 9.655 |
| Gnu | 2 | 537410 | 0.467 | 0.099 | 1.135 | 0.234 | 0.325 | 4.618 |
| Gnu | 4 | 537411 | 0.582 | 0.115 | 1.814 | 0.300 | 0.463 | 5.879 |
| Gnu | 8 | 537412 | 0.687 | 0.102 | 3.000 | 0.335 | 0.962 | 9.048 |

These fixed-size timings increase with node count; they do not demonstrate a
speedup. The [six-case data](data/2026-10-06-sparse-capacity-followup/capacity-v4.json)
records phase timings, logical communication counters, memory samples, source
and receipt hashes, and all numerical checks. Portable replay admission was
unrun, with a `null` bound and zero admission counters. Resource reverse-directory
validation remained active and reported `131072 + P` logical broadcasts per rank.

Each case stored exactly 131,072 original COO32 coefficients, totaling
4,194,304 bytes. All 786,432 records across the six cases were decoded and checked
for exact ownership and coefficient bits; hashes matched between GNU and Cray at
the same placement. Persisted HDF5 files were checked independently against their
manifest lengths and hashes and did not inflate the counted original input.
All three numerical roundtrips passed, with maximum sampled amplitude error
below 8.68e-19 and final norms within tolerance of one.

This source still owns both the input and scratch native registers. Their
combined actual host-array payload is 16/8/4 MiB per rank at 2/4/8 nodes. These
measurements precede the separate `9adbef47` buffer-reuse implementation and must
not be presented as its allocation evidence. Each MPI rank enforced an 8 GiB
address-space limit and used a 4 GiB managed budget; the explicit worker-stack
allowance was 287 times 8 MiB. The largest sampled rank RSS was
223.62 MiB and largest sampled rank address space was 2.603 GiB.
Samples may miss peaks and cover only rank PIDs; independent high-water bounds
remain separate in the data.

Strict capacity remains **open**: 4 MiB original input is smaller than every
8 GiB rank cap, and no aggregate whole-node cap covering coordinators, launchers
and external helpers was verified. These are completed multi-host numerical and
scaling measurements. They establish neither oversized-input execution nor the
remaining whole-node enforcement obligation.
