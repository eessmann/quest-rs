# Native linking and HDF5 discovery follow-up

Two additional portability defects were found while the frozen Cirrus campaign
was running. These corrections are separate from source snapshot
`d8c05b505d90989f405ce79f18abdf7566eb8f68ed6be81b735e7804e7ed993a`.
That snapshot's cluster results do not validate these later changes.

## Whole-archive input admission

Library filename suffixes do not establish what the linker will load. A file
named `libexample.so` can be a GNU linker script containing `INPUT(archive)`.
Previously the scope lowering classified that file as a shared library and
removed the surrounding whole-archive state, losing unreferenced archive
members. A real compiler/linker regression reproduces the difference: an
otherwise unused constructor produces 73 with whole-archive and 0 without it.
The original implementation incorrectly admitted the scoped script.

Scoped lowering now reads at most 64 bytes to distinguish archive files from
ELF shared objects. Unsupported scripts or unrecognized inputs receive an
explicit admission error because their original linker scope cannot be
preserved by this representation. The native linker still validates admitted
binary contents. Ordinary unscoped shared-library scripts retain their prior
behavior, including a tested downstream Rust-library consumer. This does not
implement a linker-script interpreter or extend Darwin whole-archive support.

## HDF5 selection parity

The previous raw `pkg-config --cflags-only-I` query could omit system include
paths even when the locked HDF5 dependency accepted the installation. A
successful pkg-config result without a usable header also prevented the
dependency's subsequent default-layout fallback.

Discovery now uses the same existing `pkg_config::Config` API as locked
`hdf5-metno-sys` 0.12.4, retaining system paths and target-specific environment
handling. It falls back after either a failed query or a successful query with
no usable header, keeping the selected fallback's header and library paths
together. Explicit `HDF5_DIR`, Homebrew selection and parallel-HDF5 rejection
retain their existing contracts. Real `.pc` fixtures, including paths with
spaces, fail before the change and pass afterward. No dependency was added.

## Focused local evidence

The final source passed independent review. There are no new unsafe Rust blocks,
lint exceptions or native QuEST changes. The existing LLVM/Clang QuEST 4.3.0
installation and matching system MPICH were used for downstream checks.

| Check | Result |
| --- | --- |
| Linker-script regression before correction | Failed at the admission assertion; native counterexample reproduced |
| HDF5 regressions before correction | Both failed at header discovery |
| Complete `quest-build` tests | 62 passed |
| `quest-build` Clippy, all targets/features | Passed with `-D warnings` |
| `quest-qsvt-io`, all features | 54 tests and 2 doctests passed |
| `quest-sys`, MPI feature | 51 tests passed, zero skipped |
| Workspace all-feature compilation | Passed |
| Binding freshness | Passed |
| Installed CPU/OpenMP consumers | Ten numerical cases passed |
| Native doctor | Passed |
| Formatting and whitespace | Passed |

Earlier Clippy failures from guarded indexing and function length were fixed;
their logs remain preserved alongside the final passes. The existing
`quest-polynomial` nightly trait-solver warning remains in downstream build
logs. Package Clippy passes without suppressing it in the changed crate.

## Workspace campaign and unresolved CMake failures

The subsequent local workspace campaign completed with exit status 1. Its
all-feature test stage failed; the other five stages passed. It used the same
Clang-built native installation and matching MPICH, four nextest test workers,
eight Cargo build jobs and two OpenMP threads. All Cargo commands were locked
and offline.

| Stage | Result |
| --- | --- |
| Workspace all-feature tests | 1,706 passed, 2 failed, 6 skipped; exit 100 |
| Workspace default-feature tests | 1,515 passed, 1 skipped |
| Workspace all-feature doctests | 65 passed, 1 ignored |
| Workspace default-feature doctests | 55 passed, 1 ignored |
| Workspace all-target/all-feature Clippy | Passed with `-D warnings` |
| Workspace default-feature compilation | Passed |

Both failures were in `quest-build` probes, before their assertions:

- `probe::tests::cmake_build_evaluates_target_requirements_and_paths_with_spaces`:
  CMake could not open its installed `CMakeParseImplicitIncludeInfo.cmake` module.
- `probe::tests::cargo_discovery_evaluates_the_selected_package_once`:
  CMake could not open its installed `Platform/Linker/GNU.cmake` module.

