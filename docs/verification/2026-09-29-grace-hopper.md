# Grace Hopper verification

The workspace builds and executes against the installed QuEST package on the
single-node Grace Hopper host `gh02`. CPU, OpenMP and GPU register deployment,
including cuQuantum, passed Rust consumer checks outside Cargo. The setup uses
manual and Spack dependencies; see the [reproduction guide](../grace-hopper.md).

## Changes

- Install the existing QuEST build with dependency runtime paths. Track CUDA,
  cuQuantum and compiler search-path environment changes through shared native
  discovery, leaving CMake's imported target authoritative.
- Preserve requested GPU and threading modes separately from capabilities.
  Local registers pass `Enabled=1`, `Disabled=0`, and `Auto=-1` to native
  allocation. This also applies to sampling and internal QSVT registers.
  Cloning retains native deployment; collective allocation policy is unchanged.
- Convert pure states to density matrices with native `init_pure_state`, avoiding
  amplitude export/reimport and preserving subnormalized complex states.
- Share numerical-matrix identity between admission and materialization. Charge
  each storage pointer and ordered control-sign profile once, while retaining
  per-instruction storage and conservative preparation allowances. Program and
  oracle caches remain separate.
- Extend independent consumer validation with explicit backends and add AArch64
  FPCR policy detection/restoration coverage. Refresh rolling-nightly lint and
  compile-fail diagnostics without relaxing lint policy.

`Enabled` now requires the selected mode for small registers too. Callers wanting
QuEST's size thresholds must use `Auto`. Public method signatures are unchanged.
HDF5 explicit-prefix discovery still follows `hdf5-metno-sys`'s `lib`/`bin`
layout; pkg-config selects alternative layouts, including `lib64`.

## Installed configuration

| Component | Verified value |
| --- | --- |
| Host | Linux AArch64 GNU, NVIDIA GH200 480GB, driver 580.95.05 |
| Rust | Rolling nightly, `rustc 1.101.0-nightly (c1070d693 2026-09-28)`, LLVM 23.1.1 |
| Native C/C++ compiler | Spack GCC 16.2.0 |
| CMake | 4.4.2 |
| CUDA | 13.4.92, architecture 90 |
| cuQuantum / cuStateVec | 26.06.0.17 / 1.14.0 |
| QuEST | 4.3.0, source `503552065045eaf89baba85e6cd6aad728525554` |
| QuEST configuration | Shared, binary64, CUDA/cuQuantum/OpenMP enabled; MPI, subcommunicators and deprecated APIs disabled |
| QuEST installation | `/path/to/installed/quest`, `CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON` |
| HDF5 | System serial 1.10.7, selected through pkg-config |
| Parser tools | Matching Clang/libclang 23.1.2, `/path/to/quest-tools` |
| Nextest | 0.9.146 |

The existing build requested NUMA but did not find libnuma; it continued without
NUMA support. No native feature settings were changed other than installation
runtime paths. The installed `libQuEST.so` resolves CUDA, cuStateVec and Spack GCC
dependencies with loader overrides cleared. Runtime paths exclude CUDA stubs.

Verification used:

```sh
export QUEST_ROOT=/path/to/installed/quest
export CARGO_TARGET_DIR=/path/to/quest-rs/target
export CC=/path/to/gcc-16/bin/gcc
export CXX=/path/to/gcc-16/bin/g++
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$CC"
export CLANG=/path/to/quest-tools/bin/clang
export LIBCLANG_PATH=/path/to/quest-tools/lib
unset HDF5_DIR
```

The selected Spack compiler directory was on `PATH`; the build inherited
`CC=gcc` and `CXX=g++`, resolving to the absolute paths above. Selecting that same
GCC as Rust's linker fixes the observed `CXXABI_1.3.15` link failure from combining
the system GCC 11 linker with the GCC 16 C++ bridge.

## Results

Exact commands and exit statuses are in [checks.json](data/2026-09-29-grace-hopper/checks.json).
The feature set includes QSVT, ndarray, workers, serde, codespan reporting,
offline synthesis, Rayon, SIMD, synthesis/ZX/MITM workers, and default
certification/HDF5 support. It excludes MPI.

| Final check | Result |
| --- | --- |
| Locked workspace build, default and expanded features | Passed |
| Default workspace Nextest | 712 passed, 1 explicitly ignored scale test |
| Expanded-feature workspace Nextest | 800 passed, 3 explicitly ignored scale tests |
| Default / expanded-feature doctests | 47 / 49 passed; 1 ignored build-script snippet in each |
| Default / expanded-feature Clippy, all targets, `-D warnings` | Passed |
| Formatting and diff whitespace | Passed |
| Generated binding freshness | Passed; generated artifacts unchanged |
| Direct, facade, wrapped and renamed standalone consumers | All four passed RUNPATH and complete native dependency closure |
| Explicit CPU, OpenMP and GPU execution | All requested backends passed deployment and numerical checks in fresh processes |
| GPU requested with `CUDA_VISIBLE_DEVICES` empty | Direct and facade consumers exited 1 with no-GPU diagnostics |
| Register conversion modes | CPU, OpenMP, GPU and Auto mixed-deployment tests passed |

The standalone consumer command was:

```sh
cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp,gpu \
  --work-dir /path/to/quest-rs/target/grace-hopper-consumers-final
```

Consumer binaries ran outside Cargo with `LD_LIBRARY_PATH`, `LD_PRELOAD` and
`LD_AUDIT` cleared. [Consumer evidence](data/2026-09-29-grace-hopper/consumers.json)
preserves run output and ELF dependency inspection. The direct GPU state-vector
and density-matrix checks reported maximum errors of approximately `1.11e-16`
and `2.22e-16`, respectively, below the explicit `1e-12` tolerance, and total
probability `0.9999999999999998`. Both reported `cuquantum=true`.

The register regression ran with `QUEST_REGISTER_TEST_MODE` unset, then set to
`threads`, `gpu` and `auto`:

```sh
cargo test -p quest-rs --test register_modes --locked --quiet
```

It covers state-vector/density allocation and clones, complex subnormalized
conversion, source immutability, tight-budget success, allocation rollback and
retry. The opt-in Auto case verifies a six-qubit CPU state vector converting to
a GPU density matrix under this QuEST configuration. GPU and Auto checks used
host device access because the sandbox hides NVIDIA devices.

Shared-matrix regressions cover repeated storage across target orders and signed
control profiles, separate charging for distinct profiles, conditional
operations, and failed preparation preserving allocations and existing programs.
The AArch64 adapter test modifies FPCR, detects the changed numerical policy,
and restores the original control word.

## Scope

The three pre-existing ignored scale tests cover degree-8192 interval FFT,
dense degree-8105 offline synthesis, and degree-8105 parallel acceptance. They
were not run. The ignored doctest is a build-script snippet exercised by the
independent consumers. MPI, multi-node execution and Darwin runtime validation
are outside this pass. The existing Nix workflow is retained but was not executed
on this node. Matrix budgets remain conservative estimates, not allocator-exact
peak measurements. Broader optimizer fallback cleanup and crate restructuring
remain deferred.
