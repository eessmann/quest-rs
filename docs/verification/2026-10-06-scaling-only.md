# Explicit scaling mode and Cirrus campaign

The sparse example now has an explicit scaling mode that observes existing
process limits without changing them. Six focused unit tests, strict example
Clippy and two local MPI correctness runs passed. GNU and Cray release builds
passed, and all six cases for each compiler passed their numerical, placement,
input and identity checks. GNU runtime used direct Slurm, while Cray runtime
used the supplied Torc server and one Slurm coordinator. Independent Torc export
acceptance also passed. This work does not close the capacity
requirement or establish a speedup.

## Immutable source and scope

| Artifact | SHA256 |
| --- | --- |
| Source manifest, 2,599 files | `ff4d5ce8215176f16e243e5dc7c733f46a6c2931ada4a3592a9a75d169cb797f` |
| Source archive | `e205959bc8d8f1c8a948538579c944a0dbc54e9e99e3e0a601bd0789ec29a155` |
| Parent manifest, 2,594 files | `452a49852c4e23f018b82795c19a3fd183c781234f2914df2c0ab516d696918f` |

All 2,599 frozen files and archive members match the source manifest. Comparison
with the [reviewed parent](2026-10-06-portable-reviewed.md) identifies exactly
11 changed or added paths: four example/documentation paths, six Cirrus fixture
paths and the existing acceptance plan. No paths were removed. Library source,
workspace manifests and lockfiles, the native QuEST installations and the safe
supervisor are unchanged from the parent. The JSON records every overlay path
and its hash. This report and its JSON were written after the freeze and are not
part of that archive.

The later [server-backed Torc guide](fixtures/cirrus/torc-server.md) and templates
are also outside this freeze. Their README link changes the working documentation
only; the queued jobs still use the original immutable `ff4d5ce…` fixtures.
At `2026-10-06T20:58:25Z`, a read-only inspection confirmed the supplied server
and successful login-node API ping/list, with no workflows present. Its reported
database directory is on NFSv3 with `local_lock=none`, which does not disable
normal remote locking. NFS database reliability remains unvalidated. Compute-node
health probe 538300 was submitted at `2026-10-06T21:00:58Z` with
`afterany:538288`, one exclusive node, one CPU and a two-minute `short` allocation.
It subsequently completed `0:0` in three seconds, with compute-node ping status 0.
The verified build and health probe precede Torc workflow 1's single Cray
allocation, job 538318, submitted at `2026-10-06T21:10:54Z`. Its retained generated
script has one bare coordinator, eight nodes, one task per node, 288 CPUs per
task, no memory request and `--no-requeue`. The JSON hashes these records. This
submission and health check are distinct from the later scientific results
below; the NFS reliability limitation remains explicit.

The [example contract](../../crates/quest/examples/sparse_capacity/README.md)
documents the new invocation:

```text
sparse_capacity --scaling OUTPUT N REPETITIONS MANAGED_RANK_BYTES MANAGED_NODE_BYTES NODES RANKS_PER_NODE THREADS STACK_BYTES
```

The producer creates only each rank's own COO shard. Managed application budgets,
native state and scratch admission remain in force. The process envelope adds
the measured address-space baseline and worker-stack allowance to the managed
rank budget; the node envelope sums actual co-located rank envelopes. These
modeled values are not an enforced whole-node cap or a measured whole-node peak.
Finite observed process limits must admit the envelope; unlimited observations
are represented explicitly. The example never sets a process limit.

Scaling receipts use schema 6, `evidence_kind: "scaling-only"` and
`capacity_closed: false`, including when an existing finite limit is observed.
The legacy positional mode retains its requirement for matching finite hard and
soft address-space limits. It still rejects an unlimited environment.

## Focused local evidence

The local lane used the existing installed GNU QuEST 4.3.0, matching system MPICH,
CPU execution and a reused GNU Cargo target. Native QuEST was unchanged. These
locked, offline example checks used the dev/test profile with debug information
and incremental compilation disabled:

| Check | Result |
| --- | --- |
| Example admission unit tests | 6 passed, exit 0 |
| Strict example Clippy | Final exit 0; initial exit 101 retained |
| Example build | Exit 0 |
| Scaling mode, dimension 16, two MPI ranks | Exit 0; both receipts accepted |
| Scaling mode, dimension 64, two MPI ranks | Exit 0; both receipts accepted |
| Legacy mode with unlimited limits, before and after | Both rejected with exit 1 |
| New flag against the old parser | Two-rank invocation rejected with exit 1 |

