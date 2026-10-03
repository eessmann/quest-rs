# QuEST for Rust

A unified simulator program API for OpenQASM 3.1, typed builders, exact angles
and reusable quantum regions, with typed QSP/QSVT workflows and native QuEST 4.3 execution. The facade package is **`quest-rs`**; its Rust library
name is **`quest`**.

Read the [guide](docs/book/src/index.md) for the
[interface matrix](docs/book/src/interfaces.md), language semantics, executable
Bell/teleportation/feedback tutorials, include/export workflows, verified SSA,
and optimization certificates. The runnable source is
[examples/tutorials.rs](crates/quest/examples/tutorials.rs).
The [documentation index](docs/README.md) links setup guides, crate references,
and verification results. See [Contributing](CONTRIBUTING.md) for development
commands and repository conventions.

| Crate | Purpose |
| --- | --- |
| `quest` (`quest-rs`) | Environment-bound registers, native preparation and execution |
| `quest-language` | Owned sources/diagnostics, gate semantics, typed language, SSA and interpreter |
| `quest-qasm` | Explicit include resolution and canonical structured text export |
| `quest-compile` | Canonical compiler API and macros, exact transformations, synthesis and typed artifacts |
| `quest-macros` | Compile-time checked `circuit!` / `circuit_file!` templates |
| `quest-math` | Exact algebra and independently checked synthesis certificates |
| `quest-synthesis` | Bounded deterministic Rust Clifford+T candidate generation |
| `quest-optimizer-client` / `quest-optimizer-worker` | Optional bounded external engine boundary |
| `quest-sys` | Audited CXX bridge and native RAII resources |
| `quest-build` / `xtask` | Native configuration, runtime paths and binding generation |
| [`quest-numerics`](crates/quest-numerics/README.md) | Static arithmetic backends, directed intervals, AD, root contractors and reusable workspaces |
| [`quest-polynomial`](crates/quest-polynomial/README.md) | Typed polynomial bases, function expressions and approximation |
| [`quest-qsp`](crates/quest-qsp/README.md) | Real-parity Wx and unit-circle QSP, RHW/Half-Cholesky and inverse NLFT, with frozen-export certification |
| [`quest-qsvt`](crates/quest-qsvt/README.md) | Native-independent encodings, projectors, transforms and analysis |
| [`quest-qsvt-io`](crates/quest-qsvt-io/README.md) | JSON, PennyLane catalogs, and serial HDF5 interchange |
| [`quest-qsvt-cli`](crates/quest-qsvt-cli/README.md) | Synthesis, inverse catalogs, solve, embedded and overlap applications |

## Build

The workspace uses rolling `nightly` with rustfmt, Clippy and Rust sources.
Record `rustc -Vv` with validation results; compile-fail diagnostics can change
when nightly advances. Native
recipes target Linux GNU and aarch64/x86_64 Darwin. Each architecture needs its
own build and runtime validation; Nix evaluation alone is not that evidence.
The bridge requires CMake 3.28+, a C++20 compiler, QuEST 4.3.x with binary64
precision and deprecated APIs disabled. Pure circuit and macro builds need
neither QuEST nor libclang nor external BLAS. The full workspace also builds
the QSVT application and requires **serial HDF5**; parallel HDF5 is rejected.
On Linux, `--all-features` additionally requires an MPI/SUBCOMM-enabled QuEST
installation and an absolute `MPICC` path from the same MPI installation.

Build directly with installed packages. For a Linux installation using
GCC 16, CUDA 13.4 and system MPICH, select QuEST in Fish:

```fish
set -gxa --path CMAKE_PREFIX_PATH /path/to/installed/quest/lib64/cmake/QuEST
set -gx MPICC /usr/lib64/mpich/bin/mpicc
fish_add_path --prepend /usr/lib64/mpich/bin
cargo build --workspace --all-features --locked
cargo run --locked --example minimal
```

Replace `/path/to/...` placeholders with your installation locations.

System serial HDF5 is detected from standard Linux installation paths, including
Fedora's `H5pubconf-64.h` header layout. To select Homebrew's serial HDF5 when it
is installed, also append its pkg-config path before building:

