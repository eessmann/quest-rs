# quest-build

Installed-QuEST discovery, CMake bridge compilation and final-executable linking.
The supported recipes are native Linux GNU and native `aarch64-apple-darwin` or
`x86_64-apple-darwin`, QuEST **4.3.x**, binary64 precision, deprecated APIs disabled,
CMake 3.28+ and a C++20 compiler. Darwin development uses shared CPU/OpenMP QuEST;
MPI and GPU deployments are not covered by the Darwin recipe. Cross compilation,
Windows, relocatable application bundles and stateful static-linker groups
remain unsupported.

## Select an installed package

```sh
export QUEST_ROOT=/path/to/installed/quest
cargo build --workspace --locked
```

`QuEST_DIR` can select the directory containing `QuESTConfig.cmake`;
`CMAKE_PREFIX_PATH` uses normal CMake package search. The legacy `QUEST_DIR` and
`QuEST_ROOT` aliases remain supported. Conflicting explicit selections fail.
Package discovery and compilation share the selected compiler and build profile.
The compiled admission source verifies the supported version and configuration.
On Darwin, `SDKROOT` must name an absolute installed macOS SDK; when absent,
`xcrun --sdk macosx --show-sdk-path` selects it. The validated SDK is supplied as
`CMAKE_OSX_SYSROOT` and shared with binding generation together with the evaluated
compiler implicit system includes. Global `CXX` can select a Nix compiler wrapper;
per-target compiler and header-search overrides remain rejected. Compiler target
checks normalize Clang `arm64` to Rust `aarch64` on Darwin.

For manual/Spack Linux builds, select a Rust target linker from the same GCC
installation as `CXX`; loading Spack's `gcc` may leave the system `cc` unchanged.
The [Grace Hopper recipe](../../docs/grace-hopper.md) includes this setting.
Native dependency roots (`CUDAToolkit_ROOT`, `CUDATOOLKIT_ROOT`, `CUDA_PATH`,
`CUQUANTUM_ROOT`) and compiler lookup through `PATH` are tracked Cargo inputs.

CXX generates the bridge sources; `cmake` builds a static C++20 archive linked
against `QuEST::QuEST`. This lets CMake evaluate compile features, system includes,
conditional flags and imported dependencies. One CMake configure selects and
evaluates the installed package; a CMake File API query then supplies its
evaluated link requirements without a second package-discovery pass. This
preserves generator expressions and library ordering from the native project.

Cargo and CMake track native input changes normally. There is no attested native
JSON record, compiler/library hashing or production `ldd` discovery. Unset the
removed `QUEST_NATIVE_CONFIG` and `QUEST_RUNTIME_LIBRARY_PATH` variables. The old
`xtask configure-native` command is replaced by direct package selection and the
consumer acceptance command below.

## Native runtime installation

The installed QuEST library must resolve its own dependencies, including private
CUDA/cuQuantum libraries. For a local development installation, reconfigure
**QuEST itself** with:

```sh
cmake -S /path/to/QuEST -B /path/to/existing/build \
  -DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON
cmake --build /path/to/existing/build
cmake --install /path/to/existing/build
```

This requires QuEST's `setup_quest_rpath()` to append its relative runtime path
and honor CMake's initialized RPATH properties. Default native packaging retains
relative `$ORIGIN` entries; explicitly opting into external link paths records
absolute SDK locations for development. CUDA stub directories are not runtime
libraries. The Rust build does not patch, copy or bundle installed libraries.

## Final executable build script

Cargo propagates native library dependencies through Rust libraries, but it does
not propagate arbitrary dependency build-script linker arguments to distant
executables. Add this to the **final executable package**, including applications
that reach QuEST through a wrapper Rust library:

```toml
[build-dependencies]
quest-build = "0.1"
```

```rust,ignore
// build.rs
fn main() -> quest_build::Result<()> {
    quest_build::emit_final_target_runtime_paths()
}
```

Despite its retained name, the helper emits both required evaluated native link
options and direct-library runtime paths. It and `quest-sys` must use the same
package selection and compiler configuration. Linux executables use ordinary
`DT_RUNPATH`; Darwin executables receive one Mach-O `LC_RPATH` per absolute
direct-library directory. Evaluated `.dylib` files retain their exact filename,
and framework pairs preserve library order. The serial HDF5 helper selects
`libhdf5.dylib` on Darwin while retaining its parallel-HDF5 rejection.
Indirect dependency deployment is the native installation's
responsibility. Unsupported linker-state constructs produce an error instead of
silently changing link semantics.

## Independent consumer acceptance

```sh
cargo run --locked -p xtask -- check-native-consumers
```

Use `--backends cpu,omp,gpu` to require and execute all three backends in fresh
processes, including actual state-vector/density placement and clone checks.
The default remains `cpu`; a requested unavailable backend fails.

The Rust harness builds an independent workspace containing direct `quest-sys`,
facade, wrapped and renamed consumers. It inspects platform loader metadata and resolved
libraries, then runs quantum checks outside Cargo with loader overrides removed.
Linux acceptance inspects ELF dynamic tags; Darwin acceptance uses Mach-O tools. These loader tools are acceptance dependencies,
not production discovery dependencies. `--work-dir PATH` selects the preserved
fixture/log directory. Actual GPU execution is separate from resolving GPU
runtime libraries.
