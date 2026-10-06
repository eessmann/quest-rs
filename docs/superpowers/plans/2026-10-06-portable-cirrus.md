# Portable native builds and Cirrus multi-node validation

The accepted specification is the user's implementation request of 2026-10-06.
Implement within the existing isolated worktree, preserving the earlier sparse,
MathCore and CFD changes. Native QuEST source remains unchanged.

## Delivery sequence

1. Share evaluated native build context; support compiler wrappers, normal
   toolchain inputs, ordered native links, rsmpi discovery and serial HDF5.
2. Isolate binding discovery; separate parser and compiler contexts; add a
   diagnostic command and a portable local/Slurm MPI test supervisor.
3. Deploy source-hashed snapshots; build separate GNU and Cray installations;
   run short smoke tests and lowpriority correctness/capacity/scaling campaigns
   on up to eight Cirrus nodes.

## Acceptance

Run affected regressions, real downstream native consumers, workspace default
and all-feature checks/tests, doctests, formatting, strict Clippy, binding
freshness, and native MPI compatibility. Cover Linux GCC/Clang, Cirrus GNU/Cray,
and Apple Silicon macOS, reporting unavailable executions explicitly.

MPI evidence covers 1/2/4/8 ranks, split communicators, cross-node routing,
whole-register sparse-unitary semantics, malformed resources, allocation errors
and bounded fatal termination. Capacity requires input sharded from creation,
completed execution and canonical stored input exceeding every node's verified
enforced memory cap. Construction and scaling alone do not close capacity.
This criterion remains open: the documented Cirrus workflow does not provide
the smaller whole-node enforcement used by the proposed experiment. Historical
rank-process caps do not establish that contract. Do not introduce custom
enforcement or change the criterion to count a smaller executed input as success.

Use one coordinator and Slurm job steps, account selected by deployment, the
standard partition, short for at most two nodes/twenty minutes and lowpriority
for longer jobs. Do not use Slurm --mem on Cirrus. Keep sources, executables,
dependencies, inputs and MPI witnesses on EPCCFS. The documented Torc standalone
lifecycle uses job-local `$TMPDIR` for its live database and worker output, then
archives closed files on EPCCFS. Record source/toolchain identities, failures
and resource measurements with generic public paths.

Updated execution policy: allocate nodes with `--exclusive`, one MPI rank per
node and OpenMP threads on the allocated physical cores. Set `OMP_PLACES=cores`
and propagate `SLURM_CPUS_PER_TASK` to `SRUN_CPUS_PER_TASK`; use
`--hint=nomultithread --distribution=block:block`. The four-ranks-per-node
capacity proposal is superseded. Short allocations cover at most two ranks;
four/eight-rank tests use four/eight nodes under lowpriority. Native QuEST kernels
may use OpenMP; sparse Rust preprocessing and routing remain serial within a
rank and are reported separately.

Keep aggregate concurrent allocations at or below eight nodes, including build
jobs. Serialize eight-node verification jobs and make following build jobs
depend on their completion. Use fresh compiler-specific targets and immutable
source manifests; a successful earlier snapshot does not validate later code.

## Implementation and acceptance ledger

The records below retain exact source identities and distinguish implemented
behavior from executed acceptance. Their stage receipts are authoritative;
this ledger does not turn an earlier or focused pass into current full-matrix
acceptance.

