# Torc on Cirrus: native builds and direct MPI validation

The installed Torc CLI completed the one-node GNU and Cray compiler builds.
Subsequent direct Slurm validation passed the full GNU lane. The Cray lane
failed two compile-fail groups in each feature mode because nested dependency
builds could not link; its remaining validation stages passed. These results
apply to the frozen source identified below.

## Deployment and identities

The user-provided installation is under `$WORK/.local/bin`. Torc and the Slurm
runner report version `0.41.0`; the running client and standalone server report
build `74076e1` and API version `0.23.0`. Binary SHA-256 identities are recorded
in the [assessment](../research/torc-cirrus.md#installed-cli-follow-up).
The official `v0.41.0` tag resolves to
`74076e17c472418f956da02765e48043545fac1a`, matching the reported build identifier.
Its standalone/create/run/export/resource implementations are unchanged from the
earlier inspected commit. CLI help, dry-run validation and the actual job
receipts establish this installation's behavior.

The default server endpoint was not reachable. Each one-node allocation instead
uses Torc's documented standalone server on loopback, with the live database
and worker files in the site's node-local temporary directory. The database is
reopened between `create`, `run` and `workflows export`. Closed database and log
bytes are archived on EPCCFS before the batch script exits. Source, Cargo
targets and native build receipts remain on EPCCFS throughout execution.

No Python controller is involved. The [YAML workflow](fixtures/cirrus/torc-native-build.yaml)
and [batch payload](fixtures/cirrus/torc-native-build.sbatch) use direct mode,
truthful one-node/eight-CPU resource admission, and no Torc memory enforcement,
automatic retries or resource-changing recovery. Each job requests an exclusive
node, eight CPUs, two hours, account `d458`, partition `standard`, QoS
`lowpriority`, and no Slurm memory flag. The two concurrent jobs occupy two nodes.

## Frozen input and verification

The source manifest contains 2,579 files, including the current MPI request ABI
checks and the independent safe Rust MPI consumer. File contents were verified
before archiving and after transfer; the extracted source is read-only.

| Input | SHA-256 |
| --- | --- |
| Source manifest | `d8c05b505d90989f405ce79f18abdf7566eb8f68ed6be81b735e7804e7ed993a` |
| Source archive | `1a2fdd23bc42214aa5dd5e6a6df53bf8a9f5e5ac6cb8aba54794b30dcb6e7ddd` |
| Retained GNU QuEST library | `afdf97834ae301ce6029fd85c3e66b86c6b7934c1bc97c5c98d05d2ef3df3ee3` |
| Retained Cray QuEST library | `f1eba6a43b2f9b0b0ed043adeb66840bdae977aa1920efa6e767e4e3ec14af89` |

Both native libraries retain unchanged QuEST revision
`503552065045eaf89baba85e6cd6aad728525554`. Fresh compiler-specific Cargo targets
disable debug information and incremental artifacts through documented profile
variables. Debug assertions remain enabled. The native payload loads the
compiler-specific central serial HDF5 module.

Observed preparation checks:

- Torc `create --dry-run`: valid; no errors or warnings; one job, one resource
  requirement, no scheduler actions.
- Both batch payloads pass `bash -n`; `git diff --check` passes.
- Source and native-library SHA-256 checks pass.
- Both standalone servers created workflow 1 and started its first build
  attempt in direct mode with eight CPUs and one node.
- Native doctor, binding freshness, default/all-feature compilation and the
  independent MPI consumer build passed in both compiler lanes. The final
  source manifest check also passed.

## Job status

| Lane | Slurm job | Torc workflow / run / attempt | Final build state |
| --- | --- | --- | --- |
| GNU | 538069 | 1 / 1 / 1 | Completed; Slurm `0:0`; 3m42s allocation |
| Cray | 538070 | 1 / 1 / 1 | Completed; Slurm `0:0`; 3m40s allocation |

A passed workflow requires an exported completed job, a completed result with
return code zero, matching workflow/job/run/attempt identities, and the native
payload's successful completion receipt. The CLI exit code alone is insufficient.
Both workflows satisfy those checks. Each archived export contains exactly one
completed job and result, return code zero and attempt 1. Both archive-preserving
batch scripts exited successfully, and both Slurm allocations completed `0:0`.
The retrieved evidence archive has SHA-256
`8fabe49f7cde174cd12f467d921a74609f4e7c949486efbd609bdf8c73e1e334`.
It contains metadata exports, native stage logs/statuses, Slurm accounting and
each node-local archive, including SQLite database/WAL files and actual job
stdout/stderr bytes. [Machine-readable summary](data/2026-10-06-torc-cirrus/summary.json).
Both restored database archives pass SQLite's read-only `PRAGMA quick_check`.

Torc measured task durations of 3.54898 minutes (GNU) and 3.52888 minutes (Cray).
Its sampled peak memory reports are 4,404,703,232 and 2,653,552,640 bytes
respectively. These are task-monitor samples, not enforced caps or simultaneous
whole-node peak measurements. The independent MPI consumer was compiled here;
this stage does not claim that its distributed execution passed.

## Direct MPI runtime follow-up

GNU and Cray MPI smoke jobs `538084` and `538085` both completed `0:0`, in 60 and
57 seconds respectively. Each used two exclusive nodes in `short`, one rank per
node and 288 requested OpenMP threads. They used matching successful Torc build
receipts and direct `sbatch`/`srun`, not Torc multi-node workflows.

Both lanes passed:

- Distinct-host placement checks and the independent numerical MPI consumer at
  one and two ranks.
- Eight selected ABI/discovery tests, including actual compiled C/C++ request
  size/alignment mutations and runtime layout rejection. The compiled test's
  unavailable-wrapper skip branch was not taken. The other 55 tests were
  excluded by the declared filter.
- Four MPI smoke tests covering lifetime, threaded messaging, collective
  mismatch rejection and witnessed fatal cleanup. Two four-rank tests were
  deliberately excluded from this two-node allocation.
- Final source manifest checks.

Each fatal-cleanup test required durable per-rank pre-failure witnesses,
unsuccessful termination and no timeout. Its Slurm step shows `FAILED|127:0`
while the supervisor observes exit 255; neither numeric code alone establishes
correct failure handling. The enclosing test assertions passed. The smoke
receipt archive SHA-256 is
`3aa7df9d9cc6c58da11124188403536c899ece743ea382c4720534229df42a77`.

Full eight-node verification jobs `538140` (GNU) and `538141` (Cray) used
`lowpriority`, each with a one-hour limit. Cray had Slurm dependency
`afterany:538140`, preventing overlap. The full suites retain ordinary compile
failure tests and all MPI tests.

| Compiler | Job | Allocation result | All-feature nextest | Default-feature nextest |
| --- | --- | --- | --- | --- |
| GNU | 538140 | `COMPLETED`, `0:0`, 38m 30s | 1,706 passed, six skipped; 1,542.563s | 1,513 passed, one skipped; 508.440s |
| Cray | 538141 | `FAILED`, `1:0`, 31m 09s | 1,704 passed, two failed, six skipped; 1,181.249s | 1,511 passed, two failed, one skipped; 417.694s |

GNU ended at `2026-10-06T18:32:50Z`; Cray started one second later and ended at
`2026-10-06T19:04:00Z`. GNU's `completed.status`, `job-exit.status` and every stage
status are zero. Cray's two nextest stages returned 100; both final status
receipts are one. No rerun or skip was used to replace either result.

Both lanes passed:

- Distinct-host placement checks and independent numerical MPI consumers at
  one, two, four and eight ranks, with one rank per node.
- Eight selected ABI/discovery tests, including the compiled request-layout
  mutations; the unavailable-wrapper skip branch was not taken. The declared
  filter excluded 55 tests from this separate ABI stage.
- All-feature doctests: 65 passed, one ignored; default-feature doctests:
  55 passed, one ignored.
- Both workspace checks, strict all-target Clippy and formatting checks.
- All ten packaged CPU/OpenMP consumer cases, followed by the final source
  manifest check.

Cray's failures in both modes are the compile-fail groups
`invalid_dsl_and_semantic_capabilities_are_rejected` and
`runtime_ownership_and_kind_contracts`. Their nested trybuild dependency builds
invoke `cc` with `-fuse-ld=lld`; `rust-lld` rejects the Cray plugin options
`lto=0`, `defaults=cray` and `mllvm=-cray-math-precision=none`. This failure occurs
while linking dependency build scripts, before the intended compile-fail
diagnostics can be checked. It is a failed acceptance result, not an expected
negative-test outcome.

The all-feature suites also include witnessed fatal-path tests whose Slurm steps
deliberately fail. The distributed cleanup test passed in each lane after its
supervisor observed exit 255 without a timeout; steps `538140.239` and
`538141.239` recorded `FAILED|127:0`. The enclosing assertions, rather than those
numeric codes alone, establish the expected outcome.

Complete receipt and job-log archives were retrieved after each allocation
became terminal and passed gzip integrity checks. Their SHA-256 identities are:

- GNU: `f18bc29f327b5856aaf90985e75f50f30c01873c8511f26a603523a138d2c2ff`.
- Cray: `a40a82883d0dcedf68ff5f2ddc9835a807734c71348d96893a6cbc5c3c813a5e`.

Final Slurm accounting and stage receipts are recorded alongside the archives;
their hashes are in the [machine-readable summary](data/2026-10-06-torc-cirrus/summary.json).

The subsequent [native discovery corrections](2026-10-06-native-discovery-followup.md)
were implemented after this source was frozen. Those changes have separate local
regression and downstream evidence and are absent from these cluster jobs.

## Remaining gates

Multi-node Torc deployment remains unestablished. Its standalone local worker
does not admit the required multi-node workload; these build jobs are accurately
declared as one-node jobs. Direct documented Slurm runtime validation remains
available after matching builds succeed, with one MPI rank per node and 288
requested OpenMP threads. That route must be reported separately from Torc
multi-node execution.

macOS verification and the unsupported small per-node capacity enforcement gate
remain open. Full Cray compile-fail acceptance is also unmet. This workflow does
not turn a resource request, a running job or stored input generation into
numerical or capacity acceptance.
