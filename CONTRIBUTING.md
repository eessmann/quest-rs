# Contributing

## Development environment

Use the rolling nightly in `rust-toolchain.toml`, CMake 3.28+, a C++20 compiler,
and an installed QuEST 4.3.x package with binary64 precision and deprecated APIs
disabled. The full workspace also needs serial HDF5. See the
[build instructions](README.md#build) for installed packages and the optional Nix
shell, or the [Grace Hopper guide](docs/grace-hopper.md) for manual/Spack
dependencies and CUDA.
See [native tooling](docs/native-tooling.md) for module-based builds, diagnostic
receipts, Cargo target isolation and local/Slurm MPI test supervision.

Run commands from the repository root in a configured native environment or
the optional `devenv shell`. Record `rustc -Vv` and the native configuration with
validation results. Linux GNU and aarch64/x86_64 Darwin need separate native
validation; the optional optimizer process client is Linux-only.

## Repository layout

The root is a virtual Cargo workspace; the [crate table](README.md) describes
the public packages. Common implementation locations are:

- `crates/quest/src/`: environments, typed registers, preparation and execution.
- `crates/quest-language/`: checked semantics, typed builders, exact angles, SSA and quantum regions.
- `crates/quest-compile/`: canonical compiler API and macros, staged programs, specialization, optimization and portable artifacts.
- `crates/quest-qasm/` and `crates/quest-macros/`: text interchange and checked token templates.
- `crates/quest-numerics/` and `crates/quest-polynomial/`: checked arithmetic, enclosures, AD, root coverage, static expressions and approximation certificates.
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

For Linux all-feature acceptance, use either a system-installed MPI/SUBCOMM
QuEST with matching MPICH and serial HDF5, or `devenv --clean shell`, which
selects these dependencies together. Run:

```sh
cargo build --workspace --all-features --locked
cargo nextest run --workspace --all-features
cargo test --doc --workspace --all-features --locked
cargo nextest run -p quest-qsp --all-features --release --run-ignored only
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo run --locked -p quest-rs --example minimal
```

`devenv --clean test` runs the first four commands and the minimal example.
Formatting, binding freshness and installed native consumer checks from the
preceding list remain part of acceptance. Preserve failures and identify the
exact Rust, QuEST, MPI and HDF5 installations in the verification record.

Keep separate, initially empty Cargo target directories for the two Linux
environments. For Fedora's installed MPICH, an isolated system shell can be
started with the following command (replace the QuEST prefix):

```sh
env -i HOME="$HOME" USER="$USER" LANG=C.UTF-8 \
  PATH="$HOME/.cargo/bin:/usr/lib64/mpich/bin:/usr/bin:/bin" \
  QUEST_ROOT=/path/to/installed/quest MPICC=/usr/lib64/mpich/bin/mpicc \
  CMAKE_PREFIX_PATH=/usr/lib64/mpich \
  PKG_CONFIG_PATH=/usr/lib64/mpich/lib/pkgconfig CC=/usr/bin/cc CXX=/usr/bin/c++ \
  CARGO_TARGET_DIR="$PWD/target/system-all-features" \
  bash --noprofile --norc
```

This clears inherited Nix, HDF5 and loader overrides; Fedora's serial HDF5 is
discovered automatically. Use the corresponding package paths on other hosts.
For the Nix lane, set its target directory inside the clean shell:

```sh
devenv shell --clean -- bash -c \
  'export CARGO_TARGET_DIR="$PWD/target/devenv-all-features"; exec bash --noprofile --norc'
```

Run the acceptance commands above from each shell. The explicit `--` separates
devenv options from the invoked command. Native ABI checks verify that QuEST's
loaded MPI library matches the compiler wrapper selected by `MPICC`.

Nextest does not run doctests; run them separately. Native consumer checks
default to CPU. Use `--backends cpu,omp,gpu` when those native backends and host
devices are available. Run the feature combinations affected by a change; MPI
requires an MPI/SUBCOMM-enabled QuEST installation.

For an MPICH installation, select its compiler wrapper explicitly
before running the MPI integration tests:

```fish
set -gx MPICC /path/to/mpich/bin/mpicc
fish_add_path --prepend /path/to/mpich/bin
cargo nextest run -p quest-sys --features mpi --test mpi --locked
```

Replace the placeholder with the wrapper and launcher directory from the MPI
installation used by QuEST. Discovery follows the locked rsmpi dependency:
`MPI_PKG_CONFIG`, then `CRAY_MPICH_DIR`, then `MPICC`, then fallback probes.
Keep module-provided compiler and header settings; when explicitly selecting a
different MPI through `MPICC`, remove higher-priority MPI selectors first.
Native and generated-Rust ABI witnesses reject incompatible selections. The clean Linux
devenv supplies the absolute development-output wrapper and matching binary-output
launcher itself. System serial HDF5 can be autodetected with `HDF5_DIR` unset;
devenv selects its pinned serial package explicitly. QuEST's installed runtime
paths must resolve native dependencies with loader overrides cleared.

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
