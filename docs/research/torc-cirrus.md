# Torc for scientific campaign orchestration

## Decision and verification scope

Evaluate upstream Torc as the replacement for Python campaign orchestration.
Prefer a pinned external CLI and declarative workflow specifications. Keep
scientific validation and native/MPI ownership in the existing Rust crates;
do not reimplement Torc's scheduler inside `xtask` or embed its entire server.
New integration code must use safe Rust, explicit types and RAII, with native
QuEST unchanged. Unsupported deployment requirements are reported, without
private native interfaces, resource-control changes or patched vendor libraries.

This assessment inspected commit
`2b938514c48097ece201d5783541d2645c65cbee`. Its manifest declares package
version `0.41.0`, Rust 1.95, edition 2024 and BSD-3-Clause licensing. This is a
**source commit pin**, not a claim that this version has been released.
No Torc binary was installed, server started, workflow executed, or Cirrus job
submitted during this assessment. No Torc dependency was added to the workspace.
[Pinned manifest](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/Cargo.toml)

### Installed CLI follow-up

On 2026-10-06 the user supplied an installation under `$WORK/.local/bin`.
Live inspection reports Torc and its Slurm runner as version `0.41.0`:

| Executable | SHA-256 |
| --- | --- |
| `torc` | `ca7a5e6daa601a97a6f3cc7d6ac688ec18a015257b65dc277222bdf189e331ad` |
| `torc-server` | `2766f61186705f818e7770161bfa5ac59eecaf20a3bf47bc1534d55d9169221d` |
| `torc-slurm-job-runner` | `c13a76262064d3ea9e86bf1140ccebdcd3a7c11448821f9111baa1a9333fcfc1` |

The running client and server subsequently reported build `74076e1` and API
version `0.23.0`. The official `v0.41.0` tag resolves to
`74076e17c472418f956da02765e48043545fac1a`, matching that reported build identifier.
Comparison with the original assessment pin found the standalone lifecycle,
create/run/export, resource models and dependency lock unchanged. The later pin
only adds environment variables to workflow-action commands; this build recipe
has no workflow actions. The server does not
support `--version`. No configuration file or `TORC_API_URL` was present, and
`torc ping` failed to connect to its default loopback endpoint.

