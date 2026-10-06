# Linux native GNU validation

All twelve stages of this local GNU campaign passed against frozen source
`565f59ccd796712da5277c57a4203d0869f0ad67a26fdec47e99709b83dc2602`.
The campaign ran from 18:50:23 to 19:05:06 UTC on 2026-10-06. It used a fresh
Cargo target and disk-backed temporary directory, with no source changes,
native-library changes, package installation or failed-stage retries.

This snapshot includes the [native discovery corrections](2026-10-06-native-discovery-followup.md).
The earlier local Clang campaign's two installed-CMake module-open failures
remain unexplained. This successful GNU run uses a different compiler, native
installation and execution context; it does not establish a cause or repair for
those failures.

## Recorded configuration

The installed QuEST 4.3.0 library and the Rust CXX bridge use GCC 16.2.1
(Red Hat 16.2.1-2). Its native build cache records binary64 precision, MPI,
subcommunicators, OpenMP, NUMA, CUDA and cuQuantum support. Dependency metadata
identifies GNU `libgomp` and system MPICH. This campaign validates CPU/OpenMP
consumers and local MPI execution. Compiled CUDA/cuQuantum support and resolved
GPU-library dependencies do not establish GPU execution, which is not claimed.

| Component | Recorded version or selection |
| --- | --- |
| Rust target | `x86_64-unknown-linux-gnu` |
| rustc | `1.101.0-nightly`, commit `db8f076d2619ce2585b0380dda06e8da25a40da4` |
| Cargo | `1.101.0-nightly (f3865b2a4 2026-09-29)` |
| Native C/C++ compiler | GCC 16.2.1 |
| Binding driver and libclang | Clang 22.1.8 |
| CMake | 4.3.0 |
| MPI | MPICH 4.2.2; matching system `mpicc` and launcher |
| Serial HDF5 | 1.14.6, system `H5pubconf-64.h` layout |

Cargo commands were locked and offline. The driver selected eight Cargo build
jobs, four nextest workers, two OpenMP threads, zero development/test debug
information and disabled incremental compilation. Native GNU and MPI discovery
used ordinary compiler variables. The native bridge's independent witness
confirmed agreement between rsmpi and QuEST's MPI ABI and loaded library. The
doctor parsed 748 installed QuEST declarations with matching Clang/libclang.

## Completed stages

Every stage returned zero; the driver returned zero. The counts below retain
configured skips and ignored documentation examples.

| Stage | Result |
| --- | --- |
| All-feature workspace nextest | 1,708 passed, 6 skipped; 461.851 seconds |
| Default-feature workspace nextest | 1,515 passed, 1 skipped; 158.114 seconds |
| All-feature doctests | 65 passed, 1 ignored |
| Default-feature doctests | 55 passed, 1 ignored |
| All-feature workspace check | Passed |
| Default-feature workspace check | Passed |
| All-feature/all-target workspace Clippy | Passed with `-D warnings` |
| Binding freshness | Passed |
| Native doctor | All six stages passed |
| CPU/OpenMP consumers, module environment | 10 numerical cases and 2 native-mode checks passed |
| CPU/OpenMP consumers, loader isolation | 10 numerical cases, 2 native-mode checks and 4 executable dependency closures passed |
| Independent MPI consumer | Passed at 1, 2, 4 and 8 local ranks |

The existing `quest-polynomial` nightly `generic_const_exprs`/trait-solver
compatibility warning remains in the logs. No lint exception was introduced for
this campaign. The installed native library was reused; upstream QuEST's own
test suite was not rerun.

## Consumer and MPI scope

The two consumer modes compile with the selected compiler and loader
environment. The normal mode also preserves loader variables during numerical
execution. The separately invoked `--loader-isolated` mode removes loader
overrides for runtime checks and inspects ELF runtime paths and resolved native
dependencies. Both modes passed against the same existing installation, without
deployment changes. The four checked executable routes were direct `quest-sys`,
facade, wrapped and renamed consumers.

The [independent MPI consumer](fixtures/native-mpi-consumer/README.md) checks every
local amplitude of its five-qubit Bell state, including phase and spectator
coordinates, at absolute tolerance `1e-13`. It also checks probabilities,
partition/deployment metadata, MPI usability after QuEST cleanup and finalization
by the runtime owner. All four launches finished within their 60-second
deadlines, with no timeout or truncated output. This MPI stage retained the
normal loader environment; separate MPI execution with loader overrides removed
was not part of this campaign.

Two OpenMP threads were requested. Deployment flags were checked; actual team
size and parallel speedup were not measured. Local rank counts do not establish
multi-host capacity, and this result does not extend to Cirrus or macOS.

## Source and artifact identities

