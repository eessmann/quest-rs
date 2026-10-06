# Reviewed portable-native source and acceptance ledger

The source manifest `4e5b423ebf3db42db19c3f75481ad786a527117eecd4d41c8b9a9b9dcebd6cf1`
contains three further review corrections: HDF5 discovery finalization, an MPI
fatal-drop negative control, and safe process supervision. Their focused checks
passed. A successor containing only the corrected standalone-consumer lockfile
also passed the complete Cray build stage. Local GNU and Clang coverage is now
recorded across these two snapshots. Both corrected-source GNU and Cray cluster
runtime campaigns also passed. The remaining platform and capacity gates stay
open; each result applies to its recorded snapshot.

## Source and native identity

| Artifact | Identity |
| --- | --- |
| Source manifest, 2,594 files | `4e5b423ebf3db42db19c3f75481ad786a527117eecd4d41c8b9a9b9dcebd6cf1` |
| Source archive | `50b5a4a7727754f77282c2597d46e1cf6709bee8506ae63b5c7d05310a08b526` |
| Native QuEST revision | `503552065045eaf89baba85e6cd6aad728525554` |

All 2,594 frozen files and archive members were checked against the manifest.
The corrected files in the working tree match that frozen source. The manifest
and archive are retained under `target/portable-reviewed-cirrus-20261006`.
This ledger was written after the freeze and is not itself part of that archive.

The existing native installations remain unchanged. The
[GNU](2026-10-06-native-gnu.md), [Clang](2026-10-06-native-clang.md) and
[Cirrus](2026-10-06-torc-cirrus.md) records describe their configurations. The
[JSON summary](data/2026-10-06-portable-reviewed/summary.json) records native
library hashes separately from Rust source and evidence hashes.

## Review corrections and focused evidence