Both diagnostics said `cmListFileCache: error can not open file`, followed by an
unknown-command error from the missing module. Neither original failure reports
an operating-system error number. Their cause remains unproven, and the failed
all-feature campaign remains an open acceptance result.

The bounded diagnosis found both modules readable, with contents matching the
installed CMake 4.3.0 package digests. The inspected tests retain unique temporary
directories through synchronous configure calls; no shared fixture or premature
directory removal was identified. Current resource-limit snapshots did not
establish a cause and cannot reconstruct limits or resource pressure in the
original failing processes.

Both direct tests subsequently passed when run concurrently inside the sandbox.
They also passed separately under `strace` outside it: both module paths opened
successfully, with no relevant permission, interruption, file-descriptor,
I/O or memory-allocation errors in those traces. The first sandboxed tracing
attempts were denied by ptrace restrictions before test bodies ran. Approved
tracing outside the sandbox changes the environment and timing, while direct
tests omit nextest scheduling and the full workspace workload. These diagnostic
passes do not demonstrate a repair or replace the original failures. No source
or package changes, test skips, retry logic or campaign restarts were introduced
for this diagnosis.

The separate [local GNU campaign](2026-10-06-native-gnu.md) completed all twelve
stages successfully against the full follow-up snapshot identified below.
It passed 1,708 all-feature tests with six skipped and 1,515 default-feature
tests with one skipped, both doctest modes, checks, strict Clippy, native
discovery, both consumer loader modes and independent MPI consumers at one,
two, four and eight local ranks. Its compiler, native installation and execution
context differ from this Clang campaign; that success does not establish a
cause or repair for the two Clang failures.

The [machine-readable evidence](data/2026-10-06-native-discovery-followup/summary.json)
records source and log identities, completed stage statuses, and the diagnostic
receipt. Complete raw logs remain under
`target/link-kind-20261006-evidence`,
`target/hdf5-discovery-20261006-evidence`, and
`target/portable-discovery-20261006`. Public records use generic paths.
Current-delta Cirrus build execution is recorded below. Full cluster runtime
acceptance and macOS execution are not established by these build results.

## Prepared Cirrus follow-up

A separate immutable source snapshot includes the two discovery corrections and
the reviewed `discovery` stage of the existing standalone Torc recipe:

| Input | SHA-256 |
| --- | --- |
| Source manifest, 2,584 files | `cc71c574b4ebcfa3a69e8d404642b15748b733fe9c8c430ad29406dde131043c` |
| Source archive | `5ec5dbf1b4f3cab9faf53c57b7cf13cc633240d0c2f53bec3c27bbfab1e98c12` |

Archive and manifest identities passed local and remote verification; the remote
source is read-only. The installed Torc CLI's `create --dry-run` accepts
`torc-native-discovery.yaml` with no errors or warnings. Both shell payloads pass
syntax and ShellCheck warning checks. The two narrow ShellCheck exceptions
describe JSON serialized for the Rust MPI supervisor, not shell arguments.

This workflow runs the complete `quest-build` package tests and strict Clippy,
native doctor and binding freshness, using central serial HDF5. It creates a
fresh `targets/discovery/<compiler>/<digest>` target. Its successful receipt
cannot satisfy a complete workspace build prerequisite. It does not execute
the workspace or MPI suites and cannot replace their acceptance evidence.

The focused workflow was not submitted. EPCCFS had about 13 GiB free after its
source transfer, insufficient for a fresh pair of full workspace targets. The
user subsequently authorized removing only the inactive Cargo cache at
`$WORK/quest-validation/2026-10-06-portable/targets` (about 42 GiB). That removal
completed, leaving about 52 GiB available. Sources, native installations, logs,
receipts and current targets remain retained. This permits full verification
instead of limiting the next campaign to discovery tests.

## Full Cirrus build campaign for snapshot 565f59

The new full-build snapshot includes the discovery corrections and a subsequent
documentation clarification distinguishing default module-environment consumer
checks from `--loader-isolated`. The earlier local test receipt's README hash
continues to identify the documentation as it stood during that run; executable
Rust and C++ source is unchanged by this clarification.

