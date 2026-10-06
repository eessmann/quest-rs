# Portable native builds and Cirrus validation

This record concerns the portable-build and distributed-execution implementation.
It does not close the complete CFD research programme or its physical accuracy
gates. Native QuEST source remains unchanged. Cluster jobs use isolated,
content-addressed snapshots; the existing cluster checkout is preserved.

## Implementation

`quest-build::NativeBuildContext` shares evaluated CMake discovery across Cargo,
binding generation and installed consumers. Compiler wrapper invocation paths
remain distinct from canonical identities. Normal module/toolchain/header flags
are admitted. Ordered native linking resolves balanced whole-archive scopes to
actual shared libraries or archives, retaining dependency order and rejecting
unsupported state transitions.

MPI discovery uses the same pinned `build-probe-mpi` implementation as the locked
rsmpi dependency. Independent native/library witnesses and generated Rust layout
checks include sizes, alignments and status-field offsets. A deliberately packed
bindgen consumer was rejected before MPI initialization. Serial HDF5 discovery
matches the Rust dependency, including pkg-config lib64 and Homebrew selection;
normal HDF5 builds additionally verify header/runtime agreement.

`xtask native-doctor --json` reports stage-specific evidence with generic paths.
Binding discovery uses isolated temporary directories beneath Cargo's selected
target. Libclang receives semantic header requirements, not arbitrary native
optimization flags. Consumer compilation retains the module loader environment;
loader-isolated runtime checks are separately selectable.

The shared MPI test supervisor uses argument vectors, bounded pipe capture,
deadlines, rank assertions and owned Slurm-step cancellation. A successful cancel
request and confirmed step termination are distinct evidence. Fatal-path tests
retain durable pre-failure witnesses. Placement discovery uses actual MPI
shared-memory communicators, including split communicators.

## Execution policy

Both Cirrus compiler lanes load the central serial `cray-hdf5` module. The
observed module is 1.14.3.5, with separate GNU and Cray installations. Native
QuEST 4.3.0 was built twice from unchanged revision
`503552065045eaf89baba85e6cd6aad728525554`, using GNU 14.2 and CCE 19.0 with Cray
MPICH 8.1.32. The campaign uses CPU/MPI; accelerator admission remains explicit.

Runtime jobs request exclusive nodes, one MPI process per node, allocated
physical cores as OpenMP threads, `OMP_PLACES=cores`, and
`SRUN_CPUS_PER_TASK=$SLURM_CPUS_PER_TASK`. Steps use physical-core binding and
`--distribution=block:block`. Four/eight-rank tests require four/eight nodes.
Nextest 0.9.146 runs one coordinator, with MPI launches serialized; doctests run
separately. The [reproduction scripts](fixtures/cirrus/README.md) describe the
source, compiler, placement and receipt contracts.