Both successful MPI runs used two ranks on one physical host, two requested
OpenMP threads per rank, three repeated round trips, 128 MiB managed per rank,
256 MiB managed per node and 8 MiB stack allowance per worker. Soft and hard
address-space limits were observed as unlimited before and after each run.
All four rank receipts reported finite norm and sample-error values within
`1e-10` of their acceptance bounds. Their retained input shard hashes match the
actual shard bytes. These small correctness runs establish neither multi-host
scaling nor a performance comparison.

The first Clippy attempt diagnosed a missing `const` qualifier and three redundant
borrows; the final strict check passed after correction. The first independent
jq receipt validator had a Boolean-binding precedence error. Its false results
are retained, and the corrected validator accepted the same unchanged raw
receipts. No scientific run was repeated to replace that validator failure.
The local manual MPI checks used a GNU `timeout` deadline; that command is
recorded as local evidence and is not the maintained Cirrus launch mechanism.

Raw local logs, statuses, commands, inputs and receipts remain under
`target/scaling-only-20261006`. All 35 entries in its evidence manifest verified,
and the three example source hashes match the frozen cluster source.

## Maintained cluster recipe

The [Cirrus recipe](fixtures/cirrus/README.md) adds explicit `scaling-build` and
`scaling` stages to the existing native validation payload. Existing stage
commands are preserved. The build uses a separate target namespace and compiles
only the release example with `--features mpi,qsvt-io`, locked and offline.
This release build is distinct from the earlier full-suite debug evidence.
The existing GNU/Cray module, central serial HDF5 and documented Cargo host/target
configuration remain in use.

The single-node build uses the existing Torc 0.41.0 standalone lifecycle: eight
CPUs, direct execution, resource limiting disabled, a fresh node-local database
and output directory, structured result validation, and an exit archive on
EPCCFS. The workflow's `32g` resource declaration is metadata, not an imposed
memory cap. No recovery or retry is enabled. The runtime is an ordinary Slurm
allocation with direct `srun` steps; no multi-node Torc service is needed by this
frozen recipe.

Each runtime requests eight exclusive nodes. Each sequential step activates
2, 4 or 8 nodes, with one MPI rank and 288 requested OpenMP threads per active
node. Physical-core binding, `--hint=nomultithread`, block distribution and
`--kill-on-bad-exit=1` are explicit. Each case has an eight-minute Slurm step
deadline. The retained receipt distinguishes the eight allocated nodes from
the active step nodes.

| Series | 2 active nodes | 4 active nodes | 8 active nodes |
| --- | --- | --- | --- |
| Fixed global work | N = 65,536 | N = 65,536 | N = 65,536 |
| Fixed work per node | N = 16,384 | N = 32,768 | N = 65,536 |