| Input | SHA-256 |
| --- | --- |
| Source manifest, 2,584 files | `565f59ccd796712da5277c57a4203d0869f0ad67a26fdec47e99709b83dc2602` |
| Source archive | `b9256d7c0cbb72b7b2477b782c8e5666cde150564b261dd78f51cee5e2b80f5b` |

The archive and manifest passed local and remote checks; the remote source is
read-only. The installed Torc CLI accepted the complete build workflow's dry run
with no errors or warnings. Native library identities still match the retained
GNU and Cray installations. Torc build jobs `538177` (GNU) and `538178` (Cray)
were submitted with `afterany:538141`; their two exclusive one-node allocations
started after the original eight-node Cray run ended. Each requested eight CPUs,
two hours and `lowpriority`, retaining central serial HDF5 and fresh
compiler-specific Cargo targets.

| Compiler | Slurm job | Result | Allocation duration | Start and end, UTC |
| --- | --- | --- | --- | --- |
| GNU | 538177 | `COMPLETED`, `0:0` | 3m 41s | 19:04:00–19:07:41 |
| Cray | 538178 | `COMPLETED`, `0:0` | 3m 40s | 19:04:01–19:07:41 |

Both lanes passed native doctor, binding freshness, default-feature and
all-feature test compilation, independent MPI-consumer compilation and source
manifest checks. Every native stage and final receipt returned zero, and both
receipt digests match `565f59cc…2602` above. Native QuEST remains unchanged at
revision `503552065045eaf89baba85e6cd6aad728525554`.

Each Torc export contains exactly one completed job and one completed result
with return code zero. Workflow, job, run and attempt identifiers all equal one
within each standalone database, and result references match their workflow
and job records. Torc's `run` and final batch receipts are zero; structured
acceptance is true. The CLI/server binary identities match the previously
recorded Torc 0.41.0 installation. Both restored closed database/WAL sets pass
read-only SQLite `quick_check`, and both nested archives pass gzip integrity
checks.

The paired complete build-receipt archive SHA-256 is
`4d78d69b84a016d833b1c551c93f05cd9dae358bfad1c7c3a43dd0884b95486e`;
the final Slurm accounting receipt SHA-256 is
`d357dc3aea3501ae8ebcb008692f7d795255801096aa00fa4c781e1a51ef5827`.
Raw exports, native receipts, logs and archived databases remain under
`target/discovery-full-cirrus-20261006`. These successful builds establish
compilation and build acceptance for this snapshot. They do not execute the
workspace tests or MPI consumers and do not establish cluster runtime
acceptance.

## Additional HDF5 finalization correction

A later bounded review found that successful pkg-config discovery can supply a
usable header but no library search paths, for example `Libs: -lhdf5` without
`-L`. The locked `hdf5-metno-sys` 0.12.4 dependency then derives `lib` and `bin`
from the parent of the include directory that actually supplied `H5pubconf`.
Our loader-path helper previously returned an empty list instead. This could
omit the runtime path for the dependency's selected nonstandard installation.

A diagnostic using the previously built GNU doctor and real installed HDF5
headers/library reproduced the empty pkg-config selection. Selecting the same
fixture with explicit `HDF5_DIR` found its library; that was a diagnostic control,
not an alternative configuration prescribed to users. The correction applies
the dependency's finalization only when its search list is empty. It uses the
selected header's include root, preserves existing nonempty paths, and keeps
only directories containing the platform's shared-library filename.

The new regression supplies an unrelated first include, the actual header in a
second include, paths containing spaces, and both `lib` and `bin`. It checks
header selection, ordered library paths and the emitted Linux RUNPATH. This is
a discovery/argument test; its file-presence fixtures do not claim HDF5 ABI or
loader execution. Normal builds retain the locked dependency's runtime checks.

| Check | Result |
| --- | --- |
| New regression before correction | Failed: selected library paths were empty; exit 101 |
| First post-fix HDF5 suite | 7 passed, 1 failed; new test incorrectly expected separate RUNPATH flags |
| Final HDF5 suite | 8 passed, 55 filtered; exit 0 |
| Complete `quest-build` tests after correction | 63 passed, none failed/skipped; zero doctests; exit 0 |
| `quest-build` all-target/all-feature Clippy | Passed with `-D warnings` |
| Changed-file formatting and whitespace | Passed |

