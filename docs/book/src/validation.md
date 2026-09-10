# Build and validate the tutorials

Use the repository's pinned `nightly-2026-09-06`. Pure compiler tests need no native QuEST installation:

```sh
cargo test -p quest-circuit --test tutorials --locked
cargo test -p quest-language --locked
cargo test -p quest-qasm --locked
```

Native examples require installed QuEST 4.3.x, binary64 precision, deprecated APIs disabled, CMake, and a C++20 compiler. The repository root README describes generating `QUEST_NATIVE_CONFIG` and final-executable runtime paths. Once configured:

```sh
cargo run -p quest-rs --example tutorials --locked
cargo test -p quest-rs --test tutorials --locked
```

The native test runs the same functions shown in this book, inside a subprocess. It checks Bell probabilities, teleportation fidelity, deterministic feedback, bounded retry success, once-only captures, and array mutation. The subprocess owns one environment and closes it after all borrowed native resources are dropped.

```rust
{{#include ../../../crates/quest/tests/tutorials.rs:native_tutorial_tests}}
```

## Optional real workers

Build a worker explicitly, then pass its absolute path. The client does not search PATH or silently substitute an engine:

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

For repository-wide validation, use Nextest for runtime tests and Cargo doctests separately, then rustfmt, strict Clippy, and generated-binding freshness checks. Native lifecycle tests require the serialized Nextest group or their explicit subprocess harness. Focused tutorial success does not claim a complete workspace release audit.