The [single-node build workflow](../verification/fixtures/cirrus/torc-native-build.yaml)
and [batch payload](../verification/fixtures/cirrus/torc-native-build.sbatch)
use the documented standalone mode. Live SQLite databases, worker journals and
logs use job-local `$TMPDIR`; after every standalone process returns, the
closed files are archived on EPCCFS. Create, run and export reopen the same
on-disk database. They do not use `--in-memory`, which would not preserve the
created workflow across those independent commands. Structured job/result
acceptance is separate from the CLI exit code and from the numerical receipts.
The declared 32 GB is scheduling metadata, not an enforced application memory
cap (`limit_resources: false`). No Torc multi-node server is deployed by this
recipe. [Execution evidence](../verification/2026-10-06-torc-cirrus.md) is recorded
separately from this configuration.
[Installed release source](https://github.com/NatLabRockies/torc/tree/74076e17c472418f956da02765e48043545fac1a)

## Responsibility boundary

| Concern | Proposed owner |
| --- | --- |
| Workflow DAG, job state, attempts and logs | Torc |
| Allocation and MPI placement | Documented Cirrus `sbatch`/`srun` configuration |
| Compiler, QuEST and central serial HDF5 selection | Existing reviewed module setup |
| Specification/provenance checking and numerical acceptance | Safe Rust tools and existing test crates |
| Qureg, communicator and prepared-oracle lifetimes | Existing Rust RAII owners |
| Historical receipts and independent reference fixtures | Preserve unchanged; do not reinterpret as Torc execution |

The public Rust library includes `WorkflowSpec`, `JobSpec`,
`ResourceRequirementsSpec`, `SlurmSchedulerSpec`, `ExecutionConfig`, `JobStatus`
and result models. However, its client feature brings a broad application
dependency graph; some CLI helpers terminate the process, and the standalone
server owner is private to the executable. The external CLI keeps those
lifecycles under Torc's implementation. Any later Rust adapter should use
argument arrays and typed result deserialization, and forbid unsafe Rust in
that adapter. Torc and the existing low-level FFI dependencies themselves are
not claimed to contain no unsafe implementation code.
[Client API](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client.rs),
[CLI lifecycle](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/main.rs)

## Cirrus configuration requirements

Keep exclusive nodes, one MPI rank per node, 288 requested OpenMP threads,
physical-core placement, `OMP_PLACES=cores` and `SRUN_CPUS_PER_TASK`. Use
`srun --hint=nomultithread --distribution=block:block --cpu-bind=cores
--kill-on-bad-exit=1` for the MPI payload. Builds use one node and a bounded
eight CPUs. This campaign uses account `d458` and partition `standard`.
Select exactly one of `short` (at most two nodes and twenty minutes) or
`lowpriority` per job, and keep total concurrent allocations within eight nodes.
Use central compiler-specific serial HDF5. Cirrus does not support
memory requests through `--mem` or `--mem-per-cpu`; exclusive allocations receive
full-node memory. These settings remain scheduler requirements, independent of
which workflow tool submits the job.
[Cirrus batch guide](https://docs.cirrus.ac.uk/user-guide/batch/)

At the inspected Torc revision:

| Route | Evidence and implication |
| --- | --- |
| Automatic Slurm scheduler generation | Populates scheduler memory. Do not use it for this Cirrus profile. |
| Native Slurm job wrapping | Adds `--mem` for named resource requirements and hardcodes `--ntasks=1`; it does not implement our required multi-rank placement. |
| Manually specified scheduler | Supports omitted `mem` and documented `extra` SBATCH options. Suitable configuration pieces exist. |
| Explicit `execution_config.mode: direct` | Leaves the application launcher in the command. This is Torc's documented explicit-MPI mode. |
| `limit_resources: false` in direct mode | Disables Torc's memory-limit enforcement; timeout and termination remain active. Slurm retains allocation ownership. It establishes no small application memory cap. |
| `start_one_worker_per_node: false` | Keeps one Torc coordinator instead of placing a Torc worker on every MPI node. |

[Native launcher](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client/async_cli_command.rs#L60-L138),
[automatic scheduler generation](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client/scheduler_plan.rs#L498-L518),
[manual scheduler fields](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client/commands/slurm.rs#L2225-L2263),
[explicit MPI execution](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/docs/src/specialized/hpc/multi-node-jobs.md)

The scheduler model's `ntasks_per_node` field is not propagated by the inspected
submission function. Its documented `extra` field can express the required
task and CPU directives. The original assessment requested a rendered script
before submission. The later pinned 0.41.0 review establishes that submission
writes the script and immediately invokes `sbatch`; there is no render-only
branch. The [current server-backed guide](../verification/fixtures/cirrus/torc-server.md)
uses the documented `slurm_defaults` map, source-derived review before submission
and retention of the actual generated script afterward. Model fields and a
source-derived illustration alone remain insufficient execution evidence.

Do not use `max_parallel_jobs` to disguise missing resource capacity: that
option switches to count-based claiming which ignores resource requirements.
Keep each MPI job's real `num_nodes` and CPU requirements. Scheduler-level
allocation serialization applies only within that scheduler; it is not a global
eight-node cap. Bound all concurrent GNU, Cray, build and runtime allocations
explicitly. Omit automatic resource-changing retries and do not invoke automatic
recovery or resource-correction commands for the fixed validation campaign.
[Concurrency semantics](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/docs/src/core/reference/resources.md#L90-L122),
[allocation chaining](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/docs/src/specialized/hpc/chained-allocations.md)

## Deployment prerequisites

`torc --standalone run` owns a loopback server and local worker. It is documented
for a single node. The inspected CLI supplies no node count, and its local
worker defaults to one node rather than discovering the full Slurm allocation.
It therefore cannot truthfully admit our multi-node jobs. Do not declare those
jobs as one-node tasks or bypass their resource admission.
[Standalone deployment](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/docs/src/specialized/hpc/self-contained-slurm-job.md),
[local capacity](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/run_jobs_cmd.rs#L272)

The documented multi-node route uses a regular Torc server and
`torc-slurm-job-runner`, whose capacity derives from the allocation. At the
original assessment, a permitted, reachable server deployment had not been
established, and authentication was treated as a deployment prerequisite.
The user subsequently supplied an HTTP server with authentication disabled.
The [current version-pinned proposal](../verification/fixtures/cirrus/torc-server.md)
uses that supplied configuration unchanged; it introduces no authentication or
approval prerequisite. Later evidence confirmed login- and compute-node API
reachability, followed by one successful server-backed Cray scaling workflow;
the [execution record](../verification/2026-10-06-scaling-only.md) separates its
structured Torc result from scientific receipts. NFS database/WAL reliability
remains unvalidated. The earlier
default-loopback failure is not a test of this supplied endpoint. Offline
draining only lets already-running jobs finish and does not provide serverless
scheduling.
[Slurm runner](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/bin/torc-slurm-job-runner.rs#L330),
[authentication](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/docs/src/specialized/admin/authentication.md)

The live server SQLite database should use supported local storage. Torc warns
against placing it on NFS/Lustre/GPFS. Its worker time-series metrics and offline
completion journals also use SQLite WAL under their output directory, so an
in-memory server alone does not resolve the storage requirement. Separate live
database locations from finalized artifacts on EPCCFS using supported deployment options. If those
options cannot satisfy the cluster environment, record the deployment as
unsupported rather than adding filesystem or service workarounds.
[Torc storage guidance](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/docs/src/specialized/hpc/hpc-deployment.md),
[metrics storage](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client/resource_monitor.rs#L1223),
[offline journal](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client/offline_journal.rs#L58)

Cirrus documents a job-specific, memory-resident `$TMPDIR`; its storage counts
against node memory. The completed standalone build jobs now verify its use for
the single-node database lifecycle, including exports, archival restoration and
SQLite integrity checks. The site documents Jupyter servers, but those
application-specific examples do not establish Torc service placement, lifetime
or worker-to-server API connectivity across nodes. A supported multi-node Torc
endpoint and its database lifecycle remain unestablished; this is not evidence
that all user services are prohibited.
[Cirrus temporary storage](https://docs.cirrus.ac.uk/user-guide/batch/#temporary-files-and-tmp-in-batch-jobs),
[documented Jupyter deployment](https://docs.cirrus.ac.uk/user-guide/python/#using-jupyterlab-on-cirrus)

## Acceptance and migration sequence

1. Establish the supported deployment and exact Torc executable identity.
2. Validate a single-node workflow using the documented standalone mode and
   supported database storage. Preserve the original specification and output.
3. Define manual GNU/Cray schedulers and direct MPI commands. Inspect the actual
   generated SBATCH/SRUN arguments, resource requirements and dependencies.
4. Validate one-/two-node execution before the already-required four-/eight-node
   campaign. Retain the existing Rust semantic tests and scientific receipts.
5. Replace active Python orchestration only after the corresponding Torc path
   is executed and verified. Preserve historical sources/receipts; do not claim
   they validate a later Torc deployment.

CLI exit success alone is insufficient. The inspected `torc run` discards a
returned worker result, and cancellation can report partial success in JSON.
Check structured job/result states, all relevant attempts, expected rank counts,
termination evidence and numerical receipts. Keep failed, canceled, terminated
and disabled outcomes distinct. Metadata export does not bundle actual log or
artifact bytes. Preserve those separately along with source/compiler/MPI/QuEST
identities. Torc adoption cannot close the currently unsupported small aggregate
memory-cap requirement.
[Run result handling](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/main.rs#L751),
[cancellation results](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client/commands/workflows.rs#L1844),
[export model](https://github.com/NatLabRockies/torc/blob/2b938514c48097ece201d5783541d2645c65cbee/src/client/commands/workflow_export.rs)
