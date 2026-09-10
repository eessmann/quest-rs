# QuEST for Rust

A Rust workspace for QuEST 4.3: environment-bound simulation resources, a pure
circuit DAG and compiler, and an OpenQASM-style circuit macro. The facade package
is **`quest-rs`**, with Rust library name **`quest`**.

| Crate | Purpose |
| --- | --- |
| `crates/quest` | Typed runtime, faer snapshots, native preparation and execution |
| `crates/quest-sys` | Audited CXX bridge and native RAII resources |
| `crates/quest-circuit` | Pure circuit construction, exact angles, dependency DAG, compiler stages and transformations |
| `crates/quest-macros` | Rust token-tree frontend, without native dependencies |
| `crates/quest-build` | Shared installed-native configuration and final-executable runtime paths |
| `crates/xtask` | Binding generation and native setup |

## Build

The workspace pins `nightly-2026-09-06` with rustfmt and Clippy. Install QuEST
**4.3.x**, binary64 precision, deprecated APIs disabled, plus CMake and a C++20
compiler. The tested native build recipe currently supports Linux GNU targets.
Pure circuit and macro builds need neither QuEST nor libclang nor external BLAS.

```sh
export QUEST_ROOT=/path/to/installed/quest
# If indirect GPU libraries need additional directories, configure them explicitly:
export QUEST_RUNTIME_LIBRARY_PATH=/path/to/cuda/targets/x86_64-linux/lib
cargo run --locked -p xtask -- configure-native target/quest-native.json
export QUEST_NATIVE_CONFIG="$PWD/target/quest-native.json"
cargo build --workspace --locked
cargo run --locked --example minimal
```

Omit `QUEST_RUNTIME_LIBRARY_PATH` when the native installation already resolves
its dependencies. The setup probe executes with loader variables removed and
records the native compiler, ABI, canonical library identities and dependency
closure. This identifies observed QuEST/CMake inputs and libraries, rather than
every transitive system header. Unrecorded C++ flag, include-search and tool
overrides are rejected; regenerate using a supported compiler configuration.
Regenerate the record when its inputs change. The record is specific
to the installation and belongs outside version control.

The Linux development recipe emits **absolute DT_RPATH**, including indirect
native dependency directories. This is not a relocatable bundle. The build does
not modify the installed native libraries. Cross compilation, macOS, Windows and
relocatable packaging need separate verified loader recipes.

Cargo does not propagate a library build script's linker arguments into an
arbitrarily distant executable. Every final executable that uses this runtime,
including through a wrapper library, must also have:

```toml
[build-dependencies]
quest-build = "0.1"
```

```rust,ignore
// build.rs -- use the same QUEST_NATIVE_CONFIG as the native bridge
fn main() -> Result<(), quest_build::BuildError> {
    quest_build::emit_final_target_runtime_paths()
}
```

## Runtime

```rust,no_run
use quest::{Environment, QubitCount};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = Environment::builder().build()?;
    let mut register = env.state_vector(QubitCount::new(2)?)?;
    register.h(0)?;
    register.cx(0, 1)?;
    let snapshot = register.snapshot()?; // owned faer::Mat<Complex64>
    drop(register);
    env.close()?;
    println!("{:?}", snapshot); // independent of the environment lifetime
    Ok(())
}
```

An environment is unique per process and restricted to its creating thread.
Registers and prepared programs borrow it and cannot cross threads. Native calls
use the same bridge lifecycle and owner-thread admission even through direct
`quest-sys` use. Safe code cannot disable native validation. `close()` reports a
failed shutdown while retaining its owner; `Drop` is non-panicking. Leaking a
native owner still prevents finalization.

The default environment explicitly selects CPU execution without native
multithreading. GPU and native multithreading are explicit builder choices.
Distributed execution is not admitted by the facade's initial resource policy.
`MemoryBudget` bounds admission using conservative host, device and scratch
estimates; allocator or native failures remain possible. Exported snapshots leave
that accounting when returned and belong to the caller.

## OpenQASM-style Rust macro

```rust,no_run
use quest::{Environment, Shots, circuit};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bell = circuit! {
        qubit[2] q;
        bit[2] c;
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    }?;
    let env = Environment::builder().build()?;
    let mut prepared = env.prepare(bell)?;
    let samples = prepared.sample_zeroed(Shots::new(1024)?, &[2026, 9, 10])?;
    println!("{:?}", samples.counts);
    drop(prepared);
    env.close()?;
    Ok(())
}
```

Gate semantics follow the documented OpenQASM 3.1.0 subset. The macro parses Rust
tokens, with explicit array indices, `pi` angles, `${rust_expression}` interpolation,
`ctrl`/`negctrl`/`inv @` modifiers, measurement, reset and barriers. Interpolations
execute once in construction order. This is a Rust DSL, not an OpenQASM text
parser. Pure consumers use `quest-circuit`; its default `macros` feature reexports
`circuit!`, and optional `codespan-reporting` renders owned source diagnostics.
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
1–16 seeds for QuEST's process-wide RNG and initializes a fresh zero state on every
shot. The first batch retains a 4096-byte native RNG allowance in the environment
budget; transient seed copies are charged separately. Direct `quest-sys` calls
are outside facade memory accounting. Channels require density registers; reset uses trajectories on statevectors
and a complete channel on density matrices.

## Validation and generation

```sh
cargo nextest run --workspace --locked
cargo test --doc --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
cargo run --locked -p xtask -- generate-quest-bindings --check
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
explain the invariants and later work. OpenQASM text import/export, structured
runtime loops and advanced synthesis, routing and ZX passes are later milestones.
