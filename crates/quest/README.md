# `QuEST` for Rust

A Rust workspace for `QuEST` 4.3: environment-bound simulation resources, a pure
circuit DAG and compiler, and an OpenQASM-style circuit macro. The facade package
is **`quest-rs`**, with Rust library name **`quest`**.

| Crate | Purpose |
| --- | --- |
| `crates/quest` | Typed runtime, faer snapshots, native preparation and execution |
| `crates/quest-sys` | Audited CXX bridge and native RAII resources |
| `crates/quest-circuit` | Pure circuit construction, exact angles, dependency DAG, compiler stages and transformations |
| `crates/quest-macros` | Rust token-tree frontend, without native dependencies |
| `crates/quest-build` | Installed `CMake` target discovery, bridge compilation and final-executable linking |
| `crates/xtask` | Binding generation and independent native consumer checks |

## Build

The workspace pins `nightly-2026-09-06` with rustfmt and Clippy. Install `QuEST`
**4.3.x**, binary64 precision, deprecated APIs disabled, plus `CMake` and a C++20
compiler. The tested native build recipe currently supports Linux GNU targets.
Pure circuit and macro builds need neither `QuEST` nor libclang nor external BLAS.

```sh
export QUEST_ROOT=/path/to/installed/quest
cargo build --workspace --locked
cargo run --locked --example minimal
```

The installed package must export `QuEST::QuEST` and resolve its own runtime
dependencies. `CMake` compiles the static CXX bridge against that target.
`QuEST_DIR` and `CMAKE_PREFIX_PATH` selection are also supported. Local native
installs with external CUDA/cuQuantum or MPI libraries can use `CMake`'s
`CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON` option, provided `QuEST`'s RPATH helper
honors it. Cargo does not repair or bundle native installations.

The Linux development helper emits absolute `DT_RUNPATH` entries for directly
linked libraries. Cross compilation, macOS, Windows and relocatable application
bundles need separate verified recipes. The old `QUEST_NATIVE_CONFIG` JSON record
and `QUEST_RUNTIME_LIBRARY_PATH` workflow are removed; unset those variables and
select the installed package normally.

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
both kinds of prepared program borrow it and cannot cross threads. Scope exit,
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
Distributed execution is not admitted by the facade's initial resource policy.
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
    let mut prepared = env.prepare_structured(bell)?;
    let mut state = env.state_vector(QubitCount::new(2)?)?;
    let result = prepared.run(&mut state, &RunInputs::default())?;
    println!("{:?}", result.outputs);
    Ok(())
}
```

The primary macro shares the documented `OpenQASM` 3.1 simulator profile with the
text frontend in `quest::qasm`. It supports typed classical control, user gates,
nonrecursive subroutines, signed controls, integer powers and once-evaluated
`${rust_expression}` captures. Pure consumers use `quest-circuit`; its default
`macros` feature reexports `circuit!`. Optional `codespan-reporting` and `serde`
render or serialize shared owned diagnostics. `legacy_circuit!` retains the
earlier ideal-angle static frontend.
See [the circuit profile](https://github.com/eessmann/quest-rs/blob/main/crates/quest-circuit/README.md) for supported syntax.

## Circuit semantics

`ProgramBuilder` allocates program-owned logical qubits and classical bits.
Operations preserve caller target order: target zero is the least-significant
local matrix bit. The private DAG orders shared quantum wires, classical hazards,
barriers and stochastic effects; public occurrence IDs are independent of graph
storage slots. Scheduling is deterministic and native execution is serial.

Compilation consumes owners:

```text
ProgramBuilder -> ValidatedProgram -> BoundProgram -> LoweredProgram
               -> ExecutablePlan -> PreparedProgram<'env>
```

Exact rational multiples of pi and declared parameters are distinct from finite
floating angles. `UnitaryCircuit` provides coherent control and adjoint only for
exact symbolic operations. Global phase is explicit; controlling a phase makes
it relative, and `Rz(2*pi)` remains `-I`.

Numerical operators own immutable faer matrices. Their semantics are `A|psi>` or
`A rho A†`; approximate unitarity evidence does not grant an exact inverse.
Products and residuals request `Par::Seq`. Numerical fusion may change rounding;
no cross-platform bitwise or certified approximation guarantee is implied.
Gate buffers are packed row-major; density state storage is column-major;
rectangular view adapters preserve logical values, including conjugation.

Preparation builds native caches transactionally. `run` can partially modify a
register on failure and reports its completed prefix. It checks ambient rounding,
underflow and native numerical policy before mutation. Sampling uses explicit
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
cargo clippy --workspace --all-targets --locked
cargo run --locked -p xtask -- generate-quest-bindings --check
cargo run --locked -p xtask -- check-native-consumers
cargo test --locked -p quest-circuit --no-default-features
```

Binding generation additionally requires libclang. Set `LIBCLANG_PATH` if needed;
the generator obtains a coherent compiler include search rather than mixing C++
standard libraries. Edit its templates and adapter registry, then regenerate the
artifacts together. Native tests are serialized in Nextest; lifecycle regressions
use subprocesses where ordinary Cargo's in-process test harness needs isolation.

The [architecture](https://github.com/eessmann/quest-rs/blob/main/docs/superpowers/specs/2026-09-10-quest-rust-design.md),
[dated bridge audit](https://github.com/eessmann/quest-rs/blob/main/docs/superpowers/specs/2026-09-10-quest-bridge-audit.md), and
[compiler research](https://github.com/eessmann/quest-rs/blob/main/docs/superpowers/specs/2026-09-10-quest-circuit-research.md)
preserve the design and dated research evidence. Text import/export, structured
execution and optional certified synthesis/ZX workers are implemented; the guide
documents their supported profile and validation boundaries.