These settings follow the [Cirrus batch guidance](https://docs.cirrus.ac.uk/user-guide/batch/)
and [compiler-wrapper guidance](https://docs.cirrus.ac.uk/user-guide/development/).
No Slurm memory request is used. All required cluster files live on EPCCFS.

Native QuEST initialization, Hadamards and cloning can use OpenMP in the hybrid
capacity path. Sparse preprocessing, coefficient rotations and routing currently
remain serial Rust work within each rank. Native threading flags are not a
measurement of speedup. Indexed native writes retain ordered duplicate-address
semantics and cannot simply receive an OpenMP pragma.

## Local evidence

- Shared native-build focused suite: 57 tests; xtask vendor-version suite:
  55 unit and 2 integration tests, followed by two parser argument regressions.
  All 14 parser tests pass; actual MPI witness execution passes Ninja
  Multi-Config Debug, Release and RelWithDebInfo.
- Generated MPI ABI and actual packed-bindgen rejection: passed.
- Linux GCC and Clang 22 native/MPI checks; Clang binding freshness and six MPI
  tests at 1/2/4 ranks passed.
- GCC and Clang installed consumers: ten CPU/OpenMP cases each passed. GCC
  loader-isolated consumers also passed all ten cases.
- CFD/CLI migrated fixtures: 59 distinct tests and 65 MPI launch cases passed,
  including 1/2/4/8 ranks, split communicators and witnessed fatal paths. Three
  expensive CFD campaigns remain intentionally ignored.
- Initial all-feature Nextest run: 1673 passed, seven CMake filesystem failures,
  six skipped. All seven failures subsequently passed in both serial and parallel
  26-test probe reruns. A later observed `/tmp` user-quota failure was resolved by
  moving compilation artifacts to disk-backed targets; the earlier CMake failures
  are consistent with transient filesystem pressure but their cause is not proven.
- Subsequent clean full all-feature Nextest rerun: 1687 passed. After all review
  fixes, the reviewed source passed all 1691 tests and 65 doctests. After the
  final NFS cleanup regressions, all 1693 tests passed in 325.079 seconds. Six
  explicitly ignored campaigns and one doctest remain skipped. Temporary storage
  was disk-backed; formatting, strict Clippy and binding freshness also passed.
- Workspace all-feature doctests: 65 passed, one ignored. Default-feature
  workspace check passed; 1501 tests and 55 doctests passed, with one explicitly
  ignored scale test and one ignored build-script snippet.
- Final shared supervisor suite: 17 tests passed; Cirrus Python fixtures: 55
  passed. Formatting and strict workspace Clippy passed.
- Later link-fixture corrections passed nine focused local tests and strict
  package Clippy. The catalog staging correction passed its three offline
  regressions and strict xtask Clippy. These targeted checks supplement the
  earlier complete 1693-test run; they are not another complete workspace run.
  The final combined Nextest selection passed all twelve link/catalog tests;
  107 unrelated package tests were excluded by that selection.
- Local capped execution at dimensions 64, 8192 and 16384 completed. Enabled native
  OpenMP was verified at two threads with one and two local ranks. Deliberately
  excessive worker-stack reservation was rejected before writing source shards.

## Cluster investigation receipts

| Job | Scope | Outcome |
| --- | --- | --- |
| 536350 | Separate GNU/Cray native QuEST builds | Passed; initial script working directory fell back, all build paths were absolute EPCCFS |
| 536389, 536390 | First Rust compiler jobs | Rejected missing verifier after Slurm relocated script into its spool directory |
| 536392 | GNU Rust/doctor | Native bridge built; parser rejected vendor version banner |
| 536393 | Cray Rust startup | Rust bundled linker rejected Cray plugin options |
| 536395 | Small Cray linker experiment | Default reproduced failure; disabling LLD selection linked and ran |
| 536397 | Cray first-snapshot workspace compilation | Passed build, doctor, binding and no-run stages for the first source snapshot |
| 536415 | GNU revised snapshot | Exposed stale Cargo reuse with normalized archive timestamps |
| 536416 | Cray revised snapshot | Completed commands, but shared-target reuse prevents treating this as exact revised-source acceptance |
| 536497, 536498 | Isolated GNU/Cray builds, source `61533c48` | Both passed doctor, bindings, workspace compilation and examples |
| 536500, 536501 | Initial two-node smoke | Rejected batch rank-zero markers before MPI execution; coordinator guard corrected |
| 536632 | GNU two-node diagnostic smoke | Five tests passed using an explicitly recorded batch-context adapter |
| 536633 | Cray two-node diagnostic smoke | Two tests passed, three failed; OpenMP startup and floating-point mode investigated separately |
| 536636 | GNU two-node hybrid sparse pilot | Dimension 1024 forward/adjoint execution passed; capacity threshold remained open |
| 536644 | GNU isolated build, source `6a6b4ee4` | Passed all build stages |
| 536696 | GNU two-node smoke, source `6a6b4ee4` | All five tests passed directly through the corrected coordinator; expected fatal step retained its witness |
| 536697 | Cray isolated build with floating-point link correction | Passed all build stages; LibSci runtime correction still required |
| 536700 | GNU eight-node full workspace, source `6a6b4ee4` | 1680 tests passed, seven filesystem/cleanup tests failed, six skipped; 65 doctests passed, one ignored |
| 536701–536703 | First scaling submissions | Failed before execution: submission environment lacked the module shell function |
| 536709 | First real Slurm timeout probe | Stopped before execution because the probe assumed an old Cargo artifact directory layout |
| 536710 | Fresh GNU/Cray native installations | Passed; Cray excludes unused threaded LibSci and retains native OpenMP |
| 536714 | First installed Cray thread probe | Stopped before compilation because the auxiliary-source path followed Slurm's spool location |
| 536731 | Corrected installed Cray thread probe | All five C/Rust main, worker and libtest cases passed with measured OpenMP teams and MPI/QuEST execution |
| 536732–536734 | GNU sparse execution on 2/4/8 nodes | Six fixed-workload and fixed-work-per-node points completed; see the [capacity and scaling record](2026-10-06-cirrus-capacity.md) |
| 536735, 536736 | Final GNU/Cray Rust builds | Reached workspace linking, then failed with disk quota exceeded; source and build receipts retained |
| 536737 | Second timeout probe setup | Stopped before execution because direct artifact linking selected Cargo's split metadata output |
| 536795 | Cargo-built real Slurm timeout probe | Owned step cancelled, termination confirmed, and a second step completed in the same allocation |
| 536793 | GNU eight-node growth, dimension 131,072 | Completed three controlled forward/adjoint roundtrips; original input 8 MiB, capacity gate still open |
| 536807, 536808 | Final GNU/Cray build retries, source `8d85c0e8` | Both passed doctor, binding freshness, all-feature workspace compilation and examples |
| 536809 | GNU final filesystem/tooling regressions | All 170 build/tooling/I/O tests and the CFD collective cleanup regression passed |
| 536810 | Cray installed consumers | All ten CPU/OpenMP cases passed with module environment; loader-isolated check failed on unresolved MPI dependency `libfabric.so.1` |
| 536811 | Cray final two-node smoke | All five tests passed, including witnessed fatal MPI cleanup |
| 536812 | Cray eight-node workspace, source `8d85c0e8` | 1689 passed, four harness failures, six skipped; doctests stopped at a linker-policy failure |
| 536815–536817 | Cray sparse execution on 2/4/8 nodes | All three fixed-size cases completed with the matching native/compiler profile |
| 536794 | Oversized GNU admission request | Rejected on all eight ranks before input creation; zero source bytes, bounded owned-step cleanup |
| 536827 | GNU default and lint acceptance, source `8d85c0e8` | All 1509 tests and 55 doctests passed, one ignored in each; default/all-feature checks and strict Clippy passed; frozen formatting check failed |
| 536828 | Cray default and lint acceptance, source `8d85c0e8` | 1504 tests passed, five failed, one skipped; default/all-feature checks and strict Clippy passed; frozen formatting and Rustdoc policy failed |
| 536833 | GNU installed consumers | All ten numerical cases and two native-mode checks passed with modules; loader isolation failed on unresolved `libfabric.so.1` |
| 536853, 536854 | Corrected link-fixture snapshot `d17fdbab`, GNU/Cray | All 59 native-build tests and strict package Clippy passed in each compiler profile |
| 537107 | Corrected Cray Rustdoc policy | Default 55 doctests and all-feature 65 doctests passed; one ignored in each mode |
| 536865 | First scoped Cray native UI run | Native linking succeeded; one diagnostic snapshot differed because encoded flags displaced `trybuild`'s verbose diagnostic defaults |
| 537112 | Corrected Cray native UI policy | All three harness tests passed, retaining all 27 original compile-pass/fail cases and the independent trait assertion |
| 537135 | Maintained Cray native UI driver, default features | All three harness tests passed, including the same 27 UI cases; source/profile validation and completion receipt passed |
| 536862–536864 | Cray fixed-work-per-node execution on 2/4/8 nodes | All three points completed using one recorded immutable executable copy |
| 537136, 537139 | Catalog cleanup probe setup | First rejected unavailable Python `tomllib`; second failed to compile its negative control; neither executed the corrected module's tests |
| 537141 | Exact corrected catalog module on Cray EPCCFS | Negative ownership-order control failed as expected; all three corrected offline tests passed with unchanged pinned dependencies |

The fixes use shared-snapshot helper paths, semantic Clang version macros and
`-C linker-features=-lld` in the Cray profile. The latter is the documented
[Rust linker-selection control](https://doc.rust-lang.org/rustc/codegen-options/index.html#linker-features).
Targets are now separate for every compiler/source digest, with an immutable
compiler/module/flags profile. Earlier successful commands do not substitute for
the subsequent isolated campaign.

The exact GNU smoke source is
`6a6b4ee4a69aace3da90eafa8bc2bf0644e88eb21af5fef778b6b0b515bbc3c5`.
Its Nextest tests cover MPI lifetime, borrowed-thread messaging, collective
mismatch rejection, witnessed fatal cleanup and common application preparation.
The expected fatal child exits nonzero; the parent test additionally requires its
durable pre-failure witness and bounded termination.

The capacity/scaling campaign has 13 completed cases across both compiler
profiles, including fixed global work and fixed work per node. Its largest
executed original input is 8 MiB. The Cray weak-scaling lane retains a different
executable identity from the earlier strong-scaling lane: a hash preflight found
the build-directory executable had changed. Subsequent weak-scaling cases all
used one immutable copy, and the [capacity record](2026-10-06-cirrus-capacity.md)
preserves both identities without attributing their difference to an unproven
cause. None of these runs closes the oversized-input capacity gate.

A fresh Cray runtime probe found that its final-link startup object enabled
FTZ/DAZ. The compiler profile now adds `-C link-arg=-mno-daz-ftz`; a compute-node
probe verified gradual-underflow mode through MPI and QuEST initialization without
changing the native library. The
[Clang documentation](https://clang.llvm.org/docs/UsersManual.html#a-note-about-crtfastmath-o)
describes this startup control. A separate probe isolated Cray OpenMP worker-thread failure to automatic
threaded LibSci loading: preloading that library alone reproduces the fatal
error before MPI or QuEST entry. Cray native and Rust profiles now unload
`cray-libsci`, retaining OpenMP. The fresh native installation passed five independent C/Rust runtime probes,
including pthread and libtest entry, with measured two-thread OpenMP teams and
`MPI_THREAD_MULTIPLE`. Normalized probability was approximately one and FTZ/DAZ
remained disabled through the native kernels. Previous installations remain intact.

Cirrus batch coordinators have rank-zero task markers without a numeric step ID.
The supervisor admits that verified batch context, while rejecting actual MPI
rank markers and numeric/inconsistent step contexts. Rust and shell regression
matrices cover the distinction.

Library-search deduplication preserves the first spelling and search position
when EPCCFS or symlinks provide multiple paths to the same directory. An NFS
fixture now closes its independent spool reader before asserting complete
directory cleanup. The full GNU run additionally found the same relative-path assumption in an
xtask fixture and open-reader cleanup in a sparse-stream fixture. HDF5 writer,
snapshot-reader and manifest cleanup now close owned file descriptors before
unlinking temporary paths. Syscall traces verify that ordering; all 54 sparse-I/O
tests pass locally. The seven cluster failures retain their original diagnostics;
all passed the focused final GNU rerun on EPCCFS.

The main implementation campaign source is
`8d85c0e8075cec3057b4da14055c73ebc35bb04ab4b01b155153f678bdfa0f4d`.
Its first build attempts exhausted the campaign's filesystem quota at linking.
Four obsolete source-specific/shared Cargo caches and an older GNU shared cache
were removed after preserving their compiler profiles. All source snapshots,
native installations, receipts, the scaling executable's target, and both final
targets were retained. Retries reuse the identical final source and compiler
profiles; these are disk-capacity retries, not modified-source acceptance.

The real timeout probe launched a two-node child through the shared supervisor,
observed the child start, reached its five-second deadline, and cancelled only
its authenticated Slurm step. The scheduler confirmed disappearance of that
step; a second two-node step then succeeded in the surviving allocation. Total
probe time was 5.403 seconds. It used the earlier `6a6b4ee4` supervisor snapshot,
with its source identity retained separately from the final workspace run.
This checks scheduler termination and allocation
reuse; the child does not initialize MPI. Separate witnessed fatal MPI tests
cover failures after native initialization. The probe is built as a standalone
Cargo consumer so it does not assume a particular Rust artifact layout.

The binding parser now preserves header-relevant compiler invocation arguments
(such as `CXX='c++ -DREQUIRED_HEADER_CONTEXT=1'`) before target definitions and
flags. A copied-header fixture failed with the missing macro before the fix and
passed all native-doctor stages afterward. Optimization options remain filtered.

The complete Cray run took 1170.563 seconds. Its four test failures have two
harness causes: two link-order fixtures invoked `rustc` directly without Cargo's
configured linker flags, and two `trybuild` UI harnesses failed while linking
host dependencies before reaching their semantic assertions. The link fixtures
now preserve the selected linker and effective flags, with an independent
compiler/linker-wrapper regression. The corrected test snapshot is
`d17fdbabf41d9f437c2c28e149db124a432d45f46315194631324d83ff0f52eb`;
its only code difference from `8d85c0e8` is inside the link test module.
Other differences are documentation. All 59 native-build tests and strict
package Clippy passed in each compiler profile on that corrected snapshot.

The UI harness adds an explicit target, which makes Cargo omit target flags from
host dependency builds. The historical native-only `trybuild` lane preserved the
Cray linker policy and the pinned harness's diagnostic flags, without changing
expected diagnostics or the normal cross-target policy. All three harness tests
passed: three compile-pass cases, 24 compile-fail cases, and the independent
non-`Send` trait assertion. The initial native-only attempt retained an actual
diagnostic mismatch; its missing upstream verbose flag was fixed instead of
rewriting the expected error. The behavior follows
[Cargo's target flag rules](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags)
and the [pinned trybuild implementation](https://raw.githubusercontent.com/dtolnay/trybuild/1.0.121/src/cargo.rs).
That historical driver routed those two binaries into an isolated native UI
target and retained distinct base-suite, UI and doctest results. Its default-mode
cluster run also passed all three harness tests. That standalone run's original
receipt incorrectly included a base-suite filter despite running no base suite;
a subsequent reporting-only correction removes that metadata. The executed UI
selection, flags, counts and completion evidence remain unchanged. The
[current campaign](2026-10-06-torc-cirrus.md) runs ordinary, unfiltered workspace
suites, including these binaries. The historical route's passes do not replace
ordinary-suite acceptance. Its separate routing is no longer recommended under
the documented-tools-only policy; current failures are retained rather than
addressed through that profile.
The separate doctest failure came from Rustdoc's independent flag channel;
the first affected package stopped before later packages ran. The maintained
profile now preserves each of Rustc's and Rustdoc's own encoded/plain flags and
applies the Cray linker policy independently. The historical adapter passed all
55 default and 65 all-feature doctests, with one ignored in each mode. Its
receipt records the Rustdoc override separately and verifies that the original
build profile remains unchanged.

The later default-feature acceptance exposed one further NFS cleanup failure
in the catalog downloader's offline failed-read fixture. No catalog download or
catalog-content modification was performed. The staging descriptor now closes
before any error unlinks its temporary path. A deterministic ownership-order
regression fails when error propagation precedes descriptor closure and passes
with the correction. All three offline catalog tests passed locally and in a
standalone exact-module Cray EPCCFS probe with matching pinned dependencies. The
tested module SHA-256 is
`06379c5bc6b14c290a03b28ce6124ea60b83bce3630bd807210b192c2ff5b5e1`.
This is module-level evidence, not a complete workspace rerun. Two failed probe
setup attempts remain recorded separately. The frozen acceptance snapshot
also had one formatting-only fixture difference; the current workspace format
check passes after that correction.

## Open acceptance gates

GNU and Cray builds, normal installed consumers, default/all-feature checks and
strict Clippy passed. Complete workspace runs and subsequent focused corrections
are reported separately above; no single frozen snapshot passed every cluster
stage in one run. Both compilers completed sparse execution on two, four and eight
distinct nodes. Original failures remain part of the evidence.

Apple Silicon macOS execution remains unavailable. A subsequent, separately
hashed [large-count transport follow-up](2026-10-06-large-count-mpi.md) passed
2,147,483,664-byte logical transfers in both directions on two nodes under GNU
and Cray. It verifies real chunking at the signed-int count boundary; it does not
exercise MPI-4 large-count calls or change the matching routing buffer size.
The subsequent [Linux native Clang validation](2026-10-06-native-clang.md)
builds unchanged QuEST with Clang and LLVM OpenMP, verifies the current Rust
workspace and exercises an independent MPI-enabled consumer at 1/2/4/8 local
ranks. Its two installation configurations retain separate loader outcomes.
This local evidence does not extend the frozen Cirrus snapshots to later changes.
Loader-isolated Cirrus consumers remain unsupported for these installations under
the required [documented module-based workflow](https://docs.cirrus.ac.uk/user-guide/development/);
the limitation is recorded and no workaround is pursued.
Normal module-preserving consumers pass in both compiler lanes. A read-only
[installation diagnosis](data/2026-10-06-portable-cirrus/loader-isolation-diagnosis.json)
reproduced the unresolved `libfabric.so.1` dependency in both retained consumer
binaries. Both native caches already set `CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON`.
The GNU and Cray MPI libraries require `libfabric.so.1` but contain no
`DT_RPATH` or `DT_RUNPATH`; the consumers' direct-library `DT_RUNPATH` entries do
not resolve that transitive dependency. The selected libfabric library's own
closure resolves successfully. This establishes a vendor runtime-deployment
limitation, not a missing CMake link-path setting. The user's documented-tools-only
constraint closes this investigation without a workaround: native installations,
vendor libraries and the loader-isolation check remain unchanged, and no further
cluster job was submitted for this diagnosis. The failed isolation results remain
failures and are not replaced by the module-preserving passes.

Strict capacity remains open: the largest executed original input is 8 MiB,
whereas the historical campaign enforced an 8 GiB address-space cap per rank.
That custom cap is not used by the current documented-tools-only campaign.
The historical cap covered the rank
process, not launcher processes, filesystem cache or the operating system; it is
not a whole-node cgroup limit. Acceptance requires canonical original sparse
input to exceed each participating node's verified cap, followed by complete
numerical execution. Source
records occupy exactly 32 bytes per nonzero; derived encoding storage is excluded.
The [native array storage bound](../research/distributed-capacity-memory.md)
now establishes that this controlled, two-register execution profile cannot
close the gate on eight or fewer nodes merely by increasing the input size.
Local capacity machinery and multi-node scaling are distinct from that acceptance.

The [machine-readable receipts](data/2026-10-06-portable-cirrus/summary.json)
retain source/profile identities, stage outcomes and hashes of the raw evidence.
The [capacity record](2026-10-06-cirrus-capacity.md) records all thirteen completed
cases, placement, timings, memory accounting and the rejected oversized request.