```fish
set -gxa --path PKG_CONFIG_PATH /path/to/installed/hdf5/lib/pkgconfig/
```

These settings append to the existing package search paths, including system
MPICH. Use the wrapper and launcher directory matching your QuEST installation;
the MPICH paths above are Fedora's system installation. Leave `HDF5_DIR` unset
to use system serial HDF5. The default system `cc` and `c++` use GCC 16; no
compiler override is
needed. The installed QuEST library resolves its dependencies through its own
runtime paths, so `LD_LIBRARY_PATH` is unnecessary. Run the workspace validation
commands below in this native environment.

For other installations, select package roots explicitly:

```sh
export QUEST_ROOT=/path/to/installed/quest
export HDF5_DIR=/path/to/installed/serial-hdf5
export MPICC=/path/to/matching-mpich/bin/mpicc
export PATH=/path/to/matching-mpich/bin:$PATH
cargo build --workspace --all-features --locked
cargo run --locked --example minimal
```

The optional standalone [devenv](devenv.nix) provisions the configured Rust toolchain,
compiler, CMake, libclang, serial HDF5 and an installed shared CPU/OpenMP QuEST
package. Linux enables MPI and SUBCOMM using the same MPICH package for QuEST,
rsmpi and `mpiexec`; GPU backends are disabled. Darwin retains its CPU/OpenMP
configuration without MPI. Its inputs are pinned in `devenv.lock`, including
`eessmann/QuEST`'s `cmake-packaging` source at commit `5035520`. From the
repository root:

```sh
devenv --clean shell         # enter without inherited native package overrides
devenv build outputs.quest   # build the installed QuEST package
devenv --clean test          # all-feature workspace checks and the minimal example
```

The shell selects the built package with `QUEST_ROOT` and serial HDF5 with
`HDF5_DIR`. On Linux it also sets `MPICC` to MPICH's development output wrapper
and puts the matching launcher output first in `PATH`. The QuEST derivation
checks independent shared-library loading and an installed CMake consumer with
CPU-only initialization and loader overrides cleared. On Linux, `devenv test`
runs the all-feature build, workspace nextest,
doctests, ignored release QSP tests and minimal native example. The full Linux
all-feature recipe needs local MPI networking for its two- and four-rank tests;
Darwin retains its minimal-example test and still needs separate feature and
native validation.
Use `devenv update quest-src` to intentionally refresh the QuEST
source lock. If native shell integration is already configured, `devenv allow`
can activate it in this directory; no `.envrc` is required.

For ARM64 Grace Hopper nodes using manual/Spack dependencies, follow the
[no-Nix setup and GPU validation recipe](docs/grace-hopper.md). Select the
matching Rust linker when Spack's GCC differs from the system `cc`. Serial HDF5
can also be selected through its `hdf5` pkg-config entry without `HDF5_DIR`.

For just the facade and its default examples, use `cargo build -p quest-rs --locked`;
that package does not require HDF5. The installed package must export
`QuEST::QuEST` and resolve its own runtime dependencies. CMake compiles the
static CXX bridge against that target; the evaluated target supplies native
link requirements. `QUEST_ROOT` selects the exact installation prefix, and `CMAKE_PREFIX_PATH`
retains conventional CMake package search. See [`quest-build`](crates/quest-build/README.md) for package selection.

For a local QuEST installation whose external native libraries live outside
system loader paths, configure **QuEST itself** with
`-DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON` before installing. The package must
resolve its indirect dependencies: Linux uses ELF `$ORIGIN`/`DT_RUNPATH`, while
Darwin uses Mach-O install names and `@loader_path`/`LC_RPATH`. The final-target
helper emits search paths for direct dependencies. Cargo does not repair or
bundle native installations. Unset the removed `QUEST_NATIVE_CONFIG` and
`QUEST_RUNTIME_LIBRARY_PATH` variables and select the installed package normally.

Cargo does not propagate a library build script's linker arguments into an
arbitrarily distant executable. Every final executable that uses this runtime,
including through a wrapper library, must also have:

```toml
[build-dependencies]
quest-build = "0.1"
```

```rust,ignore
// build.rs -- select the same installed package as the native bridge
fn main() -> Result<(), quest_build::BuildError> {
    quest_build::emit_final_target_runtime_paths()
}
```

