# Installed-target native build verification — 2026-09-10

This record supersedes the setup recipe, not the historical evidence, in the
[compiler verification record](2026-09-10-m6-m11.md). The Rust baseline is
`2c2135d`; the native baseline is QuEST `411b762c` with the RPATH property fix.
For current setup, use the [build guide](../../README.md#build).

## Native configuration and initial failure

The installation is `/var/home/erich/Projects/opt/quest`, QuEST **4.3.0**, shared,
binary64, deprecated APIs disabled. Its enabled features include OpenMP, MPI,
subcommunicators, CUDA, cuQuantum and BMI2; HIP, ADIOS2 and NUMA are disabled.
The existing native build uses GCC 15 and MPICH 5.0.1; the Rust CXX bridge uses
the selected system C++ compiler (GCC 16.2.1). Public target requirements discover
the same MPICH installation. This is a native Linux x86_64 GNU verification.

The updated imported target exports installed includes, C/C++ language features
and the public `MPI::MPI_CXX` dependency. Its shared-library private SDK
dependencies need no consumer-side replacement targets. However, the initial
installed library had only `$ORIGIN/` RUNPATH and no bundled SDK libraries. A
plain CMake executable using only `find_package(QuEST 4.3 CONFIG REQUIRED)` and
`target_link_libraries(... QuEST::QuEST)` failed to link with loader variables
unset because cuStateVec/cuBLAS could not be resolved.

The native fix appends the relative RPATH entry and respects initialized
`INSTALL_RPATH` and `INSTALL_RPATH_USE_LINK_PATH`. Its regression first failed
for explicit RPATH and link-path opt-in; all three cases pass after the fix.
The existing native build was reconfigured with the sole cache change
`CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON`, rebuilt and reinstalled. The resulting
library RUNPATH contains `$ORIGIN/`, the actual CUDA target library directory,
cuQuantum's library directory and MPICH's library directory, with no CUDA
`stubs` directory.

The fresh target-only consumer then configured, built and executed with
`LD_LIBRARY_PATH`, `LD_PRELOAD`, `LD_AUDIT` and `LIBRARY_PATH` unset. Its CPU
probability check returned `0.99999999999999978`. Loader inspection resolves
QuEST, cuStateVec, cuBLAS/cuBLASLt, MPICH and OpenMP. Native packaging's
install/relocate/consume, behavior, configuration and RPATH checks passed **6/6**.

## Rust integration and migration

`cmake 0.1.58` builds a static CXX archive against `QuEST::QuEST`.
`cmake-package 0.2.0` provides the Cargo build-script discovery preflight; it
does not supply flattened compiler/linker flags. A small CMake File API query
uses the same native project, compiler and profile to obtain evaluated link and
generator header context. CMake owns compile features, system includes and
conditional target requirements.

The cmake-package preflight requires Cargo's `OUT_DIR`; tooling instead calls
the same authoritative CMake project using its explicit work directory. No
unsafe environment mutation or process-global reporting hook is introduced.
The `cmake` crate's panic-based build failures are converted at a narrow typed
error boundary.

Final consumers emit evaluated linker options and direct-library `DT_RUNPATH`
entries through `quest-build`. Installed libraries resolve their own private
dependencies. Native SHA records, `configure-native`, production `ldd`, the
Python native-consumer harness and forced legacy `DT_RPATH` are removed.
Obsolete environment variables produce migration errors. The unrelated Python
mathematical-oracle fixture generator remains an optional research utility.

Standard Debug CMake compilation also exposed an unused generated
`from_qcomp_vec` helper whose nonexistent CXX vector specialization was hidden
by the former function-section elimination. The unused helper was removed from
the generator template and regenerated source; no adapter semantics changed.
Regenerated coverage now records `initCustomMpiCommQuESTEnv` as MPI-gated because
the current installed public headers expose it.

## Reproduction and acceptance

The historical checks used pinned `nightly-2026-09-06` and:

```sh
unset QUEST_NATIVE_CONFIG QUEST_RUNTIME_LIBRARY_PATH
unset LD_LIBRARY_PATH LD_PRELOAD LD_AUDIT
export QUEST_ROOT=/var/home/erich/Projects/opt/quest
export CARGO_TARGET_DIR=/var/home/erich/Projects/quest-rs/target
export CARGO_BUILD_JOBS=4
```

| Check | Final result |
| --- | --- |
| `cargo build --workspace --all-features --locked --offline` | Passed. |
| `cargo nextest run --workspace --all-features --locked --offline` | **333 passed, zero skipped**; includes 19 build-helper and 20 xtask tests. |
| `cargo test --doc --workspace --all-features --locked --offline` | **13 passed**, one intentionally ignored final-package `build.rs` snippet. |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | Passed under the existing strict workspace policy. |
| `cargo fmt --all -- --check` and `git diff --check` | Passed. |
| `cargo run -p xtask --locked --offline -- generate-quest-bindings --check` | Passed using the installed target's MPI-enabled header context and matching LLVM resources. |
| `cargo run -p xtask --locked --offline -- check-native-consumers --work-dir "/tmp/quest native cmake final consumers"` | All four consumers built and ran successfully outside Cargo, with loader variables unset. |
| Facade `minimal`, `bell_circuit`, `tutorials`; bridge `min_example` | All four examples passed with `--locked --offline`; facade examples also enabled `--all-features`. |
| `cargo package --workspace --list --allow-dirty --locked --offline` | Passed for all 12 workspace packages; native CMake sources included, obsolete Python harness absent. |
| `mdbook build docs/book` | Passed with mdBook 0.5.4, installed from cached crates with `--locked --offline`. |

For the all-features test run, `QUEST_TUTORIAL_WORKER` identified the freshly
built `target/debug/quest-optimizer-worker`. Native bridge tests that initialize
MPI ran with local network access outside the filesystem sandbox. The independent
consumer harness explicitly selects CPU execution and also passed inside the
sandbox. It records separate build, ELF, dependency-resolution and execution logs
for direct `quest-sys`, facade, wrapped and renamed consumers.

Every independent executable has `DT_RUNPATH`, with no `DT_RPATH`. Its dependency
inspection resolves QuEST and the installed cuStateVec, cuBLAS/cuBLASLt, MPICH
and OpenMP libraries. The executable runtime paths cover direct dependencies;
the repaired QuEST installation supplies the private SDK paths. The preserved
fixture directory contains spaces, exercising source, include, CMake and Cargo
path handling.

The tutorials reported Bell probabilities of approximately 0.5, teleportation
fidelity 1, feedback probability 1, one capture across four loop iterations and
array result 6. Separately, the bridge `min_example` automatically selected a
GPU-backed 20-qubit register and reported total probability
`0.9999999999999999` after random-state initialization. Its environment reported
`CUDA=1 OpenMP=1 MPI=1`, one MPI rank and `cuQuantum=1`. This is a limited native
GPU execution smoke check, distinct from the explicitly CPU-only independent
consumer acceptance and its GPU dependency-resolution checks.

The build-helper tests exercise real evaluated generator expressions, paired
linker arguments, missing imported dependencies, ABI precision rejection and
Debug/Release compiler consistency. They reject unsupported linker state and
library-name collisions, including an unreferenced shadow library in an earlier
search directory. Reusing a discovery directory after changing package selection
is tested; fresh CMake configuration prevents stale `QuEST_DIR` cache selection.
Rebuild tracking retains both symlink lookup paths and canonical input paths.
Tooling tests cover obsolete-variable diagnostics, LLVM context, preserved
dependency prefixes and refusal to overwrite a nonempty consumer fixture.

The pure frontend/compiler check passed in a fresh target directory with native
tool paths deliberately unavailable:

```sh
env QUEST_ROOT=/quest-native-unavailable CMAKE=/quest-cmake-unavailable \
  LIBCLANG_PATH=/quest-libclang-unavailable CXX=/quest-cxx-unavailable \
  CARGO_TARGET_DIR=/tmp/quest-native-cmake-pure-check CARGO_BUILD_JOBS=4 \
  cargo check --offline --locked -p quest-language -p quest-qasm \
  -p quest-circuit -p quest-macros --no-default-features
```

## Limits of this evidence

Native dependency resolution is distinct from GPU kernel execution. Beyond the
GPU smoke check described above, no dedicated GPU correctness/performance suite,
distributed simulation, full native QuEST numerical suite, cross-compilation,
macOS or Windows validation is claimed. Native packaging
relocation tests passed; this does not establish relocatable Rust application
bundles or validation of every native feature combination. The supported linker
translation rejects stateful static-linker groups. Arbitrary downstream
`RUSTFLAGS` and Cargo configuration can introduce additional link search paths
outside the evaluated native project's contract.

## Relocated CPU package

The complete workspace suite also passed using the relocated CPU package at
`/tmp/quest-rpath-package-build/tests/packaging/work/install/relocated prefix`:
**333 Nextest tests passed, zero skipped; 13 doctests passed, one intentional
ignore**. This package is QuEST 4.3.0, binary64, deprecated APIs disabled, with
GPU, MPI and OpenMP disabled. The Rust source was unchanged by this verification.

A subsequent native installation appeared at `/var/home/erich/Projects`.
Its `lib64/libQuEST.so.4.3.0` had only `$ORIGIN/` RUNPATH; loader inspection could
not resolve cuStateVec, cuBLAS/cuBLASLt or MPICH. The test runner consequently
could not enumerate native tests against that installation. This is a new native
installation limitation; it does not replace the earlier GPU-enabled acceptance
evidence above. Its setup needs the documented
`CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON` installation step before those libraries
can load without loader environment variables.

## Repaired GPU installation revalidation

The loader failure above was resolved by repairing the native installation.
Rust source at `fcdcf0c` was revalidated against
`QUEST_ROOT=/var/home/erich/Projects`. The native repository was at `adfd00d6`;
its build cache records `CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON`. Installed headers
report QuEST 4.3.0, binary64, deprecated APIs disabled, and CUDA, cuQuantum, MPI,
OpenMP, subcommunicators and BMI2 enabled. The installed NUMA flag remains off.

The installed library now records `$ORIGIN/`, the actual CUDA target library,
cuQuantum and MPICH directories in `DT_RUNPATH`. With `LD_LIBRARY_PATH`,
`LD_PRELOAD`, `LD_AUDIT` and `LIBRARY_PATH` unset, all dependencies resolve,
including cuStateVec, cuBLAS/cuBLASLt and MPI. No CUDA stub directories or legacy
`DT_RPATH` are present.

Fresh checks against this repaired installation passed:

| Check | Result |
| --- | --- |
| Workspace all-features build, `--locked --offline` | Passed. |
| Workspace all-features Nextest, `--locked --offline` | **333 passed, zero skipped**. |
| Workspace all-features doctests, `--locked --offline` | **13 passed**, one intentional ignore. |
| Workspace all-targets/all-features Clippy, `--locked --offline -- -D warnings` | Passed. |
| Binding generator `--check` | Passed with the installed MPI header context. |
| Rust independent-consumer harness | Direct `quest-sys`, facade, wrapped and renamed consumers all passed ELF, dependency-resolution and numerical checks outside Cargo. |
| Fresh target-only CMake consumer | Configured, built and executed through `QuEST::QuEST`; CPU Bell norm `0.99999999999999978`, with equal approximately 0.5 probabilities for 00 and 11 and zero for 01 and 10. |
| GPU execution smoke check | The `quest-sys` example ran outside Cargo using a **20-qubit GPU-backed state vector**; total probability was **1**. |

The target-only consumer used GCC 15.3.1; Rust bridge discovery and compilation
used the same selected `/usr/bin/c++` compiler as their final Rust consumers.
The CMake consumer's RUNPATH contains only QuEST/MPI directories: its indirect
CUDA/cuQuantum dependencies resolve through the installed library. Both the
independent consumers and the GPU example ran with the loader variables above
unset. The GPU example reported `CUDA=1 OpenMP=1 MPI=1`, one MPI rank and
`cuQuantum=1`. This remains a GPU smoke check, not a full GPU or distributed
simulation validation suite.