The intermediate failure is retained. Production selection was already correct;
the test expectation was corrected to the existing colon-joined Linux RUNPATH
contract. All Cargo checks were locked and offline. The complete package check
used `cargo test --locked --offline -p quest-build --all-features`, GCC/G++, and
its separate `target/safe-supervisor-20261006/cargo` target. Focused checks used
`target/hdf5-discovery-delta`; no broad acceptance campaign was rerun for this
individual correction.

| Artifact | SHA-256 |
| --- | --- |
| Previous `crates/quest-build/src/hdf5.rs` | `ed7eeaef5ba99319d891478fccacd5533ca53b25a3f02063fbebf9ca11be87b6` |
| Corrected `crates/quest-build/src/hdf5.rs` | `71fb027e017144ec73f78131bd0ab20a00eec86e42ff032bef239c33077ac357` |
| Failing regression log | `77c83c6c7ecf8057b5836033898320e467826bc749903e5828692b93d7a19eae` |
| Final focused suite log | `01cf78867085c7be75b069a4c34cd971a621d6cc697270f3dbf1f24c31befa01` |
| Complete 63-test package log | `ab3a7941b00eba5baee15a3846c162966bdb121abcce285dc1d80248c7d4376d` |

This correction follows both the successful GNU/full-Cirrus-build snapshot
`565f59ccd796712da5277c57a4203d0869f0ad67a26fdec47e99709b83dc2602`
and the [Cray host-policy build](2026-10-06-cargo-host-policy.md) snapshot
`e68d5468b006d0dd3c5bbb6420d9a5c5db5a5a0727aafd1b8bd93540055e32d0`.
Neither earlier snapshot's acceptance covers this change. The combined corrected
source is now frozen as
`4e5b423ebf3db42db19c3f75481ad786a527117eecd4d41c8b9a9b9dcebd6cf1`;
its full workspace/cluster acceptance remains a separate gate. No native QuEST
source, dependency or unsafe Rust was added for this HDF5 correction. It does
not explain or repair the earlier two Clang CMake failures. The JSON retains the
older source identities and results separately; new raw receipts are under
`target/portability-review-20261006` and `target/safe-supervisor-20261006`.

## Concurrent binding discovery

Two direct invocations of the existing GNU `xtask` executable ran
`generate-quest-bindings --check` concurrently from immutable source manifest
`452a49852c4e23f018b82795c19a3fd183c781234f2914df2c0ab516d696918f`.
Both used `target/native-reviewed-gnu-20261006/cargo` as `CARGO_TARGET_DIR`,
the unchanged GNU QuEST installation, system MPICH, GCC/G++ and the ordinary
Clang parser. No source changes, additional compiler wrappers or instrumentation
were introduced.

At `2026-10-06T20:04:14.128464054Z`, an ordinary Bash glob observed two distinct
`xtask-binding-native-discovery-*` directories directly under that Cargo target.
Both direct `xtask` processes were alive before and after the directory check,
and `ps` recorded both commands. Each check exited 0. The initial and final
directory counts were both zero; no manual cleanup was performed. All 2,594
source-file hashes and the executable, native library, package configuration and
compiler input hashes still matched after both processes completed.

| Artifact | SHA-256 |
| --- | --- |
| Existing GNU `xtask` executable | `62250dd4c81433f8b1c6ac1d48c4cb6b6a54ea0656991eb92c03774164f240c4` |
| Unchanged GNU `libQuEST.so.4.3.0` | `2128fd71d6effd5ae463f9f92ff467032f09089a07cdb9ba7a5d565719b95396` |
| Concurrency summary | `8293a8adf6d323f55d5a1d948d648cd06eb71b64740ef11112744815fd670e6e` |
| Raw receipt index, 49 files | `7cf0b083e5f1b340638dcc1f9f0b3e24eca2f3d830d17b64d321af7e0a831d99` |

Final-worktree `cargo fmt --all -- --check` and `git diff --check` also exited
0. Exact commands, environment, launch and wait timestamps, process and directory
observations, source checks and statuses are retained under
`target/binding-concurrency-20261006-evidence`. This closes the observed local
concurrent-isolation and normal RAII-cleanup case. It makes no performance,
failure-path-cleanup or additional-platform acceptance claim.
