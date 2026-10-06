# Bounded pipe capture independent review

Status: APPROVED after independent frozen source review, twenty-five focused tests
and the authorized minimal eight-rank MPI initialization check. No source edits or
scientific consumer/source/synthesis jobs by this reviewer. Earlier file-cap campaign failure, diagnosis,
source manifests and supervisor approvals remain historical and unchanged.

## Diagnosis and design boundary

Read the retained diagnosis and the 432-byte C reproducer. It invokes only
MPI_Init_thread/FUNNELED, rank/size printing and MPI_Finalize. The retained eight-rank
strace evidence shows PSM3 ftruncate to 4,337,664 bytes receiving EFBIG under global
RLIMIT_FSIZE 4,194,304, then SIGXFSZ. Thus per-stream capture limits and a process-wide
file-size limit have materially different scope; using the latter as a capture cap
interferes with transport setup. The read-only evidence supports replacing only
that coupling, not increasing scientific/managed/AS/time limits or predicting that
the N32 consumer will succeed.

Parent-owned selector reads of at most 64 KiB, checking each retained stream length
before writing its prefix, can enforce the intended 4 MiB per stdout and stderr.
The fixed capture allowance is at most 8 MiB across the two output files, plus
bounded transient chunks/OS pipes. This does not bound MPI shared-memory files,
dataset storage, all child-created files or the filesystem as a whole. Existing
explicit dataset writer limits and hashes must remain separate.

## Final source/test gates sent to owner

- Exact cap followed by EOF is admitted; the next observed byte is an explicit
  overrun. An exit-zero child with over-limit output must not be marked completed.
- Drain both streams concurrently without unrestricted communicate() buffering.
  Stored prefixes must never exceed the limit, including before error handling.
- Preserve the blocked spawn/registration signal protocol. Initialize/select pipes
  safely before restoring TERM, or handle pending TERM at every setup point.
- After a child is spawned, every selector, file-write, timeout and signal error path
  must kill the active group and reap its direct child before clearing registration.
- Leader exit is distinct from pipe EOF. Descendants holding descriptors require a
  finite capture/cleanup deadline. Escaped-session descendants cannot be promised
  killed by launcher-group signaling: record incomplete closure, close bounded
  capture handles, and qualify process-group scope. Fake tests must explicitly clean
  up any intentionally escaped process they create.
- Preserve timeout, supervisor1500+10s grace, typed overrun/capture failures, partial
  byte counts/hashes and last atomic receipt. No retries, physical parameter changes
  or source/synthesis execution in this correction's focused validation.

Final approval will require source inspection, maintained fake-child tests and only
the separately authorized minimal MPI_Init/Finalize check, after owner freeze.

## Frozen implementation and independent verification

Reviewed collect() and its driver classification: retained bytes are checked before
writing at most the remaining prefix, reads are <=65,536 bytes, and both streams
are drained through one selector. Exact-cap EOF remains successful capture; any
observed excess sets output_limit before driver acceptance, regardless of child
exit status. Main prioritizes output-limit/timeout/incomplete-capture failures
before parsing successful rank receipts. There is no communicate() accumulation.

The selector is populated while TERM remains blocked, and the registered child is
available before unmasking. Setup/write/read exceptions pass through finally that
kills and waits for the direct child. Leader exit does not substitute for EOF;
remaining descriptors are bounded by job/cleanup deadlines, marked truncated when
closed without EOF. Escaped descendants are explicitly outside the group-termination
guarantee and the maintained synthetic test separately kills its known escaped PID.
Source/phase/dataset limits remain unchanged. The method explicitly says inherited
host limits still apply; `process_file_size_limit: null` denotes no runner-imposed
limit, not a measurement or guarantee that the ambient limit is infinite.

Independent command:
`python3 -B docs/verification/fixtures/persisted-weighted-transform/test_run.py`
passed 25 groups in 4.563 seconds. These include independent output streams,
exact-cap EOF, overrun prefixes, simultaneous drain, transport-like ftruncate,
post-spawn registration failure cleanup, held-open escaped descendants, job timeout,
external supervisor/spawn race, receipt-boundary interruption and full fake-driver
output-limit classification. Log `<private-artifacts>/quest-persisted-capture-independent-tests.log`,
SHA `d98e4ddb7e5217fd3a4d8467ddecdb8a1dd90214f29b605a4606dc74dfb53eeb`.
These are fake children and schema tests, not N32 numerical evidence.

The separate independent MPI check used the exact previously hashed C source and
binary from <private-artifacts>/quest-mpi-fsize-diagnostic and the frozen collector. It ran only
MPI_Init_thread/FUNNELED, rank/size printing and MPI_Finalize at eight local ranks.
All eight printed expected rank/size/provided values and returned zero, under 2 GiB
address-space, 4 MiB per captured stream, a stricter 30-second diagnostic timeout
and 65,536-byte reads. Captured stdout was 296 bytes, stderr 0, with neither timeout,
overrun nor incomplete EOF. Collector elapsed was 0.5012132929987274 seconds. The
parent's inherited RLIMIT_FSIZE was (-1,-1) on this host, separately recorded.
No QuEST, persisted-source generation, phase synthesis or native inverse executed.

Reproducer `<private-artifacts>/quest-persisted-capture-independent-mpi-init.py`; raw files and
receipt in `<private-artifacts>/quest-persisted-capture-independent-mpi-init/`. Receipt SHA:
`cd585881f376aaebaf4ff0cbf5973837231c8c45becf406b8b603278bd1c9205`.
Stdout SHA `d3a8da8d8ca5ebbe7c7a33e21faa06eda107ad81d0641d25b47db9a28461031e`.
MPI C source remains SHA
`7f53384f29909e7984d4d0bd1442513ba6a2a5fcfe397dab09a32dfa014bbb00`, executable
`e135610e233c6753c1ad5b41a9a5d43e0e61a2215e59e5538dd457b92ad37594`.

Final manifests verified after all checks:

- Python five-file capture manifest SHA
  `407fe4fc823887bec31912a0076a75547a3c18c7dd35b9c77e9102775df02b0e`.
- Complete fourteen-file consumer capture manifest SHA
  `fac76f206b3d7aca6e0b9c89e1b021c864084ed6c9724f617a35320d896ccdb1`.
- Runner before/after SHA
  `0313e0ba2ab937873d08012b6214ab442cc2a342bf1f786e547aa61fdb635e26`.

Read-only diff of root's private attempt2 wrapper confirms only the four exclusive
attempt path suffixes changed from1 to2; source/native/tool/build/scientific command
and timeout policies are otherwise the previously reviewed implementation. SHA is
`66596d054a6ef04506ef6e217c3645e1b6c26db588f6c8e7f99ea23e72e1b128`.
No wrapper build/campaign action was invoked by this reviewer.

No remaining actionable finding was established in the capture correction. This
approval closes only runner capture/transport-startup evidence; attempt1 failure
and all historical sources/receipts remain immutable. A second fixed N32 campaign
still requires root release and a new stable source/build pin. No feasibility,
convergence, inverse or multi-host acceptance follows from this minimal check.
