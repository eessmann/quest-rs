# Checkpoint 08 matching fatal-test diagnosis (read-only)

The saved failure is a diagnostic-output assertion failure; it does not establish a failure to terminate the MPI job. No source edits, tests, builds, subprocess reproductions, or campaign jobs were performed for this diagnosis while checkpoint 08 gates were active.

## Saved execution evidence

`<temporary>/quest-programme-checkpoint-08-allfeatures.log`, lines 1510–1530, reports parent duration 1.435 s and inner test duration 1.00 s. The only failed googletest assertion is the absent `unrecoverable distributed MPI operation or cleanup failure` substring. Actual captured stderr is the earlier checked-failure injection marker. The assertions for unsuccessful exit, exit code other than GNU timeout 124, and the injection marker did not fail. The exact subprocess numerical exit code was not printed or retained separately in that output, so it is unknown from this saved evidence. These observations support prompt unsuccessful/non-timeout termination, not an exact MPI abort exit-code claim.

## Source path

`failure_tests.rs` invokes H on both local native registers, then an all-agree, before rank 0 prints the injection marker. Rank 0 invokes the checked local adapter with `start=local_amplitudes` and one output element. `quest_bindings.cpp::require_local_range` rejects this range (`count > numAmpsPerNode-start`) before any memory read and throws `std::invalid_argument`; the CXX exception becomes a Rust error. The `fatal` helper catches errors or panics and calls `quest_sys::mpi::abort_job`. The C++ helper first calls `fputs(..., stderr)`, then `MPI_Abort(MPI_COMM_WORLD, EXIT_FAILURE)` when MPI is initialized/not finalized, with `std::abort()` as fallback. Rust has an additional process-abort fallback if the FFI call unexpectedly returns.

The intended native abort argument is EXIT_FAILURE; it is not proof of the launcher's observed subprocess code. Writing a diagnostic before MPI_Abort does not establish that the MPI launcher will drain and forward that diagnostic before termination. The missing text is compatible with a launcher delivery race, but this particular saved run does not independently prove that cause. No production fflush, sleep, or abort-path change is justified by the captured result alone.

## Narrow regression proposal after freeze release

Keep the two-rank subprocess under its existing 20 s GNU timeout, require a nonzero/non-124 outcome, and print/record the actual subprocess status for future failures. Replace mandatory launcher-forwarded abort prose with a fixed bounded filesystem witness under a fresh parent-owned private temporary directory:

1. Each rank writes and syncs a fixed native-entered/peer-ready marker after H; agree these fixture stages before the deliberate range failure or peer receive.
2. Rank 0 obtains the actual checked-range error and writes/syncs its fixed checked-error witness before handing that same error to the production fatal boundary. Failure to create the witness must fail fixture acceptance, not masquerade as a successful abort.
3. Rank 1 still enters the deliberately unmatched receive; no matching response is introduced. A fixed return marker after that receive and another after `fatal` must remain absent.
4. Parent checks exact witness contents, prompt unsuccessful/non-timeout status, and absence of the return witnesses. These conditions exclude MPI-startup/fixture-admission failures and preserve the essential fatal termination acceptance without requiring best-effort launcher stderr delivery.

The peer-ready stage establishes that the peer reached the receive stage; it should not claim an independently measured instant at which the peer was already blocked inside MPI_Recv. Existing diagnostic text may remain optional debugging output.

## Frozen source hashes

- `crates/quest/src/qsvt/matching/collective/failure_tests.rs`: `9231d7a60ef93583e1799990c9b19ccc60ea338418541d5fdac55f21f0a56976`
- `crates/quest/src/qsvt/matching/collective.rs`: `f3abfefce349da8a16ca43d8d0d9b3e2d261b62ace55dde193790d2451aefb8a`
- `crates/quest-sys/src/mpi.rs`: `838489993e2cbb9f70d90fe64eaeb01c7ad9f976249eae73973bafcc302c154e`
- `crates/quest-sys/src/lib.rs`: `991b1e01596e789b5fd8a06f7c405d7af59d4a99daccbfa345a740db57e75879`
- `crates/quest-sys/src/cxx_bindings/quest_mpi.cpp`: `ef6cb0796533a4c867c7ae54fc738c6865df0d762c354c2cc1a3bcc6685ffc21`
- `crates/quest-sys/src/cxx_bindings/quest_bindings.cpp`: `7a64f5b56cf63bac384a5e8ce01f75db526339dc6bb308797bdcd16f5858a799`
