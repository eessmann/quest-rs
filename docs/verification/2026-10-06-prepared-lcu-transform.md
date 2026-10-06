# QSVT over prepared distributed weighted matching operators

The CPU/MPI `prepare_matching_lcu_transform` adapters now apply the reusable
compact QSVT schedule to an owning weighted matching encoding. They consume
the existing [prepared composition](2026-10-06-distributed-weighted-matching.md),
preserving its independently sharded children and the distinct identities of
the represented operator and whole unitary. Native QuEST is unchanged.

The [method and API contract](../research/prepared-lcu-transform.md) explains
descriptor-only phase factories, mapped compact projectors, complete admission,
inverse orientation and resource accounting. The implementation streams the
schedule and traverses native local partitions. Production requires no complete
matrix, coefficient list, gate stream or global statevector on a rank.

## Independent semantic evidence

The [focused receipt](data/2026-10-06-prepared-lcu-transform/focused.json) records
independent source review and local CPU/MPICH tests against the
[final 13-file identity](data/2026-10-06-prepared-lcu-transform/source.json).
The source remained unchanged through those final checks.

| Check | Independent result |
| --- | --- |
| Arbitrary whole-register forward and standalone adjoint | All 512 amplitudes compared with a bounded portable reference |
| Signed controls and non-success sectors | Padding, selectors, flag/color failures, response and spectators included |
| Zero and underflowed preparation weights | Five input owners preserve filtering and surviving branch semantics |
| MPI equivalence | Seven requests covering 1/2/4/8 ranks, split communicators and edge inputs at 1/2 ranks |
| Projector mapping | Independent 64-coordinate predicates with nonzero fixed values, range offsets and reordered targets |
| Projected polynomial block | Independent scalar Chebyshev predictions for degrees 1, 3 and 5 |
| Reciprocal replay | Successful physical amplitudes compared directly with a complex matrix inverse |
| Admission and fatal boundaries | 21 subprocess cases plus three allocation-free resource tests |
| Compatibility | Existing matching and weighted-composition regressions pass |

The independent polynomial fixture uses `A = 0.625 I + 0.125 i X`, with source
`A†` and its actual implemented normalization. For the selected imaginary-U00
Wx convention, endpoint phases `π/4` and zero interior phases give the required
Chebyshev response. The initially proposed all-zero phase fixture instead gives
zero selected response; correcting that test expectation did not change the
production synthesis algorithm. The reciprocal oracle uses the independently
derived complex inverse and the physical `||b||/(alpha*c)` scale.

The final independent CPU parent test completed in 0.07 seconds, the seven MPI
requests in 4.16 seconds, and the resource/fault parent tests in 11.38 seconds.
These are bounded test timings, not comparative performance results. Pure phase
factories, actual-capacity accounting and unchanged component tests have their
own scoped evidence in the receipt. An earlier sandbox-denied MPI invocation
is retained separately from its successful matching-native rerun.

## Review corrections and resource meaning

Review reproduced a collective hang without allocating a large payload. Legal
rank-dependent placement limits and an accounting-only external reservation
caused one rank to overflow a checked node-byte product before the agreement
entered by its peer. The original two-rank probe reached its timeout; after
moving the complete checked result inside collective agreement, the unchanged
probe returned a common error in 0.390 seconds. A maintained regression also
covers the case. This is failure-control evidence, not actual large-count or
large-memory transport evidence.

Review also found that communicator-dependent constructor/admission loops
needed to participate in the work cap. Constructor coordination now has a
checked `256*P` allowance. Application preflight includes the conservative
`256*P*(semantic_steps+8)` coordination floor, with control, rank and aggregate
checks agreed before those loops. A degree-zero schedule cannot bypass that
admission merely because it makes no source queries.

The [initial tested identity](data/2026-10-06-prepared-lcu-transform/source-initial.json)
is preserved. Only the two native transform resource files changed between
that snapshot and final focused verification. Existing phase, projector and
source semantics remained unchanged. Strict scoped Clippy passed.

All child scratches, source owners, phase capacities, projector vectors and
temporary buffers participate in managed admission. Construction comparison
traffic and repeated directional routing payload have separate receipts.
These models exclude allocator overhead, native temporaries and MPI protocol
traffic; they do not certify actual host placement or process RSS. Earlier
producer, synthesis and child preparation costs remain separately charged.

## Reproduction and remaining scope

```sh
export QUEST_ROOT=/path/to/installed/quest
export MPICC=/path/to/matching/mpicc
export PATH=/path/to/matching/mpi/bin:$PATH
cargo test -p quest-qsvt --all-features --test transform_schedule_factories --test owned_replay --test replay_transform --test reciprocal_review
cargo test -p quest-rs --features qsvt,mpi --test matching_lcu_transform --test matching_lcu_transform_collective --test qsvt_matching_transform
cargo test -p quest-rs --features qsvt,mpi --lib qsvt::matching_lcu -- --test-threads=1
```

This closes the reviewed local CPU/MPI transform integration. Uniform encoding,
preparation, RHS and native arithmetic error bounds remain unknown where the
descriptor says so. The tiny inverse test therefore does not supply a certified
physical residual, success lower bound or quantum CFD accuracy claim. Generic
distributed tensor and persisted-child composition, physical-history LCU
decomposition, multi-host capacity and actual huge-count transport remain
separate work. Workspace checkpoint results also retain their own source scope.