## Structured programs

`circuit!` constructs a `Program<Constructed>`: typed classical expressions,
scoped declarations, nonrecursive gates and subroutines, arrays and references,
runtime branches/loops, measurement, reset and feedback. Rust `${ ... }`
captures evaluate once during construction. `circuit_file!` admits text from
compiler-tracked files. `quest-qasm` provides pure in-memory text import with an
explicit include resolver and owned source snapshots.

Consume `.verify()?.lower()?.plan()?`, pass the `Program<Executable>` to `Environment::prepare`, then run against an existing register
with `RunInputs`. The tutorials execute the same functions tested by the
process-isolated integration test:

```sh
cargo run -p quest-rs --example tutorials --locked
cargo test -p quest-rs --test tutorials --locked
cargo test -p quest-compile --test tutorials --locked
```

Structured compilation consumes checked stages:

```text
source / circuit! / ProgramBuilder -> Program<Constructed>
    -> verify -> Program<Verified> -> lower -> Program<Lowered>
    -> plan -> Program<Executable> -> Environment::prepare -> PreparedProgram<'env>
```

Scalar joins and loop-carried values use SSA block arguments. Memory tokens and
alias sets preserve effects. An independent verifier checks ownership, dominance,
types, interfaces, predecessor sealing and resource bounds. Classical and static-window quantum SSA
optimization are explicit and reverified; frozen structured syntax remains the
authority for canonical export. Quantum windows admit guarded inverse
cancellation and CNOT/Clifford+T parity resynthesis without crossing effects or
control-flow edges.

This is a bounded OpenQASM 3.1 simulator profile, not pulse/timed hardware
execution. Numeric widths are 1–64 bits and floats are binary32/binary64. `pi` is
a floating constant; integer `1/2` is zero. Stored `angle` values wrap modulo a
turn. The U gate follows OpenQASM 3.1's scalar phase convention. See
[the language chapter](docs/book/src/language.md) before migrating old programs.

## Quantum regions and optimization

`ProgramBuilder` constructs the common typed program, including classical control and finite quantum regions. `QuantumRegionBuilder` constructs a finite capability with owned wire identities, exact rational angles, symbolic parameters, occurrences and provenance. Bind its original parameter obligations, apply compiler extension traits from `quest_compile::prelude::*`, then embed the result with `ProgramBuilder::region` or `Program::from_bound_region`.

Explicit passes provide phase-correct local rewrites, bounded CNOT synthesis, exact affine parity folding, and numerical fusion after binding. `NativeSynthesis` is the default in-process Rust rotation generator for explicit `synthesize_rotations` calls. Optional process clients and QuiZX integration retain the same independent candidate checks. Construction and macro expansion never run synthesis.

`export_source` preserves the immutable original textual program. `export_compiled` persists optimized SSA, captures, payloads and evidence; `load_compiled` validates that artifact without replacing its executable with re-lowered source. Local rotation error bounds, exact operator equality, and numerical fusion rounding changes remain distinct claims. See [optimization](docs/book/src/optimization.md).

Target order is semantic: target zero is the least significant local matrix bit.
Global phase is explicit, so controlled `Rz(2*pi)` cannot be erased as identity.
Numerical operators own immutable faer matrices with `A|psi>` / `A rho A†`
semantics. Approximate unitarity evidence grants no exact inverse. Products use
sequential faer evaluation; fusion may change rounding.

## QSP and QSVT

Start with [typed polynomial preparation](docs/book/src/numerical-polynomials.md),
[QSP synthesis](docs/book/src/qsp-synthesis.md), then
[QSVT encodings and transform builders](docs/book/src/qsvt-model.md).
Consuming builders distinguish admitted targets, completed polynomials, frozen
controls, validated transforms and environment-bound prepared execution.

```text
polynomial -> admission -> completion -> frozen QSP candidate
                                      -> optional independent certification
encoding + typed phases/controls -> QSVT transform -> admission -> preparation
                                                   -> execution -> release/condition
```