| Delivery | Implemented contract | Evidence and remaining gates |
| --- | --- | --- |
| Native discovery and linking | Shared `NativeBuildContext`; evaluated compiler invocation, target and header requirements; ordinary module/toolchain inputs; ordered link scopes; shared/static selection and bounded whole-archive input admission; locked rsmpi selection with independent ABI witnesses; dependency-aligned serial HDF5. | [Native discovery follow-up](../../verification/2026-10-06-native-discovery-followup.md) records the latest regressions. Reviewed-source [GNU](../../verification/2026-10-06-native-gnu.md) and [Clang](../../verification/2026-10-06-native-clang.md) workspace checks passed; the independent MPI consumer passed after its separate lockfile correction. [Cirrus GNU and Cray execution](../../verification/2026-10-06-portable-reviewed.md) passed on the corrected `452a498…` snapshot, with all 25 runtime stage/control statuses zero in each lane. The earlier Clang CMake module-read failures remain unexplained. Apple Silicon execution remains unavailable. |
| Tooling and MPI supervision | Cargo-target-owned temporary discovery; semantic parser context separated from compiler optimization; stage-specific doctor JSON; bounded Rust supervisor using local launchers or owned Slurm steps, expected-rank assertions, deadlines, cancellation and durable failure witnesses. | [Tooling contracts](../../native-tooling.md), [original Cirrus campaign](../../verification/2026-10-06-portable-cirrus.md) and [MPI request ABI evidence](../../verification/2026-10-06-mpi-request-abi.md) distinguish parser, compiled ABI and fatal-path checks. The [current-source Slurm deadline probe](../../verification/2026-10-06-portable-reviewed.md#current-source-slurm-deadline-and-allocation-reuse) passed owned-step cancellation, termination confirmation and allocation reuse. Ordinary module-preserving consumers and opt-in loader-isolated consumers are separate gates. Cirrus loader isolation remains unsupported for the retained vendor installations. |
| Cirrus sparse execution, resources and orchestration | Immutable source deployment; separate GNU/Cray builds with central serial HDF5; explicit placement; input sharded from creation; phase timings and numerical checks; Torc standalone builds and a server-managed Cray runtime with archived artifacts. | [Torc/runtime campaign](../../verification/2026-10-06-torc-cirrus.md), [buffer-reuse checks](../../verification/2026-10-06-matching-buffer-reuse.md) and [real large-count transfers](../../verification/2026-10-06-large-count-mpi.md) retain distinct snapshots and scopes. The [explicit scaling campaign](../../verification/2026-10-06-scaling-only.md) passed all six 2/4/8-node cases in each compiler lane on `ff4d5ce…`; the Cray allocation was submitted through the supplied Torc server. These runs establish distributed correctness and scoped scaling measurements. [Whole-node enforcement](../../verification/2026-10-06-node-enforcement.md) and oversized-input capacity remain open. |

Keep these outstanding checks explicit:

- Complete default/all-feature tests, doctests, compilation, formatting, strict
  Clippy, bindings and installed consumers for each admitted current compiler
  lane. Preserve configured skips and failed attempts.
- Retain full-register sparse-unitary comparisons, controls, adjoints, failure
  flags, padding, persisted replay and inverse residual assertions in ordinary
  MPI suites. Placement or ABI success alone is insufficient.
- Retain split communicators, malformed shards, allocation errors, cross-node
  routing, real count chunking and bounded fatal termination as separate
  evidence. Neither a launcher code nor a stderr phrase proves failure safety.
- Keep per-rank managed allocations, actual process samples and whole-node peak
  measurements distinct. Preparation, persistence, transfer and repeated
  execution costs must retain their recorded scopes; runtime success is not
  a capacity or speedup claim.
- Obtain actual Apple Silicon execution and a supported capacity enforcement
  and storage contract before closing those gates. Unsupported environments
  are reported without patched native/vendor libraries or custom resource
  controls.

## Scaling without a verified capacity cap

Ruling: implement an explicit `sparse_capacity --scaling` route for the
documented Cirrus environment. The accepted plan requires retaining scaling
evidence when an enforceable capacity cap is unavailable. The existing finite
`RLIMIT_AS` requirement prevents that route when the scheduler leaves the
process limit unlimited. The new route observes finite or unlimited limits;
it never sets them. Existing capped invocations retain their contract.

Managed rank/node budgets and the modeled MPI-baseline/OpenMP-stack envelope
remain admission inputs, separate from enforcement evidence. Scaling receipts
must identify themselves as scaling only and leave strict capacity open.
Sampled MPI-rank RSS, native-array payload and modeled budgets cannot be
substituted for measured whole-node peaks or enforced whole-node limits.
If this distinction were wrong, a successful small run could falsely certify
capacity; explicit receipt fields and regression checks guard that boundary.

The [scaling-only follow-up](../../verification/2026-10-06-scaling-only.md)
records the implemented mode, focused local MPI checks and a separately frozen
release campaign. Its immutable source differs from the completed GNU/Cray workspace
snapshot only in the listed example, fixture and documentation overlay. Runtime
results apply to their own snapshots. All six fixed-work/fixed-work-per-node
cases passed in each compiler lane, with independently verified canonical input
hashes. These small inputs establish multi-host execution and measured scaling,
not the required oversized-input capacity result.

The [server-backed Torc guide](../../verification/fixtures/cirrus/torc-server.md)
records a confirmed login-node endpoint and successful API checks from the login
host and a compute node. The supplied server is
unchanged; its configured live database resolves to NFS storage. Torc advises
avoiding NFS for this purpose, and the successful health check does not validate
locking, durability or worker storage. This limitation remains explicit rather
than being treated as a demonstrated database failure. Workflow 1 submitted a
single eight-node allocation, 538318, whose six scientific cases passed. Its
generated script retained one coordinator, payload-owned `srun` steps, no explicit
Slurm memory request, and the prescribed placement and QoS. Scientific and Torc
lifecycle receipts remain separate evidence.
