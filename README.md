# QuEST for Rust

Structured OpenQASM 3.1 simulator programs, exact ideal circuit graphs, typed
QSP/QSVT workflows, and native QuEST 4.3 execution. The facade package is **`quest-rs`**; its Rust library
name is **`quest`**.

Read the [guide](docs/book/src/index.md) for the
[interface matrix](docs/book/src/interfaces.md), language semantics, executable
Bell/teleportation/feedback tutorials, include/export workflows, verified SSA,
and optimization certificates. The runnable source is
[examples/tutorials.rs](crates/quest/examples/tutorials.rs).

| Crate | Purpose |
| --- | --- |
| `quest` (`quest-rs`) | Environment-bound registers, native preparation and execution |
| `quest-language` | Owned sources/diagnostics, gate semantics, typed language, SSA and interpreter |
| `quest-qasm` | Explicit include resolution and canonical structured text export |
| `quest-circuit` | Structured pipeline, ideal DAG, exact transformations and bounded fusion |
| `quest-macros` | Structured `circuit!` / `circuit_file!`, plus migration `legacy_circuit!` |
| `quest-math` | Exact algebra and independently checked synthesis certificates |
| `quest-optimizer-client` / `quest-optimizer-worker` | Optional bounded external engine boundary |
| `quest-sys` | Audited CXX bridge and native RAII resources |
| `quest-build` / `xtask` | Native configuration, runtime paths and binding generation |
| [`quest-numerics`](crates/quest-numerics/README.md) | Binary64 kernels, interval arithmetic, reusable workspaces and observers |
| [`quest-polynomial`](crates/quest-polynomial/README.md) | Typed polynomial bases, function expressions and approximation |
| [`quest-qsp`](crates/quest-qsp/README.md) | Canonical/generalized QSP, binary64 inverse NLFT and separate certification |
| [`quest-qsvt`](crates/quest-qsvt/README.md) | Native-independent encodings, projectors, transforms and analysis |
| [`quest-qsvt-io`](crates/quest-qsvt-io/README.md) | JSON and optional serial HDF5 interchange |
| [`quest-qsvt-cli`](crates/quest-qsvt-cli/README.md) | Synthesis, inverse catalogs, solve, embedded and overlap applications |

## Build

The workspace pins `nightly-2026-09-06` with rustfmt and Clippy. Install QuEST
**4.3.x**, binary64 precision, deprecated APIs disabled, plus CMake and a C++20
compiler. The tested native build recipe currently supports Linux GNU targets.
Pure circuit and macro builds need neither QuEST nor libclang nor external BLAS.
The full workspace also builds the QSVT application, whose default IO feature
requires a **serial HDF5** installation; a parallel HDF5 build is rejected.

```sh
export QUEST_ROOT=/path/to/installed/quest
export HDF5_DIR=/path/to/installed/serial-hdf5
cargo build --workspace --locked
cargo run --locked --example minimal
```

For just the facade and its default examples, use `cargo build -p quest-rs --locked`;
that package does not require HDF5.

The installed package must export `QuEST::QuEST` and resolve its own runtime
dependencies. CMake compiles the static CXX bridge against that target; the
evaluated target supplies the native link requirements. Standard `QuEST_DIR`
and `CMAKE_PREFIX_PATH` selection are also supported. See
[`quest-build`](crates/quest-build/README.md) for selection and compiler details.

For a local QuEST installation whose CUDA/cuQuantum or MPI libraries live
outside the system loader paths, configure **QuEST itself** with
`-DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON` before installing. The upstream RPATH
helper must honor that standard option. Default native packaging retains
relative `$ORIGIN` paths. Cargo builds do not repair or bundle native libraries.

The Linux development helper emits absolute **DT_RUNPATH** entries for directly
linked libraries. Cross compilation, macOS, Windows and relocatable application
bundles need separate verified recipes. The old `QUEST_NATIVE_CONFIG` JSON
record and `QUEST_RUNTIME_LIBRARY_PATH` workflow have been removed; unset those
variables and select the installed package normally.

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