The source manifest contains 2,584 files; its SHA-256 is the snapshot identity
above. The source archive SHA-256 is
`b9256d7c0cbb72b7b2477b782c8e5666cde150564b261dd78f51cee5e2b80f5b`.
Every manifest entry still matched after the campaign. The recorded installed
library, package configuration and GCC executable hashes also remained unchanged.
The native library SHA-256 is
`2128fd71d6effd5ae463f9f92ff467032f09089a07cdb9ba7a5d565719b95396`.

The [machine-readable summary](data/2026-10-06-native-gnu/summary.json) records
commands, timestamps, counts, toolchain/native input identities and raw log
hashes. Raw evidence and the Bash driver remain under
`target/native-gnu-20261006`; frozen source and its archive remain under
`target/discovery-full-cirrus-20261006`. Published paths are generic. These
documentation files were written after the frozen campaign and are not included
in its source manifest.

## Reviewed-source follow-up

A second GNU campaign tested the three later corrections described in the
[reviewed-source ledger](2026-10-06-portable-reviewed.md). Its original driver
ran from 19:29:00 to 19:43:59 UTC on 2026-10-06 and **finished with exit 1**.
Eleven stages passed; the standalone MPI consumer build failed with exit 101
because its separate lockfile omitted the supervisor's new direct `rustix`
dependency. Cargo rejected the locked build before that consumer executed.
This failed driver remains recorded independently of the correction below.

| Frozen source | Manifest SHA-256 |
| --- | --- |
| Original reviewed source, 2,594 files | `4e5b423ebf3db42db19c3f75481ad786a527117eecd4d41c8b9a9b9dcebd6cf1` |
| Corrected standalone consumer lockfile, 2,594 files | `452a49852c4e23f018b82795c19a3fd183c781234f2914df2c0ab516d696918f` |

The follow-up used a fresh Cargo target and disk-backed temporary directory.
Compiler/tool versions and the four recorded native/compiler input hashes match
the earlier GNU campaign. The existing GNU QuEST installation, matching MPICH,
eight Cargo build jobs, four nextest workers and two requested OpenMP threads
were retained. Commands remained locked and offline; no native installation or
library changes were made.

| Stage on original reviewed source | Result |
| --- | --- |
| All-feature workspace nextest | 1,710 passed, 6 skipped; 482.810 seconds |
| Default-feature workspace nextest | 1,516 passed, 1 skipped; 163.869 seconds |
| All-feature doctests | 65 passed, 1 ignored |
| Default-feature doctests | 55 passed, 1 ignored |
| All-feature and default-feature workspace checks | Both passed |
| All-feature/all-target workspace Clippy | Passed with `-D warnings` |
| Binding freshness | Passed |
| Native doctor | All six stages passed |
| CPU/OpenMP consumers, module environment | 10 numerical cases and 2 native-mode checks passed |
| CPU/OpenMP consumers, loader isolation | 10 numerical cases, 2 native-mode checks and 4 dependency closures passed |
| Independent MPI consumer | Build failed with stale lockfile; exit 101; runtime unrun |

The consumer lockfile was refreshed separately, preserving all Rust and C++
source files. Manifest comparison confirms that
`docs/verification/fixtures/native-mpi-consumer/Cargo.lock` is the only changed
file between the two frozen snapshots. A separate invocation on the corrected
snapshot ran from 19:44:47 to 19:45:08 UTC and returned zero:

```bash
cargo run --locked --offline \
  --manifest-path docs/verification/fixtures/native-mpi-consumer/Cargo.toml
```

That corrected consumer verified 1, 2, 4 and 8 local ranks. Every launch returned
zero, without timeout or truncated output; launch durations were approximately
547, 542, 562 and 608 milliseconds. It retained ordinary loader settings and the
same numerical, deployment and lifetime checks described above. No separate
loader-isolated MPI run, GPU execution, measured OpenMP team size or multi-host
capacity result is claimed.

Both 2,594-file source manifests and the native/compiler identities passed
post-execution verification. The original source archive SHA-256 is
`50b5a4a7727754f77282c2597d46e1cf6709bee8506ae63b5c7d05310a08b526`;
the corrected archive SHA-256 is
`46636f8a311fa221d0754f4042a961e2191d756597ea2fe1ce97f50d1f2898a7`.
The original twelve-stage Bash driver and all raw stage receipts remain under
`target/native-reviewed-gnu-20261006`. The corrected-consumer invocation has its
own command, source digest, timestamps, log and status; it is not part of that
original driver. The JSON records all 92 evidence files and 36 consumer logs.

The eleven original passes and the separately executed corrected consumer
provide local GNU evidence for the planned checks across these explicitly
identified snapshots. They do not turn the original driver into a pass or claim
a complete workspace rerun on the corrected snapshot. The earlier successful
`565f59…` campaign remains unchanged above. The earlier Clang module-open failures
remain unexplained, and Cirrus, macOS and enforced capacity retain their own
acceptance requirements.
