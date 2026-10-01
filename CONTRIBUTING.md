# Contributing

## Development environment

Use the rolling nightly in `rust-toolchain.toml`, CMake 3.28+, a C++20 compiler,
and an installed QuEST 4.3.x package with binary64 precision and deprecated APIs
disabled. The full workspace also needs serial HDF5. See the
[build instructions](README.md#build) for installed packages and the optional Nix
shell, or the [Grace Hopper guide](docs/grace-hopper.md) for manual/Spack
dependencies and CUDA.

Run commands from the repository root in a configured native environment or
the optional `devenv shell`. Record `rustc -Vv` and the native configuration with
validation results. Linux GNU and aarch64/x86_64 Darwin need separate native
validation; the optional optimizer process client is Linux-only.

## Repository layout

The root is a virtual Cargo workspace; the [crate table](README.md) describes
the public packages. Common implementation locations are:

- `crates/quest/src/`: environments, typed registers, preparation and execution.
- `crates/quest-language/`: checked semantics, typed builders, exact angles, SSA and quantum regions.
- `crates/quest-compile/`: staged programs, specialization, optimization and portable artifacts.
- `crates/quest-circuit/`, `crates/quest-qasm/` and `crates/quest-macros/`:
  public construction facade, text interchange and checked token templates.
- `crates/quest-synthesis/` and `crates/quest-math/`: bounded candidate generation and independent exact verification.
- `crates/quest-qsp/` and `crates/quest-qsvt/`: response synthesis, frozen numerical certification and typed transformation routes.
- `crates/quest-build/`: installed native configuration and executable runtime paths.
- `crates/quest-sys/src/cxx_bindings/`: C++ adapters and the low-level CXX bridge.
- `crates/xtask/`: binding generator, templates and native consumer validation.
- `crates/quest/examples/` and `crates/quest-sys/examples/`: runnable examples.
- `docs/book/`: user guide, with source snippets from examples and tests.

## Checks

```sh
cargo build --workspace --locked
cargo nextest run --workspace --locked
cargo test --doc --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run --locked -p xtask -- generate-quest-bindings --check
cargo run --locked -p xtask -- check-native-consumers
mdbook build docs/book
```

Nextest does not run doctests; run them separately. Native consumer checks
default to CPU. Use `--backends cpu,omp,gpu` when those native backends and host
devices are available. Run the feature combinations affected by a change; MPI
requires an MPI/SUBCOMM-enabled QuEST installation.

For an MPICH installation, select its compiler wrapper explicitly
before running the MPI integration tests:

```fish
set -gx MPICC /path/to/mpich/bin/mpicc
cargo nextest run -p quest-sys --features mpi --test mpi --locked
```

Replace the placeholder with the wrapper from the MPI installation used by QuEST.

MPI tests require local networking, including the two- and four-rank subprocess
tests; run them outside a sandbox that blocks local MPI communication.

Use `googletest` with `#[gtest]` for runtime assertions and build-helper
regressions; compile-fail harnesses use `#[test]`. Name tests after behavior,
cover invalid inputs and lifecycle changes, and use explicit numerical tolerances.
Preserve the serialized `quest-sys` group in `.config/nextest.toml` and existing
subprocess isolation. No numerical coverage threshold is configured.

## Style and ownership

Use rustfmt with four-space Rust indentation. Prefer `snake_case` for functions
and modules, `UpperCamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
Format C++ with `crates/quest-sys/.clang-format` (Chromium, C++20).

Preserve `QuestResult` error propagation and RAII ownership. Destroy native
handles before finalizing their environment. Document caller-visible behavior
changes and keep examples consistent with the public API.

## Generated bindings

Edit generator logic/templates and
`crates/quest-sys/generated/generated_adapters.json`, then run:

```sh
cargo run --locked -p xtask -- generate-quest-bindings
```

Commit generated bindings and API manifests with the generator changes.
Generation requires matching Clang and libclang; set `CLANG` and `LIBCLANG_PATH`
when discovery needs an explicit selection. Append `--check` to verify freshness
without rewriting files.

## Documentation and changes

Keep current usage in the [user guide](docs/book/src/index.md), crate READMEs and
[setup guides](docs/README.md). Put reproducible technical measurements in
[verification records](docs/verification/README.md), with configuration, commands,
results and limitations. Keep temporary build output outside the documentation.

Use short, imperative commit subjects and focused changes. Pull requests should
explain resulting behavior, link relevant issues, list validation results and
identify the QuEST version/configuration used. Include generated artifacts when
bindings change.
