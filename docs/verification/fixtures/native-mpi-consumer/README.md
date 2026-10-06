# Independent MPI consumer

This separate Cargo workspace exercises the public `quest-rs` MPI feature and
final-executable runtime-path helper against an installed, unmodified QuEST
package with MPI, SUBCOMM and OpenMP enabled. The Rust application and build
script forbid unsafe code. Existing FFI dependencies retain their own audited
implementation boundaries.

Select matching native and Rust MPI installations, then run the coordinator
once from the repository root:

```sh
export QUEST_ROOT=/path/to/quest-install
export MPICC=/path/to/mpi/bin/mpicc
export PATH=/path/to/mpi/bin:$PATH
export CARGO_TARGET_DIR=/path/to/isolated-cargo-target
cargo run --locked --offline \
    --manifest-path docs/verification/fixtures/native-mpi-consumer/Cargo.toml
```

The locked dependencies must already be cached for offline execution. Compiler
selection uses the ordinary `CC`/`CXX` environment. The coordinator uses the
existing Rust MPI test supervisor to launch 1, 2, 4 and 8 local ranks, each with
a 60-second deadline. Do not put the coordinator inside `mpiexec`.

Each rank validates its complete amplitude partition against an analytic Bell
state, including phase and spectator coordinates, at absolute tolerance
`1e-13`. It also checks probabilities, register deployment and native
multithreading metadata, MPI usability after QuEST cleanup, and finalization
after the runtime owner is dropped. Two OpenMP threads are requested; actual
team size is not measured. These small local runs do not establish multi-host
capacity or a speedup.

The [native Clang record](../../2026-10-06-native-clang.md) distinguishes the
default installation's module-environment results from the separately built,
documented external-SDK configuration's loader-isolated results. The fixture
does not change QuEST, wrap native state storage or supply deployment fixes.
