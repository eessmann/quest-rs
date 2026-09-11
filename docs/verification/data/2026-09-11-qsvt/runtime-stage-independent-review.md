# Independent bounded review: local QSVT stages and comparison fixtures

Read-only review of `quest/src/qsvt/{mod,hadamard,projection}.rs`, their shared plan/register reservation transitions, runtime/QSVT Criterion benches, and the new C++ adapters/reference tests. No production or benchmark source was edited.

## Actionable findings

1. **C++ phase fixture publication can report success without writing a fixture.** `benchmarks/reference/cpp_qsp.cpp:83-92` does not check opening/writing/flushing the phase stream and returns success solely from the solver result; it also tolerates an unexpected non-phase outcome. Reproduced with the degree9 input and an output path below a nonexistent directory: exit code0, JSON `success:true`, no file created. Evidence: `cpp-reference-write-failure-review.json`. Require the expected phase outcome and a successfully closed output stream before a successful exit. The companion generalized adapter already checks its output stream.

2. **The general runtime benchmark continues timing failed samples.** `crates/quest/benches/runtime.rs:14-24` and `31-38` only record an error and defer returning it until Criterion finishes the whole benchmark. This contradicts the file's stated fail-fast contract and can publish timings dominated by error paths. Use the fail-fast checked-result pattern already present in `qsvt_execution.rs` (or an equivalent immediate abort) for both preparation and execution. This finding follows directly from control flow; no deliberate native failure injection was run.

## Stage and resource observations

The new consuming admission/preparation split is structurally sound in the reviewed paths. Admission owns all lowered plans, projector payloads and aggregate reservations before native allocation begins. Materialization transfers those reservations into RAII native owners. Partial moves and temporary owners release earlier native objects and remaining reservations on failure. Native register/matrix fields precede their reservations in drop order. The overlap stage reserves all three register workspaces before materialization and drops its packed host amplitudes/preparation reservation after transfer.

Coordinate cubes are recognized through exact integer membership, with coordinate uniqueness supplied by the model layer; fixed target bits become projection controls. General coordinate sets use a diagonal projector. Dense projectors form the admitted local isometry product with `faer::Par::Seq`; their explicit peak allowance covers the snapshot/matrix/native transfer path. No host full-state snapshot appears in warm transform execution; logical decoding remains an explicit cold readout. Execution still preflights register identity and the stored numerical fingerprint before mutation.

The QSVT benchmark correctly excludes fixture cloning/admission from native-preparation measurement, restores input outside warm-execution timing, and labels its inclusive execution-plus-conditioning measurement explicitly. It does not currently measure conditioning alone; an isolated conditioning benchmark would be needed before reporting a separate postselection duration. Warm general circuit execution can allocate its output bit vector; the QSVT plans in the fixture have no classical outputs, and the code does not claim general zero-allocation execution.

The C++ reference tests compare every matrix entry including global phase and final K, with explicit empirical scope. Four canonical fixtures reach degree8105 and a complex generalized fixture tests ordering and K. They do not confuse finite-grid differential agreement with an independent certificate. This review did not rebuild the pinned C++ project, rerun native QSVT integration, or establish GPU/distributed reference coverage; the root owns those validation runs.

A focused coverage gap remains for the public `.admit()` owners: the current local runtime tests exercise `.prepare()` convenience paths but do not directly assert reservation release when an admitted transform/overlap is dropped without materialization. The ownership implementation was inspected, but a direct regression would protect the new public boundary.
