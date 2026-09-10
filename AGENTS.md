# Repository Guidelines

## Project Structure & Module Organization

- `src/core/` implements the public Rust environment, register, and error abstractions; `src/lib.rs` exports the interface.
- `crates/quest-sys/` contains the low-level `cxx` bridge, C++ wrappers in `src/cxx_bindings/`, build discovery in `build-support/`, and integration tests in `tests/`.
- `crates/xtask/` holds binding-generation logic and templates. Generated bindings and API manifests live in `crates/quest-sys/src/` and `crates/quest-sys/generated/`.
- `examples/` and `crates/quest-sys/examples/` provide runnable examples. Generator unit tests sit beside their implementation.

## Build, Test, and Development Commands

Use Rust supporting edition 2024, CMake, a C++20 compiler, and an installed QuEST package. Set `export QUEST_ROOT=/path/to/quest`; `QUEST_DIR` or `CMAKE_PREFIX_PATH` also work. The bridge requires double precision and deprecated QuEST APIs disabled. Generation additionally needs libclang; set `LIBCLANG_PATH` if discovery fails.

Run from the repository root:

- `cargo build --workspace`: build all crates and native bridges.
- `cargo nextest run --workspace`: run unit and integration tests.
- `cargo test --doc --workspace`: run doctests separately from Nextest.
- `cargo run --example minimal`: run the public-interface example.
- `cargo fmt --all -- --check`: check Rust formatting.
- `cargo clippy --workspace --all-targets`: inspect Rust lint diagnostics.

## Coding Style & Naming Conventions

Use four-space Rust indentation and rustfmt. Prefer `snake_case` functions/modules, `UpperCamelCase` types, and `SCREAMING_SNAKE_CASE` constants. Format C++ using `crates/quest-sys/.clang-format` (Chromium, C++20). Preserve `QuestResult` error propagation and RAII ownership; drop native handles before finalizing the environment.

## Testing Guidelines

Use `googletest` with `#[gtest]` for runtime assertions; build-support tests also use `#[test]`. Name tests after behavior, such as `finalize_fails_while_raii_handles_are_live`. Preserve the serialized `quest-sys` test group in `.config/nextest.toml`. Cover changed adapters, invalid inputs, lifecycle behavior, and numerical results with explicit tolerances. No numerical coverage threshold is configured.

## Generated Bindings

Edit generator logic/templates and the adapter registry `crates/quest-sys/generated/generated_adapters.json`, then run `cargo run -p xtask -- generate-quest-bindings`. Commit regenerated artifacts together; append `--check` to verify freshness without rewriting files.

## Commit & Pull Request Guidelines

History uses short descriptive subjects, sometimes prefixed with `test:` or `example:`; no strict convention is established. Write imperative subjects and keep changes focused. PRs should explain behavior changes, link relevant issues, list validation commands/results, and identify the QuEST version/configuration used. Include generated artifacts when bindings change.
