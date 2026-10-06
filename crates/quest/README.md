# `QuEST` for Rust

A Rust workspace for `QuEST` 4.3: environment-bound simulation resources, a pure
circuit DAG and compiler, and an OpenQASM-style circuit macro. The facade package
is **`quest-rs`**, with Rust library name **`quest`**.

| Crate | Purpose |
| --- | --- |
| `crates/quest` | Typed runtime, faer snapshots, native preparation and execution |
| `crates/quest-sys` | Audited CXX bridge and native RAII resources |
| `crates/quest-language` | Checked semantics, exact angles, effects, SSA and immutable quantum payloads |
| `crates/quest-compile` | Optimization, specialization, portable lowering and compiled artifacts |
| `crates/quest-compile` | Public program API over the semantic core and compiler |
| `crates/quest-synthesis` | Bounded Rust Clifford+T candidate generation; independent certificates belong to `quest-math` |
| `crates/quest-macros` | Rust token-tree frontend, without native dependencies |
| `crates/quest-build` | Installed `CMake` target discovery, bridge compilation and final-executable linking |
| `crates/xtask` | Binding generation and independent native consumer checks |

## Build

The workspace uses rolling `nightly` with rustfmt and Clippy. Install `QuEST`
**4.3.x**, binary64 precision, deprecated APIs disabled, plus `CMake` and a C++20
compiler. Native recipes target Linux GNU and `aarch64/x86_64` Darwin; each
architecture needs its own build and runtime validation.
Pure circuit and macro builds need neither `QuEST` nor libclang nor external BLAS.

```sh
export QUEST_ROOT=/path/to/installed/quest
cargo build -p quest-rs --locked
cargo run --locked --example minimal
```

Building the entire workspace also selects the QSVT application and its serial
HDF5 support. Select serial HDF5 through pkg-config or `HDF5_DIR`, then use
`cargo build --workspace --locked`. The facade itself does not require HDF5.

The installed package must export `QuEST::QuEST` and resolve its own runtime
dependencies. `CMake` compiles the static CXX bridge against that target.
`QUEST_ROOT` selects the exact installation prefix; `CMAKE_PREFIX_PATH` supports conventional package discovery. Local native
installs with external CUDA/cuQuantum or MPI libraries can use `CMake`'s
`CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON` option, provided `QuEST`'s RPATH helper
honors it. Cargo does not repair or bundle native installations.

The development helper emits absolute runtime paths for directly linked libraries:
`DT_RUNPATH` on Linux and `LC_RPATH` on Darwin. Cross compilation, Windows and
relocatable application bundles remain unsupported. The old `QUEST_NATIVE_CONFIG` JSON record
and `QUEST_RUNTIME_LIBRARY_PATH` workflow are removed; unset those variables and
select the installed package normally.

