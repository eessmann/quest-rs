# Native tooling and diagnostics

The native build and tooling paths use the same `quest-build` context. Select
an installed QuEST with `QUEST_ROOT` or `CMAKE_PREFIX_PATH` in the compiler and
module environment that will build the consumer. Compiler wrapper names and
arguments are part of the invocation; resolving a wrapper symlink is useful
for identity checks but must not change the executable name used to compile.

## Diagnose the selected environment

```sh
export QUEST_ROOT=/path/to/quest-install
cargo run --locked -p xtask -- native-doctor --json > native-doctor.json
```

The receipt separates native context, CMake package and compiled ABI admission,
binding parser, MPI selection, and serial HDF5 selection. It records the compiler
invocation separately from its canonical executable, installed SDK capabilities,
header search context, MPI discovery source, and parser configuration. Failed
stages remain failures and dependent stages are marked skipped. A failed probe
makes the command return a nonzero exit status while retaining the JSON receipt.
Dependency probes run in child processes so Cargo directives printed by a native
discovery library cannot corrupt the JSON document.

Receipts replace the home, workspace, selected installation and temporary probe
prefixes with labels such as `$HOME`, `$WORKSPACE`, `$QUEST_PREFIX`, and
`$PROBE_DIR`. These labels describe the captured environment; they are not shell
commands to execute. Compiler versions and system SDK paths remain visible.
The receipt states each check's scope: discovery and header parsing do not prove
a downstream executable's runtime deployment or an MPI collective run.

MPI discovery follows the locked `mpi-sys` dependency, including explicit
`MPI_PKG_CONFIG`, the Cray MPI package, a compiler wrapper and pkg-config fallback.
Ordinary MPI-enabled builds require **libclang** because `mpi-sys` generates C
bindings with bindgen. This requirement applies even when the checked-in QuEST
bindings are unchanged. QuEST binding regeneration additionally requires a Clang
driver from the same LLVM major version as libclang. Set `LIBCLANG_PATH` and
`CLANG` consistently when the module environment does not select a matching pair.

Serial HDF5 must come from the same installation selected by `hdf5-metno-sys`.
Use the compiler-compatible serial HDF5 module and its `HDF5_DIR` when a system
pkg-config entry points to another build. Parallel HDF5 is rejected for QSVT IO.
On Cirrus, both compiler lanes load the central `cray-hdf5` module. The
[campaign scripts](verification/fixtures/cirrus/README.md) preserve its selected
prefix and use source-specific Cargo targets. The Cray profile disables Rust's
bundled LLD selection so the compiler wrapper can use its compatible linker.
It uses Cargo's documented nightly host configuration for build scripts and
procedural macros as well as target settings, because explicit `--target`
invocations do not inherit global target flags into host builds. The selected
settings and caller flag channels are retained in build/runtime receipts; see
the campaign's independent host-policy probe and full-suite evidence.
Parser compatibility uses Clang's semantic version macro: a vendor banner can
change under another compiler module without changing the parser's LLVM version.

## Regenerate or check bindings

```sh
cargo run --locked -p xtask -- generate-quest-bindings --check
cargo run --locked -p xtask -- generate-quest-bindings
```

Discovery uses a fresh temporary directory under Cargo's metadata-reported
target directory. Cargo configuration and `CARGO_TARGET_DIR` therefore apply,
and concurrent invocations do not share a CMake cache. The temporary discovery
directory is removed after the invocation.

The parser receives the evaluated include paths, definitions, target, sysroot,
and supported language/preprocessor/ABI flags. Native optimization,
instrumentation and warning options are not forwarded wholesale to libclang.
Header parse errors and mismatched Clang/libclang versions are explicit failures.

## Check downstream consumers

```sh
cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp
cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp --loader-isolated
```

Both commands preserve the compiler module environment while generating the
consumer lockfile and compiling. The default command also retains that environment
when running the numerical consumers. This permits compilers and installed SDKs
which depend on module-provided `LD_*` or `DYLD_*` variables.

`--loader-isolated` separately checks runtime deployment with loader overrides
removed. It additionally inspects ELF RUNPATH and the native dependency closure
on Linux, or Mach-O install names, LC_RPATH and loaded libraries on macOS. An
installation can pass the module-environment check while failing loader isolation;
report the two results separately. The compiler still runs with its module
loader environment in both modes.

Use `--work-dir /path/to/empty-evidence-directory` to keep the generated standalone
consumer workspace and logs at a chosen location. A nonempty directory is rejected
before writing fixtures. Otherwise a fresh evidence directory is retained and its
path is printed. These raw local logs may contain installation paths; the generic
path substitution applies to the doctor receipt.

For native QuEST installations tied to fixed external SDK locations, QuEST
documents the standard CMake option `CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON`.
The [Linux native Clang record](verification/2026-10-06-native-clang.md)
preserves a failed loader-isolated default installation and successful checks
of this explicitly configured second installation. This does not make the
installation relocatable across different SDK locations. Existing Cirrus
installations already enable this option; their separate vendor dependency
limitation remains recorded.

## MPI tests

The [shared test supervisor](../crates/quest-test-support/README.md) supports local
launchers and owned Slurm steps with bounded output, deadlines and explicit rank
checks. Run one coordinator outside MPI. On Cirrus, use exclusive nodes, one
rank per node and OpenMP threads within each node. The capacity executable opts
into native multithreading explicitly; its Rust sparse routing and preprocessing
remain serial within a rank. Native threading flags do not establish a speedup.

The [independent MPI consumer](verification/fixtures/native-mpi-consumer/README.md)
is a separate Cargo workspace exercising the public MPI feature, native
deployment and runtime ownership at 1/2/4/8 local ranks. Its main and build script
forbid unsafe Rust. Default-feature installed consumers and this MPI consumer
cover different dependency paths.

The [Torc standalone build workflows](verification/2026-10-06-torc-cirrus.md)
passed on Cirrus with GNU and Cray. They replace Python orchestration for that
one-node build stage; numerical checks and MPI ownership remain in Rust.
Subsequent two-node smoke tests passed through direct documented Slurm execution.
The later [server-backed scaling workflow](verification/fixtures/cirrus/torc-server.md)
used the supplied Torc 0.41.0 server and a manual scheduler configuration. It
launched one coordinator in an eight-node allocation; the unchanged scientific
payload owned its ordinary `srun` steps. All six Cray 2/4/8-node scaling cases
passed. Torc's automatic resource generation does not match this Cirrus profile,
and database durability on the supplied NFS storage remains unvalidated. The
GNU scaling and full-suite allocations used direct Slurm submission; these
results do not claim complete migration of the campaign to Torc.