`circuit!` constructs a `StructuredProgram`: typed classical expressions,
scoped declarations, nonrecursive gates and subroutines, arrays and references,
runtime branches/loops, measurement, reset and feedback. Rust `${ ... }`
captures evaluate once during construction. `circuit_file!` admits text from
compiler-tracked files. `quest-qasm` provides pure in-memory text import with an
explicit include resolver and owned source snapshots.

Use `Environment::prepare_structured`, then run against an existing register
with `RunInputs`. The tutorials execute the same functions tested by the
process-isolated integration test:

```sh
cargo run -p quest-rs --example tutorials --locked
cargo test -p quest-rs --test tutorials --locked
cargo test -p quest-circuit --test tutorials --locked
```

Structured compilation consumes checked stages:

```text
source / circuit! / typed Builder -> TypedModule -> verified SSA
    -> lower -> plan -> PreparedStructuredProgram<'env> -> bounded run
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

## Ideal circuits and optimization

`ProgramBuilder` constructs finite ideal circuits with program-owned qubit/bit
identities, explicit gate occurrences and a dependency DAG. It retains exact
rational multiples of π and symbolic parameters before binding. The old static
macro frontend is available as `legacy_circuit!`; it has different semantics
from primary `circuit!`.

```text
ProgramBuilder -> ValidatedProgram -> BoundProgram
    -> ExecutablePlan -> PreparedProgram<'env>
```

Explicit passes provide phase-correct local rewrites, bounded CNOT synthesis,
exact affine parity folding, and numerical fusion after binding. Optional
`workers` APIs call pinned synthesis/QuiZX engines through a bounded protocol;
parent-side checks verify candidates and retain certificates. Local rotation
error bounds, exact operator equality, and numerical fusion rounding changes
remain distinct claims. See [optimization](docs/book/src/optimization.md).

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
`certification` feature checks frozen values using Astro Float; the separate
`offline-synthesis` feature enables explicitly requested arbitrary-precision
synthesis. Production failures never invoke it automatically. Numerical
residuals, certified QSP error bounds and conditional QSVT theorem bounds are
different evidence; see [certification](docs/book/src/qsp-certification.md).
No GMP or MPFR dependency is required.

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
caller-owned Rayon pools and optional rsmpi execution. The default application
enables native execution, HDF5 and certification; pure library builds remain
independent of those application dependencies. MPI requires both the Cargo
feature and a compatible MPI/SUBCOMM-enabled QuEST installation.

Generate the API references locally with:

```sh
cargo doc -p quest-qsp -p quest-qsvt --all-features --no-deps --locked
```

The [verification record](docs/verification/2026-09-11-qsvt-port.md) separates
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
are explicit choices. `MemoryBudget` bounds admission with conservative host,
device and scratch estimates. Returned snapshots belong to the caller and leave
facade accounting. Direct `quest-sys` calls are outside facade memory accounting.

Preparation builds caches transactionally. Execution can partially modify a
register before failure and reports completed work; it checks numerical policy
before mutation. Channels require density registers. Reset uses trajectories on
state vectors and a complete channel on density matrices. Ideal sampling APIs
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
The [approved compiler plan](docs/superpowers/plans/2026-09-10-openqasm-ssa-implementation.md)
tracks current implementation and evidence. The
[dated bridge audit](docs/superpowers/specs/2026-09-10-quest-bridge-audit.md) and
[compiler research](docs/superpowers/specs/2026-09-10-quest-circuit-research.md)
preserve historical findings; they are not current feature checklists.

Local compiler evidence is preserved in [the M6–M11 verification record](docs/verification/2026-09-10-m6-m11.md).
The updated native build and loader checks are recorded in
[native CMake verification](docs/verification/2026-09-10-native-cmake.md).
