# Linux native Clang and MPI validation

Local Linux validation is complete for the recorded configurations. The baseline
workspace, normal downstream consumers and independent MPI consumer passed.
Loader-isolated consumers failed for the baseline native installation; that
failure remains part of the result. A separate documented fixed-SDK installation
passed scoped isolated-consumer and MPI checks.

The [reviewed-source follow-up](#reviewed-source-workspace-and-consumer-follow-up)
below records later complete workspace coverage against that fixed-SDK library,
with a standalone-consumer lockfile correction retained as separate evidence.

QuEST 4.3.0 was configured, built and installed from unchanged native revision
`503552065045eaf89baba85e6cd6aad728525554` using Clang 22.1.8. This local Linux
x86_64 installation enables double precision, MPI, subcommunicators, OpenMP and
NUMA; GPU backends and upstream QuEST examples/tests were disabled. Rust uses
`rustc 1.101.0-nightly (db8f076d2 2026-10-03)`, system MPICH and serial HDF5 1.14.6.
The native package and Rust CXX bridge both use Clang. MPICH's C wrapper invokes
GCC, and GNU libstdc++ remains a dependency.

The native CMake cache selects `-fopenmp=libomp`. Saved dependency trees for the
installed library and independent MPI consumer contain LLVM `libomp.so` and no
GNU `libgomp`. This supplies native Clang/LLVM OpenMP evidence beyond the earlier
Clang-bridge receipt, which used a GCC-built QuEST library and GNU OpenMP. It does
not claim that MPI or every transitive dependency was rebuilt with Clang.

## Baseline verification

The workspace all-feature Nextest run passed 1,706 tests, with six explicit skips,
in 371.141 seconds. Native configure/build/install, native doctor, and the
independent MPI consumer's build, execution and scoped strict Clippy have also
passed. Cargo verification uses the isolated lane target and locked, offline
commands. Full commands, artifact hashes and stage statuses are in the
[summary](data/2026-10-06-native-clang/summary.json).
The full workspace ran against the baseline `native-install` prefix; its results
are not attributed to another native installation.

| Stage | Result |
| --- | --- |
| `native-configure` | passed |
| `native-build` | passed |
| `native-install` | passed |
| `mpi-consumer-lock` | passed |
| `mpi-consumer-build` | passed |
| `mpi-consumer-run` | passed |
| `mpi-consumer-clippy` | passed |
| `doctor` | passed |
| `all-features` | 1706 passed, 6 skipped |
| `default-tests` | 1513 passed, 1 skipped |
| `doctests` | 65 passed, 1 ignored |
| `default-doctests` | 55 passed, 1 ignored |
| `default-check` | passed |
| `all-feature-check` | passed |
| `clippy` | passed |
| `bindings` | passed |
| `consumers` | 10 numerical cases passed; two additional mode checks passed |
| `consumers-isolated` | Failed: unresolved libmpicxx.so.12 before numerical execution |
| `fmt` | passed |
| `whitespace` | passed |

The existing Rust trait-solver compatibility warnings remain in the workspace
logs. Upstream QuEST's own test suite was not enabled; the numerical acceptance
here comes from the Rust workspace and downstream consumers.

## Independent safe Rust MPI consumer

A separate Cargo workspace declares the public `quest-rs` dependency with its
`mpi` feature, its own lockfile and the final-target runtime-path build script.
Its main and build script forbid unsafe code; that statement does not extend to
existing dependency/FFI internals. The fixture uses the public borrowed runtime,
communicator, environment, prepared-program and register owners.

At 1, 2, 4 and 8 local MPI ranks it prepares a five-qubit state with
`h q[0]; cx q[0], q[4];`. Every rank checks its entire local complex-amplitude
partition: global indices 0 and 17 contain positive `1/sqrt(2)` and every other
amplitude is zero, within absolute tolerance `1e-13`. Thus relative phase and
spectator sectors are checked alongside both 0.5 marginals, unit norm and a
zero-norm forbidden joint projection. Register rank, node count, partition length,
distribution and native multithreading metadata are checked collectively.

Register and plan destruction precede environment destruction. A successful
collective afterward proves that the borrowed MPI owner survives native cleanup;
communicator destruction then precedes runtime destruction, and finalization is
checked before the unique rank-zero success marker. The consumer coordinator
uses the existing supervisor and requires a successful launcher exit and exactly
one marker per rank count. All
four launches passed without timeout or output truncation, under 60-second
per-launch deadlines.

Two OpenMP threads per rank are requested. The native multithreading flag is
verified, but actual OpenMP team size is not measured. These small local runs
make no parallel speedup or multi-host claim.

## Preserved isolation failure and documented follow-up

Normal CPU/OpenMP installed consumers passed all ten numerical cases and both
additional native-mode checks. The baseline `native-install` prefix retains the
default relative runtime-search policy. Removing loader overrides exposed an
unresolved `libmpicxx.so.12` dependency, so the isolated consumer stage returned
status 1 during dependency inspection, before numerical execution. The original
installation and failure evidence are retained.

Native QuEST's `docs/cmake.md` at the recorded revision documents
`CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON` for installations tied to fixed external SDK
locations (lines 93 and 140-151). A separate `sdk-build` / `sdk-install` follows
that option; configure, build and install passed. Its native runtime search path
retains the external MPICH library directory, and the independent MPI dependency
tree resolves LLVM OpenMP and both MPICH libraries. It changes no native source
or vendor library.

| Separate fixed-SDK stage | Result |
| --- | --- |
| Native configure, build and install | passed |
| Loader-isolated CPU/OpenMP consumers | 10 numerical cases and four complete consumer closures passed |
| Independent MPI consumer build | passed |
| Independent MPI consumer execution | 1, 2, 4 and 8 ranks passed with loader controls removed |
| Public fixture manifest build and execution | passed separately at 1, 2, 4 and 8 ranks |
| Source inventory after SDK verification | all 2,569 entries still match |

Both SDK MPI executions finished without timeouts or output truncation. The
fixed-SDK driver removes every `LD_*` and `DYLD_*` variable, `LIBPATH` and
`SHLIB_PATH` before MPI execution; installed-consumer isolation uses the existing
corresponding policy. Two OpenMP threads remain a request, not a measured team
size. Full-workspace tests were not rerun as part of that scoped SDK follow-up;
the later workspace campaign is recorded below. These
scoped successes do not replace the baseline failure or change the separately
recorded unsupported Cirrus loader-isolation mode.

The rerunnable [independent MPI fixture](fixtures/native-mpi-consumer/README.md)
was subsequently built and executed from its public manifest. Its application,
build script and lockfile match the target-local fixture byte for byte; relative
dependency paths in its manifest reflect the public location. That separate run
uses the fixed-SDK prefix and the same amplitude, ownership and finalization
checks. Its source hashes and saved build/run logs are recorded separately.

## Source and acceptance boundaries

The before, baseline-after and SDK-after source inventories match exactly across
2,569 entries; both source-stability stages returned zero. Their hashes, the
current ABI source hashes, native library identity, consumer source/lockfile/binary
hashes, and saved log hashes are recorded in the summary. The workspace source
archive and baseline MPI consumer executable are preserved separately from
the SDK artifacts and later documentation changes. The SDK consumer executable was
also preserved before building the public fixture. The public fixture and this
receipt were added after the frozen source inventory and archive; the fixture has
its own recorded source hashes and build/run evidence.

These local receipts do not establish current-delta Cirrus or macOS acceptance.
Existing frozen Cirrus results do not validate this lane's source delta. GPU
execution, another MPI implementation, restricted matching-state admission, native-storage protocol
attestation and strict capacity closure are outside this result.

## Reviewed-source workspace and consumer follow-up

Current local Clang coverage passed across the reviewed workspace snapshot and
the separate corrected-consumer snapshot described below.

The later Clang campaign ran all twelve stages once, from
`2026-10-06T19:30:38Z` to `2026-10-06T19:44:28Z`, against immutable source
`4e5b423ebf3db42db19c3f75481ad786a527117eecd4d41c8b9a9b9dcebd6cf1`.
It used the existing fixed-SDK native installation unchanged, a fresh Cargo
target and disk-backed temporary directory, matching system MPICH, eight Cargo
build jobs, four nextest workers and two requested OpenMP threads. Cargo commands
were locked and offline, with development/test debug information and incremental
compilation disabled.

Eleven stages passed. The final independent-consumer stage failed before build
or MPI execution because its standalone lockfile was stale after the supervisor's
dependency change. Cargo correctly refused to update it under `--locked`;
that stage retains status 101 and the original campaign retains status 1.

| Stage | Reviewed-source result |
| --- | --- |
| All-feature workspace nextest | 1,710 passed, six skipped; 431.861 seconds |
| Default-feature workspace nextest | 1,516 passed, one skipped; 142.319 seconds |
| All-feature doctests | 65 passed, one ignored |
| Default-feature doctests | 55 passed, one ignored |
| Both workspace checks | Passed |
| All-feature/all-target Clippy | Passed with `-D warnings` |
| Binding freshness and native doctor | Passed |
| Normal CPU/OpenMP consumers | Ten numerical cases passed |
| Loader-isolated CPU/OpenMP consumers | Ten numerical cases and four complete executable dependency closures passed |
| Independent MPI consumer on the original snapshot | Failed before build: stale standalone lockfile; exit 101 |

Only the consumer was then run from the corrected immutable snapshot
`452a49852c4e23f018b82795c19a3fd183c781234f2914df2c0ab516d696918f`,
from `2026-10-06T19:45:34Z` to `2026-10-06T19:45:55Z`. An independent recursive
byte comparison confirmed that its sole difference from the workspace-tested
snapshot is `docs/verification/fixtures/native-mpi-consumer/Cargo.lock`; all Rust
and native source files are identical. The corrected locked/offline consumer
passed at one, two, four and eight local MPI ranks. Every launcher returned zero,
without timeout or output truncation. The workspace suites were not repeated.
This consumer execution retained the normal loader environment; the separate
loader-isolated CPU/OpenMP consumer stage is recorded above.

All 2,594 source entries matched before and after each applicable run. The native
library, package configuration and Clang executable identities also remained
unchanged. The reused library SHA-256 is
`daeaa610f45f0fec9527cb6c251253eadbb9f33342d521663a3ad2c5b9b3dcf1`.
The [summary](data/2026-10-06-native-clang/summary.json) records both source
identities, original and corrected lock hashes, stage statuses and raw evidence
identities. Logs and both drivers remain under
`target/native-reviewed-clang-20261006`; the verified 102-file evidence-index
SHA-256 is `0e29846045f9a65fc9469273d6866e64a710db191ee92b9d842eed2c304e67c2`.

The earlier [discovery-delta Clang campaign](2026-10-06-native-discovery-followup.md#workspace-campaign-and-unresolved-cmake-failures)
retains its two installed-CMake module-open failures. This later successful
workspace run neither explains their cause nor demonstrates a repair. It also
does not replace the baseline loader-isolation failure above, remove configured
skips or establish multi-host capacity, Cirrus runtime or macOS acceptance.