Every case requests three round trips, 4 GiB managed per rank/node and an 8 MiB
worker stack. The two eight-node cases are independent executions. No scheduler
memory option, process cap or replacement resource-control mechanism is added.
The [Cirrus resource rules](https://docs.cirrus.ac.uk/user-guide/batch/#resource-limits)
and [capacity assessment](2026-10-06-node-enforcement.md) explain why the required
small enforced aggregate node cap remains unsupported in this workflow.

The runtime requires a completed matching scaling-build receipt and compares
source, compiler/module profile, native library and release executable identity.
It checks source, native and executable bytes again after execution. Per-case
invocation JSON records the exact argument array and those identities. The
[receipt validator](fixtures/cirrus/scaling-report.jq) requires all ranks, distinct
physical hosts within the independently observed eight-node allocation, typed
unchanged process-limit observations, correct envelopes and input counts,
finite numerical/timing metrics and the declared case settings. Separate
SHA256 checks verify retained input shards against each rank's receipt.

Bash syntax, ShellCheck at warning severity and whitespace checks passed during
fixture preparation. Independent review found no remaining concrete blocker.
Six synthetic valid report sets were accepted and seven deliberately invalid
sets were rejected: null norm, null limits, duplicate host, wrong envelope,
missing stage, incomplete ranks and wrong source identity. These are validator
contract checks only; they supply no scientific, MPI, timing or cluster evidence.

## Submission history and completed GNU evidence

The source was deployed read-only under the generic campaign location
`$WORK/quest-validation/2026-10-06-scaling-only`. The recorded observation before
submission was `2026-10-06T20:27:36Z`, with 22 GiB available on EPCCFS.

| Job | Work | Requested allocation | Dependency | State at submission |
| --- | --- | --- | --- | --- |
| 538274 | GNU release example build through Torc standalone | 1 exclusive node, 8 CPUs, lowpriority, 2 h | `afterany:538261` | Pending |
| 538275 | GNU scaling cases | 8 exclusive nodes, 288 CPUs/rank, lowpriority, 1 h | `afterok:538274` | Pending |
| 538288 | Cray release example build through Torc standalone | 1 exclusive node, 8 CPUs, lowpriority, 2 h | `afterany:538275` | Pending |

The timeout probe 538261 itself follows the earlier eight-node GNU validation
job. This dependency chain keeps these allocations within the eight-node
aggregate limit. Pending scheduler exit fields of `0:0` are not completion
evidence. The separate Cray build submission completed with exit 0 and was
observed pending at `2026-10-06T20:42:16Z`. Its dependency keeps it after the GNU
scaling allocation. The Cray runtime was submitted later through the supplied
Torc server after both that build and connectivity probe 538300 succeeded.

Both GNU submissions succeeded before the submission script attempted an invalid
combined `scontrol show` query. That query printed `too many arguments for
keyword:show`, and the script exited 1 before reaching Cray submission. Separate
per-ID queries then confirmed both GNU jobs and their dependencies. Neither job
was resubmitted. The initial and corrected scripts are retained for provenance;
the corrected script is not evidence of another submission. Cray build 538288
was submitted later by a separate build-only script, with its own successful
log and status retained.

The submission archive SHA256 is
`dc2b33bf0b7c2fba630e0f0a4ee31d2d027fc3045e85b5d16de9875544d38e87`.
The later Cray build submission archive SHA256 is
`7dbab49670ee235275513abf2c48662ebbca8f6ea06c5caf671ba0cac0a76be1`.
The [JSON summary](data/2026-10-06-scaling-only/summary.json) hashes that archive,
the original failure log/status, the source overlay and focused evidence.
Raw scheduler output contains private paths and is represented here only by
generic fields and artifact hashes.

GNU build 538274 completed `0:0` in 95 seconds, and Cray build 538288 completed
`0:0` in 94 seconds. Both standalone Torc exports contain exactly one completed
`native-scaling-build` job and matching result with return code 0. GNU runtime
538275 completed `0:0` in 44 seconds, holding eight exclusive nodes from
`21:04:13Z` to `21:04:57Z`. All 42 archived build/runtime/Torc status files are
zero. All six frozen receipt predicates, all 28 actual input-shard hashes and
both structured Torc build acceptance records verified. Final source, native
library and executable checks passed.

The release executable SHA256 values are
`4388c94671c829d39e4d7811b220f3a343dd27f17032d73f6d32d7923189eb18` (GNU) and
`efb772532f397ff447a6d6dfc97d95fcc8e7b96d536c5b84da16b5c23a4275c6` (Cray).
The complete GNU/runtime and both-build archive SHA256 is
`2bc5a2d26c31db4130a347b86c4ab16dc6e3fd3df184a75f72e13c9e4f240fe3`;
all 286 entries in its retrieved-file index match their retained bytes.

The following GNU measurements are maximum rank-local elapsed seconds for each
phase. Execution covers three forward/adjoint round trips. Each case was run
once, with three internal repetitions; these are not statistical confidence
intervals or independent campaign replicates.

| Series | N | Active nodes | Preprocess | Persist | Load | Prepare | Execution |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Fixed work | 65,536 | 2 | 0.606739 | 0.103653 | 1.449643 | 0.227616 | 1.060181 |
| Fixed work | 65,536 | 4 | 0.602588 | 0.086450 | 2.009482 | 0.300513 | 1.465393 |
| Fixed work | 65,536 | 8 | 0.663180 | 0.047449 | 3.210672 | 0.317218 | 2.983074 |
| Fixed work/node | 16,384 | 2 | 0.148505 | 0.070357 | 0.324410 | 0.104436 | 0.249359 |
| Fixed work/node | 32,768 | 4 | 0.316705 | 0.027344 | 1.156431 | 0.140035 | 0.851897 |
| Fixed work/node | 65,536 | 8 | 0.661338 | 0.034454 | 3.208595 | 0.314751 | 2.979264 |

For this fixed-work case, execution and load times increased with node count.
No speedup or cause of that increase is inferred. The JSON retains individual
round-trip maxima, input-storage times, application communication counters,
per-rank high-water counters and sampled known-rank groups. It does not add
send and receive totals as though they were distinct payload bytes, or sum
independent node-local samples into a simultaneous cluster peak.

All 28 rank receipts have schema 6, `capacity_closed: false`, unchanged observed
unlimited process limits, the declared managed/stack envelopes and one rank on
each distinct active physical host. OpenMP was requested at 288 threads. Norms
and bounded amplitude-sample errors passed; the largest sample error was
`1.734723475976807e-18`. The complete allocation was independently observed as
eight nodes, while individual steps used the recorded subsets. GNU runtime used
direct `srun`; its success is not evidence of server-backed Torc execution.

Cray runtime 538318 subsequently completed `0:0` in 47 seconds, from `21:10:55Z`
to `21:11:42Z`, on eight exclusive nodes. All 26 runtime statuses are zero, all
six frozen receipt predicates passed and all 28 input-shard hashes matched.
Its source, native library and release executable identities are unchanged from
the matching build. The Cray runtime archive SHA256 is
`d34c150c53d841d4c1d44dc52d9a054390a91821df4c636a84f9b25a6d76fed5`.

| Cray series | N | Active nodes | Preprocess | Persist | Load | Prepare | Execution |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Fixed work | 65,536 | 2 | 0.619340 | 0.095784 | 1.500726 | 0.240998 | 1.093993 |
| Fixed work | 65,536 | 4 | 0.607625 | 0.077532 | 2.104249 | 0.306009 | 1.548492 |
| Fixed work | 65,536 | 8 | 0.706470 | 0.051367 | 3.434629 | 0.324461 | 3.272625 |
| Fixed work/node | 16,384 | 2 | 0.145131 | 0.056060 | 0.335567 | 0.106964 | 0.246941 |
| Fixed work/node | 32,768 | 4 | 0.340228 | 0.057735 | 1.247641 | 0.168082 | 0.947052 |
| Fixed work/node | 65,536 | 8 | 0.700986 | 0.062998 | 3.433694 | 0.323556 | 3.304442 |

The same timing and sampling scope applies to these Cray values. Norms were
`0.9999999999999999`; the maximum bounded sample error was again
`1.734723475976807e-18`. Observed process limits remained unlimited and unchanged,
and each active node had one rank with 288 requested OpenMP threads. The JSON
retains per-rank memory/process samples and application communication counts for
both compilers. Fixed-work execution and load times increased with node count in
both lanes; these runs do not establish a speedup.

The independent regular-server Torc export contains exactly one completed
`native-scaling` job and one completed result with return code 0. Workflow,
job, run and attempt IDs match, and the single inactive compute record reports
eight nodes/2,304 CPUs and allocation 538318. The actual server output archive
SHA256 is `76330a387ad76bac38f1e16e8af186e1079684181c6fbec13e6c7191fa1022f7`;
its retained generated script matches the reviewed bytes, and it includes the
runner, job and Slurm log bytes. The live server database was not copied.
The independent 56-file lifecycle index verified; all five result-referenced
logs are present, and the runner recorded no failures or terminations. Its
compute-record end time is the planned allocation deadline, so actual completion
is taken from Slurm accounting and the result timestamp `21:11:42.503Z`.

Initial read-only metadata queries used an unsupported `workflows status`
subcommand and omitted `--output-dir` for the deployed log directory. Their
diagnostics are retained, with no individual exit-code claim. The corrected
`workflows get` and explicit-output-directory result query succeeded. These
query corrections did not repeat a workflow or scientific job.

Whole-node cap enforcement, whole-node
peak memory, HugeTLB coverage and persistence/load wire-byte accounting remain
unmeasured or unsupported. Known-rank process sampling excludes other node
residents, and requested OpenMP threads do not establish each native kernel's
actual team size. No capacity closure or overall portable-native acceptance is
claimed by this record.