Successful pkg-config discovery may supply the selected HDF5 header and no
library search paths. The helper now follows locked `hdf5-metno-sys` 0.12.4
finalization: an empty search list is completed from the selected include
directory's parent, checking `lib` and `bin` for the platform library. Existing
nonempty paths retain their order. The regression selects the second of two
include directories, preserves spaces and verifies the Linux RUNPATH. It failed
before correction, then passed with the seven other HDF5 tests. All 63
`quest-build` tests and strict package Clippy passed. These discovery fixtures
do not establish HDF5 ABI or loader execution. The
[HDF5 follow-up](2026-10-06-native-discovery-followup.md#additional-hdf5-finalization-correction)
retains the diagnostic, intermediate test-expectation failure and exact commands.

The [fatal-drop fixture](../../crates/quest-sys/tests/mpi.rs) previously accepted
a child that returned from environment destruction and subsequently failed its
test. A nonzero launcher exit alone could therefore satisfy its assertion.
The corrected assertion requires each rank's pre-drop witness, absence of each
post-drop witness and a non-timeout failure. The negative control releases the
live register before dropping the environment, permits that valid drop to
return, writes the post-drop witnesses and deliberately fails the child test.
It then proves that the shared fatal-abort assertion rejects this outcome.
No native QuEST behavior is altered.

The negative control failed against the old assertion with Cargo exit 101.
After correction, both the real abort and returning-drop negative control passed
using two local MPI ranks. Their child exits were respectively 1 and 101;
neither timed out. The parent test result was two passed, five filtered, exit 0.
Strict Clippy passed. A concurrent attempt stopped at the supervisor's newly
enabled unsafe-code lint before these tests ran; its exit 101 and log are retained.

The [supervisor](../../crates/quest-test-support/src/mpi.rs) now uses the existing
locked `rustix` 1.1.5 dependency with `fs` and `process` features for nonblocking
pipe flags and process-group termination. `quest-test-support` forbids unsafe
code in its own library. The initial lint check rejected all three prior direct
`libc` unsafe blocks. The safe calls retain the owned child process group,
deadline, bounded capture and cancellation behavior; no custom launcher or
native-library patch was introduced.

All 17 supervisor integration tests passed after the change, followed by strict
Clippy. They exercise argument preservation, rank checks, output bounds, local
descendant cleanup and simulated Slurm cancellation/confirmation. Those Slurm
fixtures are local contract tests, not evidence of cluster execution. The initial
attribute-placement error and intermediate Clippy failure are also retained.

| Focused check | Before correction | Final result |
| --- | --- | --- |
| HDF5 selected-header fallback | Empty library paths; exit 101 | 8 HDF5 tests passed; 63 package tests passed |
| Fatal-drop negative control | Incorrectly accepted returning drop; exit 101 | 2 MPI tests passed; strict Clippy passed |
| Supervisor safe API migration | 3 unsafe-block lint errors; exit 101 | 17 tests passed; strict Clippy passed |

Raw focused captures remain under `target/portability-review-20261006`,
`target/fatal-witness-20261006-evidence` and
`target/safe-supervisor-20261006`. The JSON hashes the terminal logs and statuses;
private paths in raw compiler output are not copied into this public record.

## Combined-source campaigns and open gates

The following table separates completed local and cluster evidence from retained
earlier failures. The local runs used fresh Cargo targets
for the frozen source. Isolated loader checks remain separate from ordinary
consumer execution.

| Lane | Execution identity | Acceptance recorded here |
| --- | --- | --- |
| Local GNU | `target/native-reviewed-gnu-20261006` | Eleven stages passed on `4e5b423…`; original driver exit 1; corrected `452a498…` MPI consumer passed at 1/2/4/8 ranks |
| Local Clang | `target/native-reviewed-clang-20261006` | Eleven stages passed on `4e5b423…`; original driver exit 1; corrected `452a498…` MPI consumer passed at 1/2/4/8 ranks |
| Cirrus Cray build | Slurm `538212`, Torc standalone, one node/eight requested CPUs | Failed `1:0`; standalone MPI consumer compilation exited 101 |
| Cirrus Cray runtime | Slurm `538213`, ordinary eight-node validation payload | Cancelled while pending after build failure; runtime unrun |
| Cirrus Cray corrected runtime | Slurm `538231`, eight-node validation on `452a498…` | Completed `0:0`; all 25 stage/control statuses zero |
| Cirrus GNU corrected build | Slurm `538235`, Torc standalone on `452a498…` | Completed `0:0`; all eight native status files zero |
| Cirrus GNU corrected runtime | Slurm `538236`, eight-node validation on `452a498…` | Completed `0:0`; all 25 stage/control statuses zero |
| macOS Apple Silicon | No runner available | Unrun |
| Enforced small per-node capacity | Supported enforcement not established under the accepted execution policy | Unsupported; no capacity claim |

Both local compilers passed 1,710 all-feature tests with six skips and 1,516
default tests with one skip; doctests passed 65/55 with one ignored in each
mode. Both workspace checks, strict Clippy, binding freshness, native doctor
and normal/loader-isolated CPU/OpenMP consumers passed. Each original
twelve-stage driver nevertheless retains exit 1: the standalone MPI consumer
was rejected by `--locked` with exit 101 before compilation or execution.
The separate corrected-consumer invocations on `452a498…` passed at one, two,
four and eight local ranks, without timeout or truncated output. Source and
native-identity checks passed. All Rust and native source files are identical
between the two snapshots; the workspace suites were not rerun on the corrected
snapshot. The [GNU follow-up](2026-10-06-native-gnu.md#reviewed-source-follow-up)
and [Clang follow-up](2026-10-06-native-clang.md#reviewed-source-workspace-and-consumer-follow-up)
retain the original failures, corrected-consumer receipts and complete stage
details. This local coverage does not establish multi-node execution.

Cray build `538212` ran for three minutes 20 seconds. Native doctor, binding
freshness, default/all-feature workspace compilation and the final source check
passed, but the standalone MPI consumer's separate `Cargo.lock` had not been
updated for the supervisor's `rustix` dependency. Its locked Cargo build failed
with exit 101. The build is failed even though those preceding stages passed;
the dependent runtime job provides no execution evidence.

The terminal accounting and native/Torc receipts are retained in
`target/portable-reviewed-cirrus-20261006/build-receipts.tar.gz`, SHA-256
`cb4dada5479166e1503876cdc1535cd2e3d008f23757abe598f15a71c1bb4a9f`.
The standalone Torc run command returned zero, but its exported job and result
are both `failed`, return code 1, with matching workflow/job/run/attempt
identities. The wrapper's structured acceptance predicate returned false and
its exit status was 1. The CLI exit code alone is not acceptance evidence.

The consumer lockfile was subsequently refreshed offline. It adds the same
`rustix` 1.1.5, `errno` 0.3.14 and `linux-raw-sys` 0.12.1 packages already locked
by the workspace, and changes `quest-test-support`'s dependency from `libc` to
`rustix`. A locked, offline dependency-tree check passed. The corrected snapshot
has 2,594 files, manifest
`452a49852c4e23f018b82795c19a3fd183c781234f2914df2c0ab516d696918f`
and archive
`46636f8a311fa221d0754f4042a961e2191d756597ea2fe1ce97f50d1f2898a7`.
It differs from `4e5b423…` only in that standalone consumer lockfile. The
dependency-resolution check and the subsequent local consumer executions
described above are separate evidence. The local workspace results retain the
frozen `4e5b423…` identity, and corrected-consumer results retain `452a498…`.

Cray build `538230` completed on the corrected snapshot with Slurm exit `0:0`
in three minutes 51 seconds. All eight native status files are zero: doctor,
bindings, default/all-feature workspace compilation, standalone MPI consumer
compilation, source-after verification, completed and job-exit. Torc's exported
job and result are completed with return code zero and matching identities;
the acceptance predicate, run status and wrapper status also passed. The
archived local database passes read-only SQLite `quick_check`, and its compressed
archive passes integrity checking. This establishes compilation and discovery,
including the corrected consumer lockfile; it does not execute the consumer or
workspace tests.

The complete corrected-build receipt archive is
`target/portable-locked-cirrus-20261006/build-receipts.tar.gz`, SHA-256
`050ced85a0b533837c4c504547dc1c256c6c45dd35c476feecaef8b9cb04fec8`.
Its timestamped accounting observation at `2026-10-06T19:42:32Z` records:

| Corrected-snapshot job | Observed state | Dependency / scope |
| --- | --- | --- |
| Cray build `538230` | Completed `0:0` | One exclusive node, eight requested CPUs |
| Cray runtime `538231` | Running | Eight exclusive nodes; runtime acceptance pending |
| GNU build `538235` | Pending | `afterany:538231`; one node/eight requested CPUs |
| GNU runtime `538236` | Pending | `afterok:538235`; eight nodes |

These are retained observations rather than a claim about the jobs' current
state. The dependencies serialize the allocations within the eight-node limit.
The separately retained submission and approved cleanup receipt has SHA-256
`76c6a99bf2b1817fe8096d1988c86f9c018478379d1bef46d0f9e4043195cc37`.
Two obsolete compiled caches were removed under the owner's approval; source,
native installations, logs and verification receipts were retained.

Cray runtime `538231` subsequently completed successfully from
`2026-10-06T19:40:41Z` to `20:19:13Z`, taking 2,312 seconds on eight exclusive
nodes with 2,304 allocated CPUs. Every one of its 25 stage/control status files
is zero. The source manifest matches `452a498…`, and the before/after source and
native preflight checks passed.

| Cray runtime check | Result |
| --- | --- |
| Unfiltered all-feature workspace tests | 1,710 passed, 6 skipped; 1,491.031 seconds |
| Unfiltered default-feature workspace tests | 1,516 passed, 1 skipped; 514.687 seconds |
| All-feature / default doctests | 65 / 55 passed; one ignored in each mode |
| ABI stage | 8 passed; 58 excluded by the ABI-stage filter |
| Physical placement and independent MPI consumer | Passed at 1, 2, 4 and 8 ranks on matching distinct hosts |
| Both workspace checks, strict Clippy and formatting | Passed |
| Installed CPU/OpenMP consumers | Ten numerical cases passed in the module environment |

Both trybuild groups passed in both feature modes. The compiled ABI-mutation
regression also passed in the full workspace suite. These results retain the
reported skips and ignored doctests; the ABI filter is separate from the
unfiltered workspace invocations. Ordinary module-environment consumers do not
claim loader isolation.

The accounting contains 22 failed child steps. Each was matched to a passing
negative-test parent in the captured output. In particular, the returning-drop
negative control passed with child step `538231.239` exiting `101:0`, while the
live-resource fatal-drop test passed with step `538231.240` exiting `127:0`
and supervisor status 255. Neither timed out or truncated output. The negative
control establishes that a valid returning drop followed by deliberate failure
cannot satisfy the fatal-abort assertion; it does not report an incorrect native
return. Synthetic supervisor timeout tests remain distinct from the separate
real Slurm cancellation probe.

The complete Cray runtime receipt archive, including consumer artifacts, batch
log and source manifest, is
`target/portable-locked-cirrus-20261006/full-cray-538231-receipts.tar.gz`, SHA-256
`62af77e81e3d28bef3964d40227afd4501c519b3aac54ec74a3ef8a341df2838`.
Its gzip integrity check passed. The UTC accounting receipt has SHA-256
`6d2c1f4d457c126f5c6da4831fa96eabc00be431777bf213cb2fd57a25f99890`.
The JSON records stage statuses, log hashes and the failed-step mapping. These
results establish correctness for this compiler/source lane, not scaling or an
enforced per-node capacity limit.

GNU build `538235` completed with Slurm exit `0:0` from
`2026-10-06T20:19:15Z` to `20:23:02Z`, taking 227 seconds. All eight native
status files are zero, including the corrected standalone MPI consumer build
and source-after check. Torc's run and wrapper statuses are zero, its structured
acceptance is true, and the single exported completed job/result have matching
workflow, job, run and attempt identities with result code zero. Both compressed
archives passed integrity checks, and the restored database with its archived
WAL passed read-only SQLite `quick_check`.

The complete GNU build archive is
`target/portable-locked-cirrus-20261006/build-gnu-538235-receipts.tar.gz`, SHA-256
`5c6f9c9b6db4fcc8dbc696e023cf230589b22f55636ddf87640054effe0a8537`.
The separate UTC accounting receipt has SHA-256
`90ba02adaa975eda011dbd2999d02097b081ef9fb557e828109d395f5a931088`.
The dependent GNU runtime job `538236` began at `20:23:04Z`; its physical
placement, independent consumers at 1/2/4/8 ranks and ABI stage passed by the
`20:24:44Z` observation. Its workspace and remaining stages were still running
at that observation, so this build result does not establish runtime acceptance.

GNU runtime `538236` subsequently completed with Slurm exit `0:0` at
`2026-10-06T21:02:11Z`, after 2,347 seconds on eight exclusive nodes with 2,304
allocated CPUs. All 25 stage/control statuses are zero. The unfiltered
all-feature suite passed 1,710 tests with six skipped in 1,524.317 seconds;
the default suite passed 1,516 with one skipped in 511.237 seconds. Both
trybuild groups passed in both feature modes, and the compiled ABI-mutation
regression passed. Doctests passed 65/55 with one ignored each; the ABI stage
passed eight with 58 filter exclusions. Both workspace checks, strict Clippy,
formatting, all 1/2/4/8-rank placement and independent consumers, ten ordinary
CPU/OpenMP consumer cases and the final source check passed.

All 22 failed GNU child steps likewise map to passing negative-test parents.
The returning-drop control `538236.239` exited `101:0`, and the fatal-drop step
`538236.240` exited `127:0` with supervisor status 255. Both parent tests passed
without timeout or output truncation. The complete GNU runtime archive is
`target/portable-locked-cirrus-20261006/full-gnu-538236-receipts.tar.gz`, SHA-256
`f42834258a927c8533c50d1866c4b3742eb1b4cc8f47c0f9cd654ce4a2b1e438`.
Its gzip integrity check passed. The UTC accounting receipt has SHA-256
`e77bb23f524fa069c9efe3bccb412494b96f4bdc350c8e86d6eef91383e29388`.
The source manifest/archive and both native libraries still matched their
recorded hashes in the final `21:03:46Z` read-only identity check. The JSON also
records the retained GNU/Cray `xtask` and independent-consumer executable hashes.

The [concurrent binding check](2026-10-06-native-discovery-followup.md#concurrent-binding-discovery)
separately observed distinct discovery directories and normal RAII cleanup for
two direct GNU invocations sharing a Cargo target. The
[later scaling-only campaign](2026-10-06-scaling-only.md) has a separate source
identity and acceptance record. Neither this runtime pass nor the concurrency
check claims scaling performance or closes the capacity requirement.

Cirrus execution retains central GNU/Cray modules and serial HDF5, the existing
[batch payload](fixtures/cirrus/native-validation.sbatch) and the
[documented Cargo host policy](2026-10-06-cargo-host-policy.md). Torc standalone
coordinates the single-node build. Ordinary `sbatch`/`srun` performs multi-node
validation with exclusive nodes, one MPI rank per node, 288 OpenMP threads per
rank and the eight-node aggregate allocation limit. There is no Python campaign
controller, special trybuild route, custom memory cap or vendor-library patch.
The [accepted plan's ledger](../superpowers/plans/2026-10-06-portable-cirrus.md)
continues to define the complete acceptance requirements.

The earlier Clang follow-up's two CMake module-open failures remain recorded in
the [native-discovery report](2026-10-06-native-discovery-followup.md). Their cause
is unresolved; the new focused checks neither explain them nor replace that
failed campaign. The earlier Cray host-policy probe and build, and the owner
cancellations of runtime jobs `538207`/`538208`, likewise retain their
[separate evidence boundary](2026-10-06-cargo-host-policy.md#full-build-and-stopped-runtime-follow-up).
Historical numerical/scaling evidence does not establish the unavailable
per-node capacity gate. Full portable-native acceptance is not claimed.

## Current-source Slurm deadline and allocation reuse

Job `538261` exercised the safe supervisor from immutable source
`452a49852c4e23f018b82795c19a3fd183c781234f2914df2c0ab516d696918f`.
The [unchanged probe](fixtures/cirrus/supervisor-timeout.rs) was compiled with
ordinary locked/offline Cargo inside a two-node exclusive `short` allocation,
using a separate sidecar package and fresh target. It followed GNU validation
job `538236`, requested one task and 288 physical CPUs per node, and used the
maintained direct `srun` argument array. No Python controller, custom launcher,
GNU timeout wrapper or source modification was involved.

The parent job completed `0:0` in 23 seconds at `2026-10-06T21:02:36Z`.
All eight stage/control statuses are zero, two distinct physical hosts were
observed, and the source and sidecar checks passed before and after execution.
The probe requires a captured child-start witness before accepting a timeout;
a slow launch alone cannot satisfy that assertion.

The first owned step, `538261.1`, reached the five-second deadline after
5.135090937 seconds. Its cancellation was accepted and its termination confirmed.
Slurm preserves the expected `CANCELLED 0:9` result; the supervisor recorded
exit 137 with no output truncation. A distinct follow-up step, `538261.2`, then
completed `0:0` in the same allocation without a timeout or truncation. Total
probe time was 5.366 seconds, within the 20-second cleanup assertion.

This establishes real Slurm deadline handling, cancellation of the captured
owned step, confirmed termination and allocation reuse with the current safe
helper. The child commands are a shell sleep and `/bin/true`; they neither
initialize MPI nor execute QuEST, so this probe supplies no numerical MPI,
OpenMP kernel-team, scaling or capacity evidence. It was submitted once and was
not retried.

The complete archive is
`target/portable-locked-cirrus-20261006/supervisor-timeout-receipts.tar.gz`,
SHA256 `5c572bf3bdad215d5d5be5d23c8bbb97f114cc195ae2506ae7e9bca4b64eff42`.
Its compressed bytes, extracted sidecar manifest and terminal source/fixture
rechecks verified. The [structured timeout summary](data/2026-10-06-supervisor-timeout/summary.json)
records the source, sidecar, exact launcher arguments, accounting and artifact
hashes separately from the full-suite evidence above.