Production synthesis and native execution use binary64. The optional
`certification` feature checks frozen values using Dashu; the separate
`offline-synthesis` feature enables explicitly requested arbitrary-precision
synthesis. Production failures never invoke it automatically. Numerical
residuals, certified QSP error bounds and conditional QSVT theorem bounds are
different evidence; see [certification](docs/book/src/qsp-certification.md).
Project-owned arbitrary-precision integers, rationals and binary floating-point
values use pinned pure-Rust Dashu components. Native QuEST remains separate
from this arithmetic.

```sh
# Pure numerical tutorials: no native QuEST installation required.
cargo run -p quest-qsp --example qsp_tutorials --locked
cargo test -p quest-qsp -p quest-qsvt --doc --locked
# Optional cold verification and explicit offline tutorials.
cargo run -p quest-qsp --example qsp_tutorials --features offline-synthesis --locked
# Native execution, postselection and complex Hadamard observations.
cargo run -p quest-rs --example qsvt --features qsvt --locked
```

The [native tutorial](docs/book/src/qsvt-runtime.md) explains scoped preparation
and consuming postselection. The [application guide](docs/book/src/qsvt-applications.md)
covers serial HDF5 setup, all 21 inverse catalogs, physical solve scaling,
caller-owned Rayon pools and optional rsmpi execution. Binary64, offline and
catalogue synthesis default to inverse NLFT, with RHW Half-Cholesky available
through an explicit algorithm selection. The default application
enables native execution, HDF5 and certification; pure library builds remain
independent of those application dependencies. MPI requires both the Cargo
feature and a compatible MPI/SUBCOMM-enabled QuEST installation.

Generate the API references locally with:

```sh
cargo doc -p quest-qsp -p quest-qsvt --all-features --no-deps --locked
```

The [QSP/QSVT verification record](docs/verification/2026-09-11-qsvt-port.md) separates
mathematical certificates, numerical comparisons, measured costs and unverified
platforms. Generalized QSVT theorem-level robustness is not certified.

## Native ownership and effects

`Environment` is the unique owner of a runtime initialized at most once per
process and confined to its creating thread. Registers and prepared programs
borrow it. Scope exit destroys those resources and automatically finalizes the
environment; no explicit shutdown method is needed or provided. Finalization
permanently ends QuEST use in that process, including when QuEST owns MPI, whose
world model cannot restart after finalization.

`Drop` never panics. A failed cleanup permanently retires QuEST while allowing
the process to continue, retaining native storage that cannot safely be freed.
Subsequent native operations and initialization are rejected. Safe low-level
bridge use retains lifecycle and native validation checks too. Pure Rust matrix
payloads and owned snapshots remain independent of the environment lifetime.

CPU execution without native multithreading is the default. GPU and threading
are explicit choices: `ExecutionMode::Enabled` requires that mode for allocated
registers, `Disabled` prevents it, and `Auto` uses QuEST's size thresholds.
`MemoryBudget` bounds admission with conservative host,
device and scratch estimates. Returned snapshots belong to the caller and leave
facade accounting. Direct `quest-sys` calls are outside facade memory accounting.

Preparation builds caches transactionally. Execution can partially modify a
register before failure and reports completed work; it checks numerical policy
before mutation. Channels require density registers. Reset uses trajectories on
state vectors and a complete channel on density matrices. Program sampling APIs
use explicit seeds and fresh zero initialization for each shot. See
[runtime limits and ownership](docs/book/src/runtime.md).

## Validation and documentation

```sh
cargo nextest run --workspace --locked
cargo test --doc --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p xtask -- generate-quest-bindings --check
cargo run --locked -p xtask -- check-native-consumers
mdbook build docs/book
```

The book is validated with mdBook 0.5.4 and includes code directly from tested
Rust sources. Pure language/QASM/circuit tests do not need QuEST. Native lifecycle
tests use the serialized Nextest group or explicit process isolation. Optional
worker tutorials require a supplied worker executable path.

Binding generation also needs libclang (`LIBCLANG_PATH` when necessary). Edit
generator templates and the adapter registry, then regenerate artifacts together.
The [contributor guide](CONTRIBUTING.md) describes the development process, and
the [verification index](docs/verification/README.md) collects dated compiler,
numerical, and native-backend results with their tested configurations.
