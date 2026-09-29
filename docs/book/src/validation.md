# Build and validate the tutorials

Use the repository's rolling `nightly` and record `rustc -Vv` with results. Pure compiler tests need no native QuEST installation:

```sh
cargo test -p quest-circuit --test tutorials --locked
cargo test -p quest-language --locked
cargo test -p quest-qasm --locked
```

Native examples need CMake 3.28+, a C++20 compiler and an installed QuEST
4.3.x package with binary64 precision and deprecated APIs disabled. From the
repository root, `devenv shell` supplies the configured toolchain and a shared
CPU/OpenMP QuEST package; `devenv build outputs.quest` builds that package, and
`devenv test` runs the minimal native example. The full workspace also uses the
shell's serial HDF5. The native recipes target Linux GNU and aarch64/x86_64
Darwin; each architecture needs separate runtime validation.

For a manual installation, set `QUEST_ROOT` to a package exporting
`QuEST::QuEST` whose native dependencies resolve, and select serial HDF5 through
pkg-config or `HDF5_DIR` for the full workspace. The
[workspace setup](https://github.com/eessmann/quest-rs/blob/main/README.md#build)
describes CMake selection and the final-executable link helper. The
[Grace Hopper guide](https://github.com/eessmann/quest-rs/blob/main/docs/grace-hopper.md)
covers manual/Spack dependencies and GPU checks. Once configured:

```sh
cargo run -p quest-rs --example tutorials --locked
cargo test -p quest-rs --test tutorials --locked
```

The native test runs the same functions shown in this book, inside a subprocess. It checks Bell probabilities, teleportation fidelity, deterministic feedback, bounded retry success, once-only captures, and array mutation. The subprocess owns one environment whose scope ends after all borrowing native resources have been destroyed; `Drop` then automatically finalizes the runtime.

```rust
{{#include ../../../crates/quest/tests/tutorials.rs:native_tutorial_tests}}
```

## Numerical and QSP tutorials

These examples need no native QuEST installation. They share the exact source
included by the numerical/QSP chapters and their googletest integration tests:

```sh
cargo run -p quest-qsp --example qsp_tutorials --locked
cargo test -p quest-qsp --test tutorials --locked
cargo run -p quest-qsp --example qsp_tutorials --features offline-synthesis --locked
cargo test -p quest-qsp --test tutorials --features offline-synthesis --locked
```

The feature-enabled run includes independent Astro Float certification and explicit
offline synthesis/approximation. It never changes the ordinary production route.
Compile-time stage-order failures are also exercised by the crate doctests.

```sh
cargo test -p quest-polynomial -p quest-qsp --doc --features quest-qsp/offline-synthesis --locked
cargo test -p quest-qsvt --doc --locked
cargo doc -p quest-qsp -p quest-qsvt --all-features --no-deps --locked
cargo bench -p quest-qsp --bench pipeline --locked
cargo bench -p quest-qsp --bench pipeline --features offline-synthesis --locked
```

Benchmarks separate binary64 construction, frozen response evaluation, cold
certification and explicit offline synthesis including its final certification.
The actual degree-8105 catalog scale run is a separately recorded verification
fixture; the small tutorial is not a claim of exhaustive numerical coverage.

The pure QSVT builder examples come from the crate README, which is also its
crate-level rustdoc and is compiled by Cargo doctests. For native preparation,
postselection and complex Hadamard observations, use the process-isolated
runtime test and the same example included in the native QSVT chapter:

```sh
cargo run -p quest-rs --example qsvt --features qsvt --locked
cargo nextest run -p quest-rs --test qsvt_runtime --features qsvt --locked
```

## Optional real workers

The optional optimizer process client runs on Linux only; macOS returns a capability error. Linux worker execution requires `/usr/bin/prlimit` specifically, including when another `prlimit` is on `PATH`. On a supported Linux host, build a worker explicitly, then pass its absolute path. The client does not search PATH or silently substitute an engine:

```sh
cargo build -p quest-optimizer-worker --features synthesis,zx --locked
cargo run -p quest-rs --example tutorials --features workers --locked -- "$PWD/target/debug/quest-optimizer-worker"
QUEST_TUTORIAL_WORKER="$PWD/target/debug/quest-optimizer-worker" cargo test -p quest-rs --test tutorials --features workers --locked
```

If `CARGO_TARGET_DIR` is set, use that directory in the executable path. Without `QUEST_TUTORIAL_WORKER`, the optional integration test does not run synthesis. The ordinary native tutorials remain independent of worker installation.

## Build this book

This guide is validated with mdBook 0.5.4:

```sh
mdbook build docs/book
```

Its output directory is the worktree's `target/book`. Source includes resolve directly to the Rust examples and integration tests, so displayed code changes with tested source. mdBook rendering alone does not compile Rust snippets; run the Cargo commands too.

For repository-wide validation, follow the
[contributor guide](https://github.com/eessmann/quest-rs/blob/main/CONTRIBUTING.md):
use Nextest for runtime tests and Cargo doctests separately, then rustfmt, strict
Clippy, and generated-binding freshness checks. Native lifecycle tests require
the serialized Nextest group or their explicit subprocess harness. The
[verification index](https://github.com/eessmann/quest-rs/blob/main/docs/verification/README.md)
records tested configurations and results.
