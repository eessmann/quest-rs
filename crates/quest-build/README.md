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

`QUEST_ROOT` must name the installation prefix itself, with `include/quest.h`
and an installed QuEST CMake package below it. A CMake package subdirectory is
not a prefix. `CMAKE_PREFIX_PATH` retains normal CMake package search and can
supply dependencies alongside an explicit `QUEST_ROOT`. The removed `QUEST_DIR`,
`QuEST_DIR` and `QuEST_ROOT` variables fail with migration guidance; unset them
and select the prefix through `QUEST_ROOT`.
`NativeBuildContext::from_cargo_env()` and `NativeBuildContext::for_tooling()`
capture the native target, profile and environment for package discovery and
bridge compilation. `NativeBuildContext`, `NativePackage`, `MpiSelection`, and
`SerialHdf5` expose borrowed getters rather than editable evaluated fields. Use
`context.request()` to obtain an editable `NativeBuildRequest`, then `capture()`
and discover again after edits. MPI compatibility checks reject an environment
that differs from the original discovery, and both witness processes receive
that captured environment.

Discovery returns data. Explicit `emit_cargo_input_metadata()`, link metadata,
and runtime-path helpers emit Cargo directives, including external input watches
without watching generated outputs. Standalone QuEST and HDF5 discovery do not
write Cargo directives to stdout. `build-probe-mpi` can still print pkg-config
invalidation directives; the doctor isolates this upstream probe in a child
with the captured environment so `native-doctor --json` always produces one
JSON receipt, including setup and dependency failures.

Tooling uses `clap` derives for command options, `cargo_metadata` for the selected
workspace and Cargo executable, and `rustc_version` for the selected Rust
compiler. `CARGO` and `RUSTC` are executable paths, including paths with spaces.
The CMake launcher remains a narrow fallible `Command` wrapper with captured
child environments and structured errors. The inspected `cmake` crate lacks
fallible execution and environment-clearing APIs, so it cannot replace that
launcher without weakening these requirements.
The compiled admission source verifies the supported version, configuration,
architecture, platform and pointer width using the actual selected toolchain.
On Darwin, `SDKROOT` must name an absolute installed macOS SDK; when absent,
`xcrun --sdk macosx --show-sdk-path` selects it. The validated SDK is supplied as
`CMAKE_OSX_SYSROOT` and shared with binding generation together with the evaluated
compiler implicit system includes. CMake selects the default compiler when no compiler is requested. Ordinary
`CC`, `CXX`, `CFLAGS`, `CXXFLAGS`, `CPATH`, compiler search variables and
`CMAKE_TOOLCHAIN_FILE` retain their native meanings. Cargo/cc-specific target
spellings such as `CXXFLAGS_<target>` remain unsupported because CMake does not
interpret them. Evaluated link constructs still have to satisfy the supported
Cargo lowering rules below.

The compiler invocation path and fixed arguments are retained separately from
its canonical identity: a wrapper selected as `CC` is invoked as `CC`, even if
that path is a symlink. No `-dumpmachine` command is required. Loaded-module
state (`PE_ENV`, `LOADEDMODULES`, Cray and Lmod inputs) and ordinary compiler and
header search variables are watched for Cargo rebuilds.

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
`libhdf5.dylib` on Darwin and recognizes standard Linux serial installations
without pkg-config metadata, including Fedora's `H5pubconf-64.h` layout. It
retains its parallel-HDF5 rejection and matches the locked HDF5 dependency's
discovery order.
Indirect dependency deployment is the native installation's
responsibility. Linux whole-archive scopes are lowered in library order. Each `-lNAME` inside
a scope is resolved using CMake's explicit and implicit search directories in
order, preferring `.so` over `.a` within each directory. A selected shared
library remains an ordinary shared dependency. A real archive uses Rust's
`static:-bundle,+whole-archive,+verbatim` modifiers, so its unreferenced members
remain included when linking a downstream executable through a Rust library.
Scoped inputs are identified from bounded file-header reads: archive signatures
or an ELF shared-object header. A filename suffix alone cannot establish that
removing the scope preserves its meaning. Linker scripts inside a scope are
rejected; ordinary shared-library linker scripts outside scopes remain supported.
Split `-Wl` options, bundled `-Wl,--whole-archive,-lNAME,--no-whole-archive`
options, `-Xlinker` forwarding and exact archive filenames are supported.
Exact-file shadow checks include bridge archive directories.

Unbalanced or nested scopes, unresolved scoped libraries, other order-sensitive
linker constructs and repeated exact libraries that Rust cannot represent with
link modifiers produce errors. Native archives are not copied or repackaged.

## Independent consumer acceptance

```sh
cargo run --locked -p xtask -- check-native-consumers
```

Use `--backends cpu,omp,gpu` to require and execute all three backends in fresh
processes, including actual state-vector/density placement and clone checks.
The default remains `cpu`; a requested unavailable backend fails.

The Rust harness builds an independent workspace containing direct `quest-sys`,
facade, wrapped and renamed consumers. It preserves the compiler/module
environment during compilation and, by default, during quantum checks outside
Cargo.

Test deployment without loader overrides separately:

```sh
cargo run --locked -p xtask -- check-native-consumers --loader-isolated
```

This mode inspects loader metadata and resolved libraries, then removes loader
overrides for execution. Linux inspection uses ELF dynamic tags; Darwin uses
Mach-O tools. These loader tools are acceptance dependencies, not production
discovery dependencies. Report ordinary and loader-isolated results separately:
a module-environment pass does not establish loader isolation. `--work-dir PATH`
selects the preserved fixture/log directory. Actual GPU execution is separate
from resolving GPU runtime libraries.


Native-link regression tests are workspace integration tests. Their minimal
C ABI Rust fixtures live under `quest-sys/tests/fixtures/linkage`, beside the
audited FFI boundary, and expose safe calls to their test consumers. Production
and packaged `quest-build` compilation does not consume those assets; running
the full native-link tests from a standalone source package requires the
workspace fixture assets.
