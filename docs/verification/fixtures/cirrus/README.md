# Cirrus source, build, and test campaign

These fixtures preserve source identity across GNU and Cray builds and run one
test coordinator outside MPI ranks. Every submitted job must request exclusive
nodes. Runtime steps use one MPI process per node, physical-core binding, and
OpenMP within that process. Select `short` for at most two nodes and twenty
minutes, or `lowpriority` for longer work and the eight-node test matrix. Use the
`standard` partition, the allocation's account, and no Slurm `--mem` option.

Use the [documented Cirrus submission workflow](https://docs.cirrus.ac.uk/user-guide/batch/#example-parallel-job-submission-scripts).
Keep the compiler and central HDF5 module environment for execution. If this
workflow cannot meet a requirement, record it as unsupported and continue;
do not introduce custom resource controls, linker patches or vendor-library
repackaging to make a check pass.

The [Torc assessment](../../../research/torc-cirrus.md) records the installed CLI
and the boundary between standalone and multi-node execution. Use the new
single-node recipe below for the current compiler builds. A multi-node Torc
workflow still needs an established server deployment. The
[version-pinned server-backed scaling guide](torc-server.md) provides the
reviewed workflow and bounded connectivity templates, with the recorded
compute-node check and subsequent Cray scaling allocation. The
supplied server's NFS database reliability remains an explicit unvalidated
limitation. Preserve the historical
Python fixtures and receipts; do not extend that controller or submit its
historical capped-memory job.

Keep sources, native installations, Cargo targets, receipts, and MPI witness
files on EPCCFS. The standalone recipes below keep Torc's live databases and
worker output in the site's job-local `$TMPDIR`, with completed artifacts
archived to EPCCFS. Examples use
generic paths and account names.

## Current standalone Torc build recipe

`torc-native-build.yaml` describes one real eight-CPU, one-node build task.
`torc-native-build.sbatch` owns its standalone server through the upstream CLI;
it uses standard `jq` to read exported JSON and requires exactly one completed
job and successful matching result. It archives the closed database and actual
log bytes separately from metadata. There are no automatic retries or resource
changes. This establishes one compiler build workflow, not a complete migration
of the multi-node campaign to Torc.

Prepare a fresh source archive and sorted SHA-256 manifest using standard
`tar` and `sha256sum`. The manifest must contain relative source paths; keep it
outside the source tree. Set `QUEST_SOURCE_DIGEST` to the manifest's SHA-256.
Verify the archive after transfer, unpack into a new source directory, verify
the manifest, and make that directory read-only before submitting. Retain the
original archive and manifest. The current recipe checks listed file contents;
unlike the historical Python verifier it does not certify file modes or reject
additional unlisted files.

```bash
export QUEST_VALIDATION_ROOT=/path/on/epccfs/new-validation
export QUEST_NATIVE_CAMPAIGN=/path/on/epccfs/verified-native-campaign
export QUEST_SOURCE=/path/on/epccfs/read-only-source
export QUEST_SOURCE_MANIFEST=/path/on/epccfs/source.sha256
export QUEST_SOURCE_DIGEST=sha256-of-source-manifest
export QUEST_COMPILER=gnu  # Submit a separate allocation for cray.
export QUEST_STAGE=build
export QUEST_NATIVE_SHA256=verified-library-sha256-for-this-compiler
export QUEST_TORC_BIN="$WORK/.local/bin/torc"
export QUEST_TORC_SERVER_BIN="$WORK/.local/bin/torc-server"
mkdir -p "$QUEST_VALIDATION_ROOT/logs"
sbatch --account=my-allocation --partition=standard --qos=lowpriority \
    --exclusive --nodes=1 --ntasks=1 --cpus-per-task=8 --hint=nomultithread \
    --time=02:00:00 --chdir="$QUEST_VALIDATION_ROOT" --export=ALL \
    --output="$QUEST_VALIDATION_ROOT/logs/torc-build-$QUEST_COMPILER-%j.log" \
    "$QUEST_SOURCE/docs/verification/fixtures/cirrus/torc-native-build.sbatch"
```

The build payload `native-validation.sbatch` selects compiler wrappers and the
central serial HDF5 module, checks the native library identity, and refuses an
existing Cargo target. It records source/module/compiler/flag identities and
builds default and all-feature tests plus an independent MPI consumer. Documented
Cargo profile variables disable debug information and incremental artifacts to
bound disk usage; debug assertions remain enabled.

The Cray profile configures host and target artifacts separately through
[Cargo's nightly host configuration](https://doc.rust-lang.org/cargo/reference/unstable.html#host-config).
Both use the `cc` wrapper, disable Rust's bundled LLD, and preserve subnormal
arithmetic with `-mno-daz-ftz`. These are compiler settings, applied to every
ordinary Cargo invocation, including nested compile-fail builds. Global
`RUSTFLAGS` alone do not configure build scripts when Cargo receives `--target`.
The profile records all host/target settings and checks their equality between
build and runtime receipts. It supplies generic host settings as well as leaves
for an existing architecture-specific host table; environment leaves alone do
not create that table. Existing flag values are retained within their respective
channels, and encoded Rust/rustdoc flags keep their normal precedence. Caller
flags can change nested test semantics; the tested profile has no caller-supplied
encoded flags. The [independent host-policy probe](../cargo-host-policy/README.md)
checks actual compiler arguments, domain separation, and subnormal execution.
Full workspace tests remain a separate gate.

For focused discovery changes, select `QUEST_STAGE=discovery` with a new source
manifest and fresh `targets/discovery/<compiler>/<digest>` target. The same standalone lifecycle then selects
`torc-native-discovery.yaml`: complete `quest-build` tests, strict package Clippy,
native doctor and binding freshness. Building `xtask` also exercises the locked
HDF5 dependency's header/library agreement against the central module. This
stage does not run the workspace tests or distributed execution. Its receipt
cannot satisfy the `build` prerequisite of `smoke` or `verify`; those require a
successful complete build at the same source digest. Check available storage
before submission and preserve existing targets and receipts.

Both payloads pass `bash -n` and ShellCheck's warning checks. The two local
ShellCheck exceptions on `QUEST_MPI_LAUNCHER_ARGS` document intentional JSON
serialization for the Rust supervisor; that value is never split into shell
arguments.

For later direct Slurm runtime checks, use that same payload with
`QUEST_STAGE=smoke` (two nodes, twenty minutes, `short`) or `verify` (eight nodes,
one hour, `lowpriority`). Set `QUEST_BUILD_RECEIPT` to the matching successful
`receipts/<compiler>/<build-job-id>` directory. Request one task per node and
288 CPUs per task. The Rust supervisor launches each test through `srun` using
its actual rank count; hostname steps first verify placement. Serialize
eight-node allocations across compilers. These direct runtime checks do not
establish Torc multi-node execution.

## Current scaling-only build and runtime stages

Freeze a new source containing the `sparse_capacity --scaling` mode and these
fixtures. Earlier immutable snapshots are not modified. Set the source, compiler,
native-library identity and Torc variables from the build recipe above, then
select `QUEST_STAGE=scaling-build`. The same standalone wrapper selects
`torc-native-scaling-build.yaml` and compiles only:

```bash
cargo build --release --locked --offline -p quest-rs \
    --features mpi,qsvt-io --example sparse_capacity
```

This stage uses a fresh `targets/scaling/<compiler>/<source-digest>` directory.
Its release profile is separate from the ordinary debug-profile workspace
evidence. Module/compiler settings, documented Cray host/target flags and
central serial HDF5 selection are shared with the maintained build profile.
It records the executable and native-library hashes and checks both identities
and the source manifest after compilation. The Torc workflow retains one
eight-CPU task, `mode: direct`, `limit_resources: false` and the existing
structured completion predicate. Its memory field is metadata, not an enforced
OS limit or a Slurm memory request.

Submit one compiler build after the preceding campaign job has ended; use its
actual job ID for `QUEST_PREVIOUS_JOB_ID`:

```bash
export QUEST_STAGE=scaling-build
sbatch --account=my-allocation --partition=standard --qos=lowpriority \
    --dependency="afterany:$QUEST_PREVIOUS_JOB_ID" \
    --exclusive --nodes=1 --ntasks=1 --cpus-per-task=8 --hint=nomultithread \
    --time=02:00:00 --chdir="$QUEST_VALIDATION_ROOT" --export=ALL \
    --output="$QUEST_VALIDATION_ROOT/logs/scaling-build-$QUEST_COMPILER-%j.log" \
    "$QUEST_SOURCE/docs/verification/fixtures/cirrus/torc-native-build.sbatch"
```

Set `QUEST_BUILD_JOB_ID` to that build's actual job ID, then submit the ordinary
Slurm scaling stage with a successful-build dependency:

```bash
export QUEST_STAGE=scaling
export QUEST_BUILD_RECEIPT="$QUEST_VALIDATION_ROOT/receipts/$QUEST_COMPILER/$QUEST_BUILD_JOB_ID"
sbatch --account=my-allocation --partition=standard --qos=lowpriority \
    --dependency="afterok:$QUEST_BUILD_JOB_ID" \
    --exclusive --nodes=8 --ntasks=8 --ntasks-per-node=1 \
    --cpus-per-task=288 --hint=nomultithread --time=01:00:00 \
    --chdir="$QUEST_VALIDATION_ROOT" --export=ALL \
    --output="$QUEST_VALIDATION_ROOT/logs/scaling-$QUEST_COMPILER-%j.log" \
    "$QUEST_SOURCE/docs/verification/fixtures/cirrus/native-validation.sbatch"
```

Make the next compiler's build depend on completion of this runtime job. These
dependencies keep the campaign at eight allocated nodes or fewer, including
build allocations. Review storage before each fresh target; preserve prior
source archives, native installations, logs and receipts.

The allocation holds eight exclusive nodes throughout. `native-scaling.sh` runs
six sequential steps with two, four and eight active nodes, one MPI rank per
node and 288 requested OpenMP threads per rank:

| Active step nodes | Strong-scaling dimension | Weak-scaling dimension |
| --- | --- | --- |
| 2 | 65,536 | 16,384 |
| 4 | 65,536 | 32,768 |
| 8 | 65,536 | 65,536 |

Each case executes three forward/adjoint round trips, with managed rank/node
budgets of 4 GiB and an 8 MiB OpenMP worker stack. The two eight-node cases are
independent executions. A one-minute hostname step verifies the complete
allocation; each data step has an eight-minute Slurm limit. Rank receipts must
show the requested number of distinct hosts drawn from that allocation and
one rank per shared-memory node. Different steps may use different subsets.

Each case retains an `invocation.json` with typed inputs, eight allocated nodes,
the active step count, exact argument vector, compiler/release profile and
source/executable/native hashes. The raw rank JSON is preserved. The
`scaling-report.jq` predicate checks schema 6, complete rank coverage, actual
placement, finite numerical results and timings, source byte counts, managed
envelopes, stack accounting, communication metrics and unchanged observed
process limits. Ordinary `sha256sum` verifies retained input shards against
their reported hashes. Final executable/native/source checks and individual
command/status receipts remain required for a successful scaling stage.

This is scaling-only evidence: `capacity_closed` must remain false and the
whole-node enforced-cap and peak fields remain null. Managed admission budgets
are not OS memory enforcement; the profile installs no custom limit. Requested
OpenMP threads are not a measured native team size. Rank-local sampled memory
and communication payload counts do not establish whole-node peaks or all MPI
wire traffic. Unsupported enforcement and the capacity gate remain open.

## Historical source-freezing procedure

The remaining sections preserve the earlier campaign's reproduction procedures
and explain its receipts. They are historical, not the submission path for new
campaigns. In particular, the Python controller, special Cray UI routing and
custom memory enforcement are superseded by the current recipe above and are
not recommended under the documented-tools-only execution policy.

From the local checkout, create an archive outside the checkout:

```sh
python3 docs/verification/fixtures/cirrus/snapshot.py \
    --root . --output /path/to/snapshot-archives
```

The JSON receipt contains the source digest and the compressed archive's SHA256.
Transfer the archive unchanged, compare its SHA256 with that receipt, and unpack
it into a fresh directory. The archive contains a `source/` directory:

```sh
export QUEST_CAMPAIGN_ROOT=/path/on/epccfs/quest-campaign
export QUEST_SOURCE_DIGEST=source-digest-from-the-receipt
export QUEST_ARCHIVE=/path/on/epccfs/source-archive.tar.gz
mkdir -p "$QUEST_CAMPAIGN_ROOT/sources/$QUEST_SOURCE_DIGEST"
tar -xzf "$QUEST_ARCHIVE" -C "$QUEST_CAMPAIGN_ROOT/sources/$QUEST_SOURCE_DIGEST"
export QUEST_SNAPSHOT="$QUEST_CAMPAIGN_ROOT/sources/$QUEST_SOURCE_DIGEST/source"
export QUEST_FIXTURES="$QUEST_SNAPSHOT/docs/verification/fixtures/cirrus"
python3 "$QUEST_FIXTURES/snapshot.py" --verify --root "$QUEST_SNAPSHOT"
chmod -R a-w "$QUEST_SNAPSHOT"
```

Verification checks the manifest digest, complete file inventory, file sizes,
contents, executable bits, and absence of symlinks. Removing write permission
does not change source identity. Added files and generated files inside the
source tree are rejected. Builds and tests use external target and temporary
directories.

## Historical native and Rust builds

Set the native checkout and exact expected revision before invoking
`build-native.sh`. That script checks the revision and tracked worktree state,
then builds separate GNU and Cray installations under `native/gnu` and
`native/cray`. Submit the script as one exclusive allocation; choose a QoS and
time limit sufficient for both native builds.

Both Cray profiles unload `cray-libsci` while retaining QuEST's OpenMP support.
The wrapper otherwise adds threaded LibSci, whose startup makes subsequent
OpenMP entry from a C pthread or Rust test worker fail with CCE's
`OpenMP parallel attempted from non-OpenMP thread` diagnostic. QuEST does not
use LibSci's scientific routines. Removing the module is supported by the
[HPE module documentation](https://support.hpe.com/hpesc/public/docDisplay?docId=a00114930en_us&page=Configure_the_Development_Environment_with_Modules_HPE.html).
A native prefix already linked with LibSci retains that dependency: rebuild
the unchanged native source under a fresh campaign directory and retain the
earlier installation and receipts.

```sh
export QUEST_SLURM_ACCOUNT=my-allocation
export QUEST_NATIVE_SOURCE=/path/on/epccfs/unchanged-quest-source
export QUEST_NATIVE_REVISION=expected-native-commit
mkdir -p "$QUEST_CAMPAIGN_ROOT/logs"
sbatch --account="$QUEST_SLURM_ACCOUNT" --partition=standard --qos=lowpriority \
    --chdir="$QUEST_CAMPAIGN_ROOT" \
    --exclusive --nodes=1 --ntasks=1 --cpus-per-task=8 --time=01:00:00 \
    --output="$QUEST_CAMPAIGN_ROOT/logs/native-%j.log" \
    --export=ALL "$QUEST_FIXTURES/build-native.sh"
```

After the native build succeeds, build each compiler profile separately. Do not
overlap builds using the same compiler and source digest.

```sh
export QUEST_COMPILER=gnu  # Repeat with cray.
sbatch --account="$QUEST_SLURM_ACCOUNT" --partition=standard --qos=lowpriority \
    --chdir="$QUEST_CAMPAIGN_ROOT" \
    --exclusive --nodes=1 --ntasks=1 --cpus-per-task=8 --time=02:00:00 \
    --output="$QUEST_CAMPAIGN_ROOT/logs/build-$QUEST_COMPILER-%j.log" \
    --export=ALL "$QUEST_FIXTURES/build-rust.sh"
```

`runtime-env.sh` selects the compiler environment and central serial `cray-hdf5`
module. Cray uses `-C linker-features=-lld` so its compiler wrapper reaches the
native linker and `-C link-arg=-mno-daz-ftz` to preserve gradual underflow at
program startup. The latter prevents the wrapper's default `crtfastmath.o` from
enabling FTZ/DAZ, which the numerical admission checks reject. This link policy
leaves the native QuEST installation unchanged; it follows the
[Clang startup-control documentation](https://clang.llvm.org/docs/UsersManual.html#a-note-about-crtfastmath-o).
The Cray policy is applied separately to `CARGO_ENCODED_RUSTFLAGS` and
`CARGO_ENCODED_RUSTDOCFLAGS`: Cargo gives rustdoc its own
[flag environment](https://doc.rust-lang.org/cargo/reference/config.html#buildrustdocflags),
so compiler flags alone do not configure doctest linking. Each encoded variable
retains precedence over its corresponding `RUSTFLAGS` or `RUSTDOCFLAGS` value;
the GNU profile retains both original flag families. Cargo build parallelism is
capped at eight jobs. The immutable profile records both effective flag lists.

With the matching runtime profile selected, check documentation examples under
both feature selections:

```sh
cargo test --locked --offline --workspace --doc -- --test-threads=1
cargo test --locked --offline --workspace --all-features --doc -- --test-threads=1
```

Targets live under `targets/<compiler>/<source-digest>`. This separation is
required because archived source timestamps are normalized. An immutable build
profile in each target directory records module/compiler/rustc identity, native
revision, and relevant flags and environment. A changed profile is rejected;
use a separate campaign directory for a different profile.

Each attempt prints a fresh receipt directory under
`receipts/<compiler>/job-<job-id>.<suffix>`. `build-complete` contains the verified
source digest and exists only after every build check and final source
verification succeed. Receipts include the native prefix, Cargo target directory,
compiler profile, Rust flags, source manifest, and script hashes. Slurm's spool
copy of the executed script is hashed; helper scripts are found in the shared
snapshot. `QUEST_SNAPSHOT_VERIFIER` can explicitly select an external verifier
for an older pilot recipe, with its hash retained in the receipt.

## Historical smoke and workspace driver

Set `QUEST_BUILD_RECEIPT` to the successful receipt from this exact snapshot and
compiler. The coordinator checks its digest, manifest, native prefix, target
directory, and current compiler profile before running tests. It requires the
current receipt format; an older pilot marker is insufficient.

Set `QUEST_PHYSICAL_CPUS` to the verified physical core count of the selected node
type. The current campaign's runtime allocation uses 288 physical cores per node.

```sh
export QUEST_BUILD_RECEIPT="$QUEST_CAMPAIGN_ROOT/receipts/$QUEST_COMPILER/job-build-id.suffix"
export QUEST_PHYSICAL_CPUS=288
export QUEST_TEST_STAGE=smoke
sbatch --account="$QUEST_SLURM_ACCOUNT" --partition=standard --qos=short \
    --chdir="$QUEST_CAMPAIGN_ROOT" \
    --exclusive --nodes=2 --ntasks=2 --ntasks-per-node=1 \
    --cpus-per-task="$QUEST_PHYSICAL_CPUS" --hint=nomultithread --time=00:20:00 \
    --output="$QUEST_CAMPAIGN_ROOT/logs/smoke-$QUEST_COMPILER-%j.log" \
    --export=ALL "$QUEST_FIXTURES/test-rust.sh"

export QUEST_TEST_STAGE=workspace
sbatch --account="$QUEST_SLURM_ACCOUNT" --partition=standard --qos=lowpriority \
    --chdir="$QUEST_CAMPAIGN_ROOT" \
    --exclusive --nodes=8 --ntasks=8 --ntasks-per-node=1 \
    --cpus-per-task="$QUEST_PHYSICAL_CPUS" --hint=nomultithread --time=02:00:00 \
    --output="$QUEST_CAMPAIGN_ROOT/logs/tests-$QUEST_COMPILER-%j.log" \
    --export=ALL "$QUEST_FIXTURES/test-rust.sh"
```

The smoke stage selects five existing 1/2-rank MPI lifecycle, messaging,
collective-rejection, fatal-cleanup, and application-result tests. The workspace
stage runs `cargo nextest run --locked --offline --workspace --all-features
--test-threads=1 --no-fail-fast --success-output=immediate` and requires eight
nodes for its 1/2/4/8-rank matrix. Workspace doctests run separately through
`cargo test --locked --offline --workspace --all-features --doc -- --test-threads=1`,
including when Nextest fails. Smoke cases use exact Nextest filter expressions.
A smaller
allocation must explicitly select the smoke stage. Neither stage silently
reduces a test's requested rank count.

The coordinator runs once as the batch script, outside `srun`. Each supervised
child executes `srun` with `--nodes=P`, `--ntasks=P`, `--ntasks-per-node=1`,
`--distribution=block:block`, `--kill-on-bad-exit=1`, physical-core binding, and
the allocated CPUs per task. Inherited task-per-node settings are cleared.
`OMP_NUM_THREADS` and `SRUN_CPUS_PER_TASK` match that allocation, and
`OMP_PLACES=cores`. Existing tests that explicitly disable native threading
continue to test that behavior; OpenMP environment settings do not override the
QuEST environment builder. The capacity fixture separately admits enabled native
threading.

Receipts live under `test-receipts/<compiler>/job-<job-id>.<suffix>`. `test-stage`
records the selected scope, `tests.log` retains Nextest diagnostics and passing
test output, and `doctests.log` separately records workspace doctests.
`nextest-status` and `doctest-status` retain their exit results. The
`tests-complete` contains the source digest only after that stage and final source
verification succeed. `status` retains the last stage and exit status for failures
after test admission. Shared temporary files are retained under
`tmp/<compiler>/job-<job-id>.<suffix>` for cross-node witnesses and diagnostics.

Passing the smoke or workspace stage does not close capacity or scaling claims.
Those require the separate capacity campaign's completed execution, actual
placement and memory evidence, and `completion.json`. Raw cluster receipts can
contain local installation paths; replace private prefixes before checking them
into public documentation.

## Historical Cray native UI profile

The earlier Cray workspace driver routed the `compile_fail` binaries
from `quest-compile` and `quest-rs` through a separate native UI profile. This
profile is no longer the accepted campaign route. The current
`native-validation.sbatch` runs ordinary unfiltered workspace suites and retains
any failures; it does not invoke `native_ui.py` or `trybuild_no_target`. Both
binaries and all three harness tests still run, including unchanged expected
compiler diagnostics. The base Nextest command excludes only those two binaries;
`native-ui.log`, `native-ui-status`, and `test-routing.json` retain the routed test
count, base skipped count, separate results, and exact filter. Any base, UI, or
doctest failure prevents the stage's completion marker. GNU and cross-target
workspace commands keep their ordinary UI path.

[Cargo's rustflags policy](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags)
withholds target flags from host build scripts and procedural macros when an
explicit target is selected, even if that target equals the host. The pinned
[trybuild 1.0.121 implementation](https://raw.githubusercontent.com/dtolnay/trybuild/1.0.121/src/cargo.rs)
provides `trybuild_no_target` for this native case. The scoped profile preserves
Cray's linker flags and adds that cfg plus trybuild's diagnostic defaults:
`--cfg trybuild --verbose -A dead_code --diagnostic-width=140`. Its unique Cargo
target directory and recorded flags are separate from the admitted base build.
The ordinary cross-target policy and expected `.stderr` files remain unchanged.

`native_ui.py` queries nightly Cargo's merged configuration in memory and records
only target selection. It verifies the selected target against `rustc -vV`.
Unknown or multiple targets cannot enter native UI mode. An explicit
`CARGO_BUILD_TARGET` equal to the host is removed only inside the UI subprocess.
A nonempty configured `build.target` must be removed before selecting the native
UI stage, since it would still suppress the required host flags. No unrelated
Cargo configuration is printed or retained. `TRYBUILD=overwrite` is rejected.

For the historical focused reproduction, the following recipe used the same
verified build receipt and one exclusive node. It selected
`QUEST_UI_FEATURES=default` or `all`; the historical workspace stage selected
`all`:

```sh
export QUEST_COMPILER=cray
export QUEST_TEST_STAGE=native-ui
export QUEST_UI_FEATURES=default  # Repeat with all.
sbatch --account="$QUEST_SLURM_ACCOUNT" --partition=standard --qos=lowpriority \
    --chdir="$QUEST_CAMPAIGN_ROOT" \
    --exclusive --nodes=1 --ntasks=1 --cpus-per-task=8 --hint=nomultithread \
    --time=00:30:00 --output="$QUEST_CAMPAIGN_ROOT/logs/native-ui-%j.log" \
    --export=ALL "$QUEST_FIXTURES/test-rust.sh"
```

For a recorded pilot against an older immutable snapshot, `QUEST_NATIVE_UI_HELPER`
can name a separately transferred helper; its SHA256 is recorded alongside the
executed coordinator hash. Such a pilot establishes the named routing profile,
not a clean rerun of the original workspace command.

## Historical real-step timeout probe

`supervisor-timeout.rs` launches a two-node child with a five-second deadline,
requires evidence that the child started, and checks both accepted cancellation
and scheduler-confirmed disappearance of its owned step. It then launches a
different step in the same allocation to prove that the allocation survived.
The following GNU recipe builds a standalone Cargo package with a path dependency
on the selected snapshot's `quest-test-support`, matching the successful cluster
probe. Keep the probe package and its target directory outside the snapshot.

After setting the campaign, snapshot, source digest, account, and physical-core
variables above, create this batch script on EPCCFS:

```sh
mkdir -p "$QUEST_CAMPAIGN_ROOT/probes/supervisor-timeout"
cat > "$QUEST_CAMPAIGN_ROOT/probes/supervisor-timeout/run.sh" <<'SH'
#!/bin/bash -l
set -euo pipefail
: "${QUEST_CAMPAIGN_ROOT:?}" "${QUEST_SOURCE_DIGEST:?}" "${QUEST_SNAPSHOT:?}"
export QUEST_COMPILER=gnu
quest_helpers="$QUEST_SNAPSHOT/docs/verification/fixtures/cirrus"
test "$(python3 "$quest_helpers/snapshot.py" --verify --root "$QUEST_SNAPSHOT" --digest-only)" = "$QUEST_SOURCE_DIGEST"
source "$quest_helpers/runtime-env.sh"
quest_receipt=$(mktemp -d "$QUEST_CAMPAIGN_ROOT/probes/supervisor-timeout/job-$SLURM_JOB_ID.XXXXXXXX")
export TMPDIR="$quest_receipt/tmp"
mkdir "$TMPDIR"
sha256sum "$quest_helpers/supervisor-timeout.rs" > "$quest_receipt/source.sha256"
sha256sum "${BASH_SOURCE[0]}" > "$quest_receipt/recipe.sha256"
python3 - "$quest_receipt" "$QUEST_SNAPSHOT" <<'PY'
import json
import shutil
import sys
from pathlib import Path

receipt, snapshot = map(Path, sys.argv[1:])
(receipt / 'src').mkdir()
shutil.copyfile(snapshot / 'docs/verification/fixtures/cirrus/supervisor-timeout.rs',
                receipt / 'src/main.rs')
shutil.copyfile(snapshot / 'Cargo.lock', receipt / 'Cargo.lock')
dependency = json.dumps(str(snapshot / 'crates/quest-test-support'))
(receipt / 'Cargo.toml').write_text(
    '[package]\nname="supervisor-timeout-probe"\nversion="0.1.0"\nedition="2024"\n'
    '[workspace]\n[dependencies]\nquest-test-support={path=' + dependency + '}\n')
PY
export QUEST_MPI_LAUNCHER=slurm
export QUEST_MPI_LAUNCHER_EXECUTABLE="$quest_helpers/test-rust.sh"
export QUEST_MPI_LAUNCHER_ARGS='["--mpi-step"]'
cargo run --offline --manifest-path "$quest_receipt/Cargo.toml" \
    --target-dir "$quest_receipt/target" > "$quest_receipt/result.log" 2>&1
cat "$quest_receipt/result.log"
test "$(python3 "$quest_helpers/snapshot.py" --verify --root "$QUEST_SNAPSHOT" --digest-only)" = "$QUEST_SOURCE_DIGEST"
printf '%s\n' "$QUEST_SOURCE_DIGEST" > "$quest_receipt/complete"
SH
sbatch --account="$QUEST_SLURM_ACCOUNT" --partition=standard --qos=short \
    --chdir="$QUEST_CAMPAIGN_ROOT" \
    --exclusive --nodes=2 --ntasks=2 --ntasks-per-node=1 \
    --cpus-per-task="$QUEST_PHYSICAL_CPUS" --hint=nomultithread --time=00:05:00 \
    --output="$QUEST_CAMPAIGN_ROOT/logs/timeout-%j.log" \
    --export=ALL "$QUEST_CAMPAIGN_ROOT/probes/supervisor-timeout/run.sh"
```

Run the Cargo coordinator directly in the batch script. Its launcher wrapper
starts each child through `srun` with one MPI process per node. The copied lockfile
seeds the offline standalone resolution; Cargo may prune workspace-only entries
in that private copy. `result.log` must show `timed_out=true` for the first step,
`cancellation_accepted=true`, `termination_confirmed=true`, and
`allocation_reused=true`. The private `complete` marker records the source digest
only after these assertions and final snapshot verification succeed. This probe
establishes scheduler cleanup behavior, independently of numerical test results.

## Historical capped-memory campaign

The user now requires the documented Cirrus scheduler workflow, without custom
resource-control or deployment workarounds. The earlier `capacity-job.sh` /
`capacity.py` campaign imposed `RLIMIT_AS` on MPI processes. It is retained for
historical evidence and receipt validation, but **must not be submitted for new
Cirrus runs**. The Python entry point rejects Slurm execution and an explicit
`srun` launcher before setting limits or launching an MPI child. Local capped
experiments remain separate.

Cirrus allocates memory from requested CPU resources, gives exclusive jobs the
full node memory, and does not support `--mem` requests. The requested small
aggregate per-node cap therefore has no established mechanism in this workflow.
Record that capacity requirement as unsupported here and continue the documented
MPI correctness/scaling work; do not replace it with process limits, cgroup
manipulation, private services, or an inferred cap. See the
[official resource rules](https://docs.cirrus.ac.uk/user-guide/batch/#resource-limits)
and [recorded diagnostic](../../2026-10-06-node-enforcement.md).

Historical `capacity-receipts/<compiler>/job-<job-id>.<suffix>` directories retain
invocations, rank inputs, receipts and completion records. Keep them unchanged.
Their successful numerical runs remain scaling evidence; their process caps do
not establish whole-node capacity. Frozen source snapshots predate the current
execution restriction and are not instructions to rerun that retired workflow.

## Local fixture regression tests

```sh
python3 -B -m unittest discover -s docs/verification/fixtures/cirrus -p '*test*.py' -v
bash -n docs/verification/fixtures/cirrus/build-rust.sh \
    docs/verification/fixtures/cirrus/runtime-env.sh \
    docs/verification/fixtures/cirrus/test-rust.sh \
    docs/verification/fixtures/cirrus/capacity-job.sh
```

These tests use temporary Git snapshots and substitute unavailable cluster tools.
They verify admission, receipt isolation, profile preservation, spool handling,
stage selection, argument boundaries, and node/core placement. They do not claim
execution on Cirrus.
