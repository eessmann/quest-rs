# Torc 0.41.0 server-backed scaling

The regular-server route has now run the Cray scaling payload. Its six scientific
cases and independent Torc export acceptance passed.
Read-only
inspection at `2026-10-06T20:58:25Z` identified the server hostname, matching
server process and listener, successful login-node API ping and an empty workflow
list. The server's working-directory mount was observed as NFSv3 with
`local_lock=none`; database reliability on that mount remains unvalidated.
Compute-node connectivity probe 538300 was submitted at `2026-10-06T21:00:58Z`
after Cray build 538288. It completed `0:0` with ping status 0. Torc workflow 1
then submitted exactly one eight-node allocation, job 538318, which completed
`0:0` in 47 seconds. No endpoint was guessed or server changed. The
[scaling report](../../2026-10-06-scaling-only.md) separates these results from
the earlier standalone builds and preserves the NFS reliability limitation.

The guide targets the installed Torc and Slurm runner version `0.41.0`, whose
official tag is commit `74076e17c472418f956da02765e48043545fac1a`. The
[Slurm guide](https://github.com/NatLabRockies/torc/blob/v0.41.0/docs/src/specialized/hpc/slurm.md),
[explicit MPI execution](https://github.com/NatLabRockies/torc/blob/v0.41.0/docs/src/specialized/hpc/multi-node-jobs.md),
[workflow specification](https://github.com/NatLabRockies/torc/blob/v0.41.0/docs/src/core/reference/workflow-spec.md)
and [configuration reference](https://github.com/NatLabRockies/torc/blob/v0.41.0/docs/src/core/reference/configuration.md)
are pinned to that release. Recheck these semantics before changing versions.

## What the template submits

[torc-server-scaling.template.yaml](torc-server-scaling.template.yaml) declares
one real eight-node job, one scheduler and one allocation action. The coordinator
invokes the immutable `native-validation.sbatch` with `QUEST_STAGE=scaling` and
the matching successful release-build receipt. Its environment map records the
source/native identities, compiler and receipt paths in the workflow itself.
The current template selects Cray and the `ff4d5ce…` scaling snapshot described
in the [campaign record](../../2026-10-06-scaling-only.md). Replace every placeholder
with authoritative values before creating a workflow. Keep the immutable source
and successful build target unchanged.

`execution_config.mode: direct` preserves the authored MPI launch commands;
`limit_resources: false` adds no process memory cap. The action explicitly keeps
`start_one_worker_per_node: false`, so the generated batch script runs one bare
`torc-slurm-job-runner` coordinator. There is no outer `srun` around that runner.
The existing scaling payload owns its ordinary `srun` steps, one MPI rank per
active node and 288 requested OpenMP threads per rank. Central serial HDF5,
compiler modules, Cray Cargo host/target flags, physical placement and scientific
receipt checks remain in the immutable payload.

The documented `slurm_defaults` map supplies `ntasks`, `ntasks-per-node`,
`cpus-per-task`, `hint` and `export`. In 0.41.0 the scheduler's separate
`ntasks_per_node` model field is not propagated into the submission map. The
template therefore uses the supported defaults map. The scheduler supplies
eight exclusive nodes, account, `standard`, `lowpriority`, one-hour walltime
and an explicit dependency, with `--no-requeue` for the one-attempt campaign.
It omits `mem` and `mem-per-cpu`.
[Pinned scheduler construction](https://github.com/NatLabRockies/torc/blob/v0.41.0/src/client/commands/slurm.rs#L2229-L2280)

`memory: 32g` is per-node Torc admission metadata, not a Slurm memory request or
an imposed limit. The runner's observed allocation/system memory also supplies
admission metadata. The example's independent 4-GiB managed rank/node budgets
remain application budgets. Whole-node capacity enforcement stays open.

The pinned generator does not shell-quote the URL or output-directory arguments
of its runner command. Use a confirmed hostname, port and output path containing
no shell metacharacters or whitespace; preserve the `/torc-service/v1` API prefix.
The ordinary URL punctuation is required. This restriction comes from upstream
generation, not from a custom QuEST launcher.
[Pinned script generator](https://github.com/NatLabRockies/torc/blob/v0.41.0/src/client/hpc/slurm_interface.rs#L214-L326)

## Stage validation before creation and submission

First copy the YAML to a writable campaign file and substitute its account,
dependency, full digests and absolute shared paths. The required predecessor
must both establish build success and serialize this allocation with every
other admitted job. If those are separate jobs, use Slurm's documented combined
dependency expression. Keep aggregate allocation at eight nodes or fewer.
Do not submit this as an additional runtime beside an already queued direct
scaling allocation for the same evidence.

The following schema-only command is offline: `--skip-version-check` avoids
the CLI's otherwise normal version request, and `create --dry-run` returns
before server creation. It does not prove account, path or dependency validity.

```bash
torc --skip-version-check -f json create --dry-run - \
    < /path/to/scaling-server.yaml > /path/to/schema-offline.json
```

Use the confirmed endpoint of the user-provided server. Substitute these generic
values; the user-provided HTTP server currently has authentication disabled.
This guide does not change its authentication or start another service.

```bash
export QUEST_TORC_SERVER_HOST=CONFIRMED_HOSTNAME
export QUEST_TORC_SERVER_PORT=CONFIRMED_PORT
export TORC_API_URL="http://$QUEST_TORC_SERVER_HOST:$QUEST_TORC_SERVER_PORT/torc-service/v1"
export PATH="$WORK/.local/bin:$PATH"
torc ping
torc -f json workflows list --limit 1
```

Before server-backed scaling, use the
[connectivity payload](torc-server-connectivity.sbatch) for a separate one-node
check after the last admitted job. Set a new receipt directory and the actual
predecessor. The two-minute Slurm deadline bounds the HTTP client; no external
timeout wrapper is used.

```bash
export QUEST_VALIDATION_ROOT=/path/on/epccfs/scaling-campaign
export QUEST_CONNECTIVITY_RECEIPT="$QUEST_VALIDATION_ROOT/receipts/torc-connectivity-CHECK_ID"
mkdir -p "$QUEST_VALIDATION_ROOT/logs" "$QUEST_VALIDATION_ROOT/receipts"
sbatch --account=my-allocation --partition=standard --qos=short \
    --dependency="afterany:$QUEST_PREVIOUS_JOB_ID" \
    --exclusive --nodes=1 --ntasks=1 --cpus-per-task=1 --hint=nomultithread \
    --time=00:02:00 --export=ALL --chdir="$QUEST_VALIDATION_ROOT" \
    --output="$QUEST_VALIDATION_ROOT/logs/torc-connectivity-%j.log" \
    /path/to/torc-server-connectivity.sbatch
```

Retain terminal Slurm accounting, client version, endpoint, node name and the
ping log/status. An interrupted probe may have no final process-status file.
A successful result proves connectivity only from that allocated node. The
server must remain reachable for the actual workflow's lifetime.

Once compute-node connectivity and the matching release build succeed, and the
concrete dependency chain is established, ordinary client commands validate the
populated specification, show its execution plan, create it and submit the single
configured allocation. Retain the observed NFS reliability limitation; this guide
does not add a separate storage-durability proof or approval gate before an
ordinary attempt:

```bash
export TORC_CLIENT__SLURM__KEEP_SUBMISSION_SCRIPTS=true
sha256sum /path/to/scaling-server.yaml > /path/to/scaling-server.yaml.sha256
torc -f json create --dry-run /path/to/scaling-server.yaml > /path/to/schema-validation.json
torc -f json workflows execution-plan /path/to/scaling-server.yaml > /path/to/execution-plan.json
# Creation writes to the supplied server; submit invokes the configured Slurm action.
torc -f json create /path/to/scaling-server.yaml > /path/to/create.json
workflow_id=$(jq -er '.workflow_id | select(type == "number" and floor == . and . > 0)' /path/to/create.json)
torc submit "$workflow_id" --no-prompts --poll-interval 5 -o /path/to/torc-output
```

No separate `schedule-nodes` command is needed; the start action already requests
one allocation. Keep resource-aware claiming enabled: do not use
`--max-parallel-jobs` or `--start-one-worker-per-node` for this configuration.
The template enables no automatic resource correction or recovery.

Torc 0.41.0 writes the batch script and immediately invokes `sbatch`; it has no
render-only submission branch. `torc slurm generate --dry-run` previews scheduler
specifications and applies profile heuristics, not a batch-script preview of
this manual scheduler. Before submission, review the YAML and pinned generator
semantics. After submission, retain and inspect the actual generated script.
HashMap option order and workflow/PID fields prevent a byte-identical prediction.
No private renderer, fake `sbatch` or generator patch is part of this recipe.
[Submission implementation](https://github.com/NatLabRockies/torc/blob/v0.41.0/src/client/commands/slurm.rs#L2307-L2380)

After execution, export server metadata and results using ordinary commands:

```bash
torc workflows export "$workflow_id" --include-results --include-events -o /path/to/export.json
torc -f json workflows get "$workflow_id" > /path/to/workflow.json
torc -f json results list "$workflow_id" --include-logs \
    --output-dir /path/to/torc-output > /path/to/results.json
```

Preserve the actual runner/worker log bytes, generated script, scheduler IDs and
terminal accounting, exported job/result identities, and all immutable-payload
stage statuses, rank receipts and input hashes. Metadata paths alone do not
archive log bytes. CLI success alone does not prove backend or numerical success.
The completed matching result must return zero, and the existing scientific and
source/native/executable checks must all pass.
[Export and result commands](https://github.com/NatLabRockies/torc/blob/v0.41.0/docs/src/core/reference/cli.md)

## Storage and retained review evidence

Upstream requires suitable SQLite locking semantics and recommends server-local
database storage instead of a live database on a parallel filesystem. Only the
server opens its server database; workers communicate with that server over HTTP.
Enabled worker time-series scopes create SQLite/WAL files; draining to the
offline journal also opens WAL under the output directory. Record those locations
and the observed storage semantics without treating a successful short run as a
durability test. The live read-only inspection
identified the server working directory as `$WORK/torc`. The user-provided
`sqlite:torc-quest.db` setting resolves there and the matching file exists on
NFSv3 with `local_lock=none`. This is relative-path resolution evidence, not an
observation of an open database descriptor. As documented by `nfs(5)`,
`local_lock=none` means locks are not treated as client-local; it does not disable
normal remote locking. Torc advises avoiding NFS, but the reviewed implementation
does not categorically reject it. Successful HTTP reads establish neither
database durability nor corruption, and reliability on this mount remains
unvalidated. Ordinary output files are not themselves SQLite databases. Keep
monitoring, journaling and output locations unchanged unless a later explicit
deployment change is requested. This guide neither migrates
the supplied database nor invents an alternative service or storage mechanism.
[HPC deployment guidance](https://github.com/NatLabRockies/torc/blob/v0.41.0/docs/src/specialized/hpc/hpc-deployment.md),
[worker metrics](https://github.com/NatLabRockies/torc/blob/v0.41.0/src/client/resource_monitor.rs),
[offline journal](https://github.com/NatLabRockies/torc/blob/v0.41.0/src/client/offline_journal.rs)

The read-only packet is retained under `target/torc-server-20261006`. The original
validated template is historical. Before server submission, review added the
documented `--no-requeue` option to the maintained template and populated YAML;
both then passed fresh offline schema checks. The maintained connectivity payload
changes only the guide-reference comment from the reviewed two-minute packet
template, and those bytes were used by probe 538300. Its initial GNU-timeout draft
was never submitted. The packet's original expected batch script remains a
source-derived illustration; the actual script for 538318 is retained separately.

| Retained artifact | SHA256 |
| --- | --- |
| Original validated YAML template | `6b12f28cfec25bf15fac3ab6bb259bb0d0dacdd009cdbcb213f931b800260b2d` |
| Current template with `--no-requeue` | `01b65583fa1cb89a48d4b93e955f17f68217724600fb22ef3fcd114a911b26fd` |
| Current offline schema JSON, exit 0, no errors/warnings | `b8e83bac230af865187af0361bbb2bed48c437e3557caad1d0e6baa7630f32b5` |
| Corrected connectivity packet template | `d6145b5b0f591b1acefaa21da460de1088c6988f4c7077d4a5568dc0dabd7e4f` |
| Pinned `slurm_interface.rs` | `6897b4797585fc289ecefcb9f41a2ebb37dd40f15fd4ddce22a738e7fdb0d81e` |
| Pinned scheduler commands `slurm.rs` | `dbc1adbae49aa8c830625983d1de2c8de4a6ead04301bc9753c7851e3af475da` |
| Read-only login inspection log | `ada6aa5d7037d93dbd258f55ff4620acd115d09ecc5491810fb08fc55c8592a9` |
| Login inspection status, exit 0 | `9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa` |
| Connectivity probe 538300 submission log | `d37509f212d226f4f732752e2b678c7d6bc15d24f0acbec71ec0338f07f05fb0` |
| Connectivity submission status, exit 0 | `9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa` |
| Initial populated YAML, never created/submitted | `cb93d64b2d91b298c8d0d0cbb050833f230373ced70a8b7262f152661f9306b6` |
| Submitted populated YAML with `--no-requeue` | `213e81f8395b791aeae1f95520216f85fc2c75c14bcef9feb49a9822821d7dbf` |
| Populated workflow offline schema result, exit 0 | `aa5ef02f05fabfe649d6a0f6b8edcf9e35c7cdda34fc36917f76b3ca3dc7391b` |
| Actual generated script for job 538318 | `bea911f70e326ae508827bc1ba8eed698213f34a100fbeedbfa9ee2bd10b4a4f` |
| Completed compute connectivity archive | `deafa6ea4009ece4e481554424120ba3e8dadd9eeb03de31c153c789e01043bd` |

The populated workflow under the packet's `live` directory also passes the
offline schema check, with `afterok:538288:538300` requiring both the Cray build
and compute connectivity probe. It was created as workflow 1 and submitted at
`2026-10-06T21:10:54Z`. Its private paths are represented here by hashes only.
The retained actual script has one bare runner, no outer `srun`, no memory-request
flags, eight nodes, eight tasks, one task per node, 288 CPUs per task, exclusive
allocation, `standard`, `lowpriority` and `--no-requeue`. No render-only preview
was used or claimed. Schema acceptance remains distinct from this actual evidence.

The two generator files were fetched independently and match the clean release
checkout byte for byte. The schema result covers one job, one resource, one
scheduler and one allocation action; placeholder values are not cluster
admission evidence. The later read-only log/status are under the packet's `live`
directory; private hostname, process and path fields stay in that raw evidence.
Login- and compute-node reachability and actual allocation are now established.
Probe 538300 requested one exclusive node, one CPU and two minutes under `short`,
with `afterany:538288` and no requeue; it completed in three seconds. Runtime
538318 followed both that successful probe and build, retaining the eight-node
aggregate chain. Its scientific receipts passed; Torc workflow/job/result export
acceptance also passed independently: exactly one completed `native-scaling`
job/result with return code 0 and matching workflow, run and attempt IDs, plus
one inactive eight-node/2,304-CPU coordinator referencing allocation 538318.
The actual output archive SHA256 is
`76330a387ad76bac38f1e16e8af186e1079684181c6fbec13e6c7191fa1022f7`; it retains
the generated script and runner/job/Slurm log bytes. No live server database is
claimed as archived. Database/WAL durability remains unvalidated. The
`nfs(5)` interpretation was checked against the installed primary man page's
`local_lock` section (`/usr/share/man/man5/nfs.5.gz`). Capacity stays open.

The first read-only metadata query used an unsupported `workflows status`
subcommand and omitted the actual output directory for `results --include-logs`.
Those diagnostics are retained as a transcription of the tool output; their
individual exit codes were not captured. The corrected commands above returned
zero. These were metadata-query corrections, with no workflow, job or scientific
retry. The [campaign JSON](../../data/2026-10-06-scaling-only/summary.json) hashes
the exports, actual logs, diagnostics and scientific evidence separately.
