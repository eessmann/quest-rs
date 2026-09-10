# quest-rs

Idiomatic Rust bindings for the QuEST quantum simulation toolkit.

## Testing

The primary test runner is `cargo nextest`. Install it from the official
prebuilt binaries if it is not already available:

```sh
curl -LsSf https://get.nexte.st/latest/mac | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
```

The low-level `quest-sys` tests need a local QuEST install. Point either
`QUEST_ROOT`/`QUEST_DIR` or `CMAKE_PREFIX_PATH` at its installation prefix:

```sh
QUEST_ROOT=/path/to/quest cargo nextest run --workspace
```

Nextest does not run doctests, so run them separately:

```sh
QUEST_ROOT=/path/to/quest cargo test --doc --workspace
```

When QuEST is shared, workspace test and example executables embed the
resolved QuEST library directory as an rpath on Linux and macOS. Static QuEST
builds are linked without adding a QuEST runtime path; static archives must be
built with `CMAKE_POSITION_INDEPENDENT_CODE=ON` for position-independent Rust
executables.
