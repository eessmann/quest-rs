# Persisted matching to prepared CPU/MPI execution

The consuming `LoadedMatching::into_prepared_matching` bridge passed independent
review and focused checks on 2026-10-06. It transfers an exclusively owned loaded
snapshot into the existing prepared matching runtime, with collective metadata,
ownership and whole-live resource admission. Native QuEST is unchanged.

The [method and API](../research/persisted-matching-preparation.md) document the
ownership transition, limits, costs and failure contract. The
[final source manifest](data/2026-10-06-persisted-matching-preparation/source.json)
has SHA-256 `131fa971f4a378647a0e0f55579ea961c601c4e12876f18e052b5be2cb401164`;
its six-file content aggregate is
`4b684544be3a29b7f857138fa9cabad099657e8e5d5f74f2877c27dac47b59c0`.
The [initial reviewed manifest](data/2026-10-06-persisted-matching-preparation/source-initial.json)
is preserved. Only the public test changed afterward to add the prior-child
failure case described below. These focused identities exclude dependencies and
do not attest a complete executable build.

The [focused receipt](data/2026-10-06-persisted-matching-preparation/focused.json),
SHA-256 `27478f719ce8b8278bacbfd456735e74bdd6a43783e78bd508d2663a8d97782f`,
records test scopes and private log hashes. This stage follows checkpoint 07;
it has no newer broad workspace acceptance attached.

## Ownership and capacity

The bridge checks the complete matching header, manifest SHA, ordered targets
and policies collectively before conversion. Atomic `Arc` unwrapping establishes
exclusive loaded-data ownership; a retained alias on one rank rejects the request
commonly. A cloned scalar replay recipe owns no loaded allocation and remains
compatible. Existing local snapshot and replay APIs remain available.

Source, snapshot, target capacity, temporary control storage and native preparation
overlap remain charged while caller loading guards and earlier children are live.
Actual snapshot capacity is checked before filling it. The parent collective lane
ends before native preparation, and temporary accounting is released only after
the native child acquires its own reservations. Source/operator identity,
construction identity and manifest integrity remain distinct.

This stage also corrects inherited native validation accounting: its incoming
permutation vector now resizes the reservation to actual capacity and rechecks
rank/node admission before filling or routing. A test-only allocation of at least
128 KiB previously passed under a 64 KiB environment; it now rejects collectively.
This is a real bounded allocation regression, not an actual large-count test.

The returned `rank_peak_bytes` and `node_peak_bytes` are conservative admitted
caps. `planned_rank_peak_bytes` predicts overlap at admission boundaries. None is
a measured process or allocator high-water mark. Prior loading still requires its
own admission; this bridge does not fix the separately identified loader-capacity
boundary or retroactively account for IO.

## Executed checks

The independent final public test passed at 1/2/4/8 local ranks and with a
four-rank world split into two independent two-rank groups. Its bounded portable
reference checks all 128 amplitudes for forward and standalone adjoint action,
including rectangular padding, complex coefficients, unsuccessful flags,
reordered targets, spectators and both outer-control values.

The final test addition retains a successfully prepared child while a second,
valid source and layout encounters a rank-zero conversion-budget failure. The
register and child-inclusive ledger remain unchanged; the retained child then
passes the same whole-unitary comparisons. Independent review reran that rank
matrix in 2.95 seconds. No production change was needed for this coverage addition.

On the unchanged implementation, independent checks also passed three private
tests covering ten MPI fault jobs, the actual-capacity regression at one and two
ranks, and strict scoped Clippy. The owner ran eight selected top-level tests,
including three existing matching/persistence regressions, and repeated Clippy
and formatting after the final test delta. Test parents and child MPI jobs are
reported separately rather than added into one inflated test count.

Early malformed IO fixture limits prevented some attempts from reaching the
bridge. Those failed logs are retained, including one with a preselected `green`
filename. Only the fixture's loading setup was corrected; the intended late
bridge limit stayed unchanged. They do not count as bridge rejection evidence.

With installed QuEST and matching MPI configured, run from the workspace root:

```sh
cargo test -p quest-rs --features qsvt-io,mpi --test matching_persisted_preparation
cargo test -p quest-rs --features qsvt-io,mpi --lib qsvt::persisted_matching::preparation::failure_tests -- --test-threads=1
cargo test -p quest-rs --features qsvt-io,mpi --lib qsvt::matching::collective::preparation_tests -- --test-threads=1
cargo test -p quest-rs --features qsvt-io,mpi --test matching_persisted --test matching_collective --test matching_runtime -- --test-threads=1
cargo clippy -p quest-rs --features qsvt-io,mpi --lib --tests --no-deps -- -D warnings
```

Each test job creates its own tiny fixture. Publishing once and reloading those
same files across rank counts, composing weighted LCU and QSVT, and measuring
their complete routing traffic remain a separate experiment. These local checks
do not establish multi-host capacity, actual huge-count transport, accelerator
support or a certified scientific inverse result.
