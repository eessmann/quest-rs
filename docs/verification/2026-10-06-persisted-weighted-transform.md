# Persisted weighted sparse inverse: local MPI execution

The fixed dimension-32 inverse completed on 1/2/4/8 local MPI ranks and two
independent two-rank split groups. One eight-rank producer created the sharded
inputs; one compilation job froze the degree-33 phases; every replay consumed
the same immutable files and phases. Each replay performed forward, standalone
adjoint and forward applications from fresh basis states. All 32 physical
solution coordinates entered each residual calculation.

The [method and runnable protocol](../research/persisted-weighted-transform.md)
defines the three complex matching sources, weighted composition, physical
scaling, bounds and failure semantics. The represented source is
`H = A† = (5 I − i X)/8`, with actual LCU normalization
`1.2500000000000002`. The 10-qubit register places the X transition across
rank partitions. This is an executed sparse inverse example; it is not a
physical CFD convergence result.

## Executed result

The final campaign completed in 6.54 seconds, including MPI startup, publication,
compilation and all five replay jobs. Every replay had maximum relative residual
`2.962203156364751e-5`, below `1e-3`, and success probability between
`0.023450768217972272` and `0.02345076821797234`. All calls passed the vector-error
and whole-register probability checks. The inverse residual uses A for forward
execution and A† for the literal adjoint.

The [independent saved-output review](data/2026-10-06-persisted-weighted-transform/attempt-4/independent-review.md)
checked all seven jobs, 28 rank receipts, 57 local call receipts and 29 immutable
input bindings. An independent cosine-series evaluation agrees with the native
residuals and success probabilities. The [publication index](data/2026-10-06-persisted-weighted-transform/attempt-4-publication.json)
binds the saved build, execution, phase, numerical and review artifacts without
copying binaries or HDF5 payloads into the repository.

| Replay | Job wall time (s) | Maximum one-application time (s) | Matching sent bytes, three applications summed across ranks | Largest modeled managed rank peak (bytes) |
| --- | ---: | ---: | ---: | ---: |
| 1 rank | 0.740 | 0.0635 | 0 | 2,514,056 |
| 2 ranks | 0.803 | 0.0829 | 2,577,408 | 2,377,608 |
| 4 ranks | 0.910 | 0.1110 | 8,283,648 | 2,309,384 |
| 8 ranks | 1.101 | 0.1533 | 14,974,464 | 2,275,272 |
| Two split groups, 4 ranks total | 0.818 | 0.0834 | 5,154,816 | 2,377,608 |

Matching counters exclude PREP, projector/response gates, constructors, readout,
native internal MPI and protocol traffic. The table sums all three applications;
it does not confuse a per-rank query count with one logical circuit's query
count. The small problem shows increasing communication overhead, not a speedup.
The maximum observed per-process RSS high-water was 317,972,480 bytes, during publication. At eight
ranks the sum of process high-waters was 2,537,975,808 bytes, which is an upper
bound on concurrent node RSS rather than a measured simultaneous peak. These
observations include costs excluded from the managed-storage model.

The build captured 975 declared workspace/fixture/configuration files and 16
actual local compiler packages. Source/native/tool snapshots were unchanged
before and after execution. The pinned executable SHA-256 is
`7acd0d38dc30a81aa299ad5704a28d7c2e0acec00f0d4a4a5f20f584759dcab9`;
the final campaign receipt SHA-256 is
`a9fb283e26c948dc2c2e5ff6871559fae968692173b54981515cc90fd19ea89f`.
The declared source set is a workspace superset, not an exact transitive compiler
input closure. External registry sources and transitive native libraries are
not fully hashed; this is not a hermetic-build certificate.

## Failures retained and corrected

The [historical publication index](data/2026-10-06-persisted-weighted-transform/historical-publication.json)
preserves attempts 1 and 2 with their original identities and outcomes:

1. A process-wide file-size cap intended for captured output also rejected an
   MPI transport shared-memory allocation. Publication terminated with SIGXFSZ;
   six dependent jobs did not run. A minimal eight-rank MPI initialization test
   reproduced the failing `ftruncate`. Bounded parent-side pipe capture replaced
   that process-wide cap; arbitrary MPI-internal files are not thereby quota-bound.
2. Publication and compilation completed, but receipt validation rejected the
   legitimate zero transport ceiling on one rank. Five replay jobs did not run.
   The narrow schema correction accepts zero only for that field at P=1 and is
   tested against the original saved compile receipt.
3. With fresh version-two resources, the one-rank inverse passed but all four
   multi-rank jobs reached their 180-second deadlines. Source inspection found
   that readout passed a peer rank as the message tag, so paired ranks waited
   for different tags. A separate bounded regression confirmed that both ranks
   entered the real readout and then timed out. The original campaign had no
   stage traces, so its precise stopping point is not independently recorded.

The [attempt 3 publication index](data/2026-10-06-persisted-weighted-transform/attempt-3-publication.json)
retains its independent saved-output audit, raw numerical receipts and immutable
input bindings. HDF5 payloads are represented by file hashes in the checked-in
evidence; the original campaign files remain separate local artifacts.

The readout fix uses one shared tag on its private communicator and validates
the received byte count. Independent tests passed on 1/2/4/8 ranks and split
communicators with a known state containing both successful and failed branches,
both adjoint orientations and all 32 solution coordinates. A synthetic short
packet status rejects collectively; it is not an actual truncated-wire test.
The [correction evidence](data/2026-10-06-persisted-weighted-transform/readout-fix/README.md)
preserves the failing bounded regression, source identities, passing owner and
independent checks, and explicitly classified intermediate compile/lint failures.
The separate 256-amplitude cold reference still compares the whole unitary,
including padding and signed spectator controls. The final campaign changed
neither the operator, tolerance nor resource ceilings.

## Acceptance boundary

The source was sharded at creation, publication used eight logical buckets, and
replay reused owned immutable resources with bounded local-state routing. This
connects the previously tested producer, loader, prepared matching, weighted LCU
and QSVT APIs in one executed example. The basis RHS has an identity coherent
preparation; direct local simulator initialization is reported separately.

Scalar polynomial/completion diagnostics and measured inverse residuals do not
establish a uniform total-error certificate for source rounding, PREP, native
execution or arbitrary RHS preparation. Dense reference matrices and normal
equations were not used by this consumer. Actual multi-host input exceeding each
node's memory cap, real huge-count transfers, hardware execution and physical
CFD accuracy remain separate acceptance gates.
