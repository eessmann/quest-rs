# Grace Hopper with manual and Spack dependencies

This recipe targets native `aarch64-unknown-linux-gnu` on one NVIDIA Grace Hopper
node. Nix is optional. QuEST owns CUDA/cuQuantum and OpenMP support; Cargo links
the installed CMake package. MPI is not enabled by this recipe.

## Select the toolchain

Load the GCC installation used to build QuEST, for example `spack load gcc@16.2.0`.
Use CMake 3.28+, rolling Rust nightly, pkg-config, and serial HDF5. Check both
`cc --version` and `gcc --version`: loading Spack GCC need not replace the system
`cc`, which Rust otherwise uses as its linker.

```sh
export CC="$(command -v gcc)"
export CXX="$(command -v g++)"
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$CC"
export QUEST_ROOT=/path/to/installed/quest
export CUDAToolkit_ROOT=/path/to/cuda
export CUQUANTUM_ROOT=/path/to/cuquantum
```

The paths above describe the tested node; substitute the corresponding installed
prefixes on another node. Keep the Rust linker and the C++ compiler on the same
GCC installation. A system GCC 11 linker with a GCC 16 C++ bridge can fail with
an unresolved `__cxa_call_terminate` / `CXXABI_1.3.15` symbol.

Use the serial system HDF5 pkg-config entry on this node:

```sh
unset HDF5_DIR
pkg-config --modversion hdf5
pkg-config --cflags --libs hdf5
h5cc -showconfig
```

Alternatively, select a serial Spack HDF5 installation through its pkg-config
directory (`lib/pkgconfig` or `lib64/pkgconfig` as installed):

```sh
export PKG_CONFIG_PATH=/path/to/serial-hdf5/lib64/pkgconfig
unset HDF5_DIR
pkg-config --cflags --libs hdf5
```

Confirm that the chosen header has `H5_HAVE_PARALLEL` disabled. Explicit
`HDF5_DIR` remains supported for prefixes with `include` and `lib`/`bin`, matching
`hdf5-metno-sys`. Use pkg-config for a `lib64` or multiarch layout; do not point
`HDF5_DIR` at `/usr` when headers are under `/usr/include/hdf5/serial`.

## Install QuEST

The existing source build already selects QuEST 4.3.0, binary64, CUDA architecture
90, cuQuantum and OpenMP, with MPI and deprecated APIs disabled. Preserve these
settings while enabling installed dependency runtime paths:

```sh
cmake -S /path/to/QuEST -B /path/to/QuEST/build \
  -DCMAKE_INSTALL_PREFIX="$QUEST_ROOT" \
  -DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON
cmake --build /path/to/QuEST/build --parallel 8
cmake --install /path/to/QuEST/build
env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT \
  ldd "$QUEST_ROOT/lib/libQuEST.so"
```

For a fresh QuEST configuration, also select `BUILD_SHARED_LIBS=ON`,
`QUEST_ENABLE_INSTALL=ON`, `QUEST_FLOAT_PRECISION=2`, `QUEST_ENABLE_CUDA=ON`,
`QUEST_ENABLE_CUQUANTUM=ON`, `QUEST_ENABLE_OMP=ON`, `QUEST_ENABLE_MPI=OFF`,
`QUEST_ENABLE_SUBCOMM=OFF`, `QUEST_ENABLE_DEPRECATED_API=OFF`, and
`CMAKE_CUDA_ARCHITECTURES=90`. Pass the selected C/C++ compilers and CUDA/cuQuantum
prefixes to CMake. The installed library must resolve its own indirect
dependencies; do not add CUDA stub directories to runtime search paths.

## Developer tools

```sh
cargo install cargo-nextest --locked
```

Binding checks require matching Clang and libclang major versions. Use an existing
Spack LLVM installation, or install them in a separate user-local prefix. The
tested node uses Conda only to provision these developer tools; no environment
activation is required:

```sh
conda create --prefix /path/to/quest-tools \
  --channel conda-forge --override-channels clang libclang --yes
export CLANG=/path/to/quest-tools/bin/clang
export LIBCLANG_PATH=/path/to/quest-tools/lib
```

Do not replace the selected Spack `CC`/`CXX` when selecting the parser tools.

## Build and validate

From the repository root, with the exports above:

```sh
cargo build --workspace --locked
cargo nextest run --workspace --locked
cargo test --doc --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p xtask -- generate-quest-bindings --check
cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp,gpu \
  --work-dir /path/to/quest-rs/target/grace-hopper-consumers
```

The consumer command builds direct, facade, wrapped and renamed applications,
then checks loader closure and executes them outside Cargo. Each backend gets a
fresh process. Requested missing backends fail instead of silently using the CPU.
Use a new or empty work directory on the large work filesystem: independent
consumer builds can require several GiB, and the node's `/tmp` is small. Evidence
is preserved there, so use a different directory name for the next run.
The default is `--backends cpu`; GPU execution requires access to `/dev/nvidia*`.
A container or sandbox may hide those devices even when `nvidia-smi` works on the
host; run GPU checks with device access enabled.

`Environment::builder().gpu(ExecutionMode::Enabled)` requires GPU registers,
including small ones. The same rule applies to `multithreading(Enabled)`.
`Disabled` prevents the mode; `Auto` retains QuEST's size thresholds. Existing
callers that enabled an environment but relied on small CPU registers should
select `Auto`. `Register::deployment()` reports actual placement.

State-vector promotion to density matrices uses native conversion and preserves
subnormalized states. Prepared numerical matrices share their native allocation
budget when storage identity and ordered control signs match. The default is CPU
execution without native multithreading.

Run optional features without MPI using:

```sh
cargo nextest run --workspace --locked --features \
  quest-rs/qsvt,quest-rs/ndarray,quest-rs/workers,quest-qsvt-cli/offline-synthesis,quest-qsvt-cli/rayon,quest-optimizer-worker/synthesis,quest-optimizer-worker/zx,quest-optimizer-worker/mitm
```

Record `rustc -Vv`, native compiler/library versions, driver information and
command results with each verification run. Rolling nightly can change Clippy
and compile-fail diagnostics; review those changes rather than disabling checks.

The [Grace Hopper verification record](verification/2026-09-29-grace-hopper.md)
contains the tested configuration, complete feature selection and results. See
the [documentation index](README.md) for the runtime guide and crate references,
or [Contributing](../CONTRIBUTING.md) for repository conventions.