See the [workspace setup](../../README.md#build) for the Nix development shell or
the [Grace Hopper guide](../../docs/grace-hopper.md) for manual/Spack dependencies,
including the matching Rust linker and GPU checks.

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

## Runtime

```rust,no_run
use quest::{Environment, QubitCount};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = {
        let env = Environment::builder().build()?;
        let mut register = env.state_vector(QubitCount::new(2)?)?;
        register.h(0)?;
        register.cx(0, 1)?;
        register.snapshot()? // owned faer::Mat<Complex64>
    }; // register is destroyed, then env finalizes the native runtime
    println!("{:?}", snapshot); // independent of the environment lifetime
    Ok(())
}
```

`Environment` uniquely owns a runtime whose native initialization may be entered
only once per process. It is restricted to its creating thread. Registers and
prepared programs borrow it and cannot cross threads. Scope exit,
including an early `?` return, destroys borrowing resources before `Environment`
automatically finalizes the native runtime. There is no high-level explicit
shutdown method and no restart: `QuEST` may own an MPI world that cannot be
initialized again after finalization. Configuration validation before native
initialization does not consume the single attempt.

`Drop` never panics. If safe finalization is prevented or native cleanup fails,
`QuEST` is permanently retired while the process continues. All later native
operations and initialization attempts are rejected, and storage that cannot
safely be destroyed is retained until process exit. Direct `quest-sys` calls
retain the same lifecycle and owner-thread admission checks. Abort, forced
termination, or deliberately forgetting the environment can prevent RAII cleanup.

Native matrix and channel caches inside prepared programs belong to the runtime.
Independent Rust `NumericalOperator` payloads and owned faer snapshots do not
borrow the environment and remain usable after its scope ends.

The default environment explicitly selects CPU execution without native
multithreading. GPU and native multithreading are explicit builder choices.
This local `Environment` does not admit distributed execution. The optional
`quest::collective` API owns distributed resources through a caller-owned rsmpi
runtime and borrowed communicators; see the MPI section below.
`MemoryBudget` bounds admission using conservative host, device and scratch
estimates; allocator or native failures remain possible. Exported snapshots leave
that accounting when returned and belong to the caller.

## OpenQASM-style Rust macro

```rust,no_run
use quest::{Environment, QubitCount, RunInputs, circuit};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bell = circuit! {
        qubit[2] q;
        output bit[2] c;
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    }?;
    let env = Environment::builder().build()?;
    let mut prepared = env.prepare(bell.verify()?.lower()?.plan()?)?;
    let mut state = env.state_vector(QubitCount::new(2)?)?;
    let result = prepared.run(&mut state, &RunInputs::default())?;
    println!("{:?}", result.outputs);
    Ok(())
}
```

The primary macro shares the documented `OpenQASM` 3.1 simulator profile with the
text frontend in `quest::qasm`. It supports typed classical control, user gates,
nonrecursive subroutines, signed controls, integer powers and once-evaluated
`${rust_expression}` captures. Pure consumers use `quest-compile`; its default
`macros` feature reexports `circuit!`. Optional `codespan-reporting` and `serde`
render or serialize shared owned diagnostics. Macro expansion emits a checked reusable
template; captures run once in source order. Typed `${Angle::pi(1, 4)?}` captures
retain exact angles alongside ordinary floating captures.
See [the circuit profile](https://github.com/eessmann/quest-rs/blob/main/crates/quest-compile/README.md) for supported syntax.

## Circuit semantics

Builder, macro and text frontends enter one checked representation. The typed
`ProgramBuilder` supports inputs/outputs, arrays and references, definitions,
calls, signed controls, adjoints, powers, measurement/reset and immutable
matrix/oracle insertion. `QuantumRegion` is a finite compiler capability; import
it with `Program::from_region` to use the same public execution lifecycle.

```text
construct -> Program<Constructed> -> verify -> Program<Verified>
          -> specialize / compile -> lower -> Program<Executable>
          -> Environment::prepare -> PreparedProgram -> run -> RunOutput
```

Native arbitrary-angle simulation is the default. Discrete synthesis is an
explicit cold operation. Exact rational and affine angles, ordinary floats and
runtime expressions remain distinct; no float is inferred to be exact π.
Binding obligations survive cancellation. Checked exact-unitary capabilities
permit adjoints and controls. Global phase is preserved: `Rz(2*pi)` is `-I`.
Numerically admitted matrices never gain exact-unitary privileges.

Numerical operators own immutable faer matrices. Their semantics are `A|psi>` or
`A rho A†`; approximate unitarity evidence does not grant an exact inverse.
Products and residuals request `Par::Seq`. Numerical fusion may change rounding;
no cross-platform bitwise or certified approximation guarantee is implied.
Gate buffers are packed row-major; density state storage is column-major;
rectangular view adapters preserve logical values, including conjugation.

Preparation resolves static gate operands and parameters once and builds native
caches transactionally. `run` can partially modify a
register on failure and reports its completed prefix. It checks ambient rounding,
underflow and native numerical policy before mutation. `sample_zeroed(shots, seeds, inputs)` returns one ordinary `RunOutput` per shot,
including named outputs and execution counts. Sampling uses explicit
1–16 seeds for `QuEST`'s process-wide RNG and initializes a fresh zero state on every
shot. The first batch retains a 4096-byte native RNG allowance in the environment
budget; transient seed copies are charged separately. Direct `quest-sys` calls
are outside facade memory accounting. Channels require density registers; reset uses trajectories on statevectors
and a complete channel on density matrices.

Every facade `Error` produces an owned shared diagnostic through `Error::report()`.
Structured runtime reports retain the typed program/block/instruction occurrence,
interpreter call frames, completed quantum prefix, and immutable source snapshots
after the prepared plan and environment are dropped. Inline macro code without a
text snapshot uses its compiler file, line, and column as a location note. Enable
`codespan-reporting` to render validated text labels and `serde` to serialize the
same diagnostic data.

## Validation and generation

```sh
cargo nextest run --workspace --locked
cargo test --doc --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p xtask -- generate-quest-bindings --check
cargo run --locked -p xtask -- check-native-consumers
cargo test --locked -p quest-compile --no-default-features
```

Binding generation additionally requires libclang. Set `LIBCLANG_PATH` if needed;
the generator obtains a coherent compiler include search rather than mixing C++
standard libraries. Edit its templates and adapter registry, then regenerate the
artifacts together. Native tests are serialized in Nextest; lifecycle regressions
use subprocesses where ordinary Cargo's in-process test harness needs isolation.

The [guide](../../docs/book/src/index.md) documents text import/export, structured
execution, optional certified synthesis/ZX workers, and their validation
boundaries. The [contributor guide](../../CONTRIBUTING.md) covers development;
the [verification index](../../docs/verification/README.md) records tested
configurations and results.

Shared `OracleFragment` calls remain retained in the common program. Preparation builds each shared body once and caches numerical matrices by
payload identity and the signed control profiles needed by reachable calls.
Ordinary payloads and reachable oracle bodies share one native pool and one
admission inventory per prepared owner. Forward and adjoint native matrices
belong to one cached variant. Targets and
orientation are applied during execution using preallocated remapping buffers;
repeated calls do not flatten or rebuild the body. The prepared owner exposes
`prepared_oracle_bodies()` and `prepared_oracle_matrix_variants()` for inspecting
oracle resources, and `prepared_matrix_variants()` for the total shared native
pool. Independently prepared owners retain their own pools and environment
reservations.

Numerical controls use the existing general linear-operator path. Preparation
embeds each required control profile once, preserving negative controls and the
phase of controlled global phases without granting approximate matrices exact
inverse privileges. An additional control doubles the matrix dimension (and
quadruples dense storage); the memory forecast includes all distinct variants
and rejects excessive widths before native allocation. Structured target indices
may vary at execution; signed control counts and states come from the admitted
SSA call graph. Uncalled captures require no native cache.

These caches preserve the existing environment borrow, thread confinement,
numerical configuration snapshot, and transactional native preparation. Coherent
oracle execution supports both state vectors and density matrices. The retained
oracle path reuses its Rust matrix and operand buffers; the general structured
interpreter and existing C++ bridge keep their own documented allocation behavior.

## Collective MPI ownership and execution

The `collective` module is compiled only with Cargo feature `mpi` and an installed
`QuEST::QuEST` target configured with both MPI and SUBCOMM. Default facade and
pure compiler builds do not depend on rsmpi or its bindgen/libclang build chain.
A Cargo feature selects the optional dependency; checked native configuration
controls whether the API exists. No placeholder MPI API is emitted for other
native configurations.

Use `quest::collective::MpiRuntime::initialize()` to own rsmpi's `Universe` with
mandatory `MPI_THREAD_MULTIPLE`, borrow a world communicator (or split a subgroup),
then call `CollectiveEnvironment::builder(&communicator)?.build()`. The builder
supports named GPU/multithreading opt-ins and a per-rank memory budget. Set
`MPICC` to the absolute wrapper for the same installed MPI implementation; the
build compares actual loaded library identity, ABI layouts and version against
the selected `QuEST::QuEST` target before accepting that pairing. The development
MPICH installation additionally needs `MPICH_CC=/usr/bin/gcc` because its saved
`gcc-13` executable is absent.

Every rank in one subgroup calls collective operations in matching order.
`state_vector`, `prepare`, and `CollectivePreparedProgram::run` share the
local facade's resource accounting and execution implementation. Preparation
compares complete canonical semantic bytes, including all complex matrix entries,
ordered targets, signed controls, phases, nested oracle bodies and adjoints.
Recoverable validation and budget failures are agreed before native work;
a native failure after entry aborts the job when ordered cleanup is uncertain.
Different subgroups can execute independent coherent schedules. Measurement,
noise, reset, runtime inputs, dynamic control and distributed solve are rejected
by this coherent collective interface. It accepts `Program<Executable>` and
returns `RunOutput`.

`CollectiveRegister::init_pure_from_root` broadcasts admitted input storage from
one subgroup root. `probability`, `total_probability`, and the unnormalized
`project` operation support scalar postselection accounting without a full-state
gather. Native registers/prepared programs borrow their environment, which borrows
the communicator, which borrows the MPI universe. Drop resources first, then the
`QuEST` owner; application MPI remains usable until its universe is dropped.
All owners stay on the initializing thread. Only borrowed `MpiThreadView` message
views cross scoped threads, with rsmpi typed buffers and pure status snapshots.
No owning rsmpi communicator can escape the universe lifetime through this API.

`Register::environment()` now returns a read-only `EnvironmentView` exposing
capabilities, memory budget and current allocation, shared by both owner kinds.
Call allocation/preparation methods on the actual owner rather than through a
register's accessor. There is no `Deref` conversion between collective and local
owners and no explicit shutdown operation.

Collective preparation also checks canonical cache-sharing relationships: equal
values assembled with different sharing graphs can require different native
allocation schedules. First-occurrence indices describe those relationships;
raw memory addresses never cross ranks. Such a schedule mismatch is rejected
before materialization, even when the numerical operations are otherwise equal.

The Linux [sparse matching experiment](examples/sparse_capacity/README.md) creates
input shards directly and records construction, persistence, loading and repeated
CPU/MPI execution. Its explicit `--scaling` mode accepts observed unlimited
process limits while retaining finite managed budgets; those receipts never claim
whole-node capacity acceptance.
