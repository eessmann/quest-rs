# quest-rs

Idiomatic Rust bindings for the QuEST quantum simulation toolkit.

## Testing

The primary test runner is `cargo nextest`. Install it from the official
prebuilt binaries if it is not already available:

```sh
curl -LsSf https://get.nexte.st/latest/mac | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
```

The low-level `quest-sys` tests need a local QuEST install. For the standard
local setup, run:

```sh
CMAKE_PREFIX_PATH=/Users/erich/Projects/opt/quest cargo nextest run --workspace
```

Nextest does not run doctests, so run them separately:

```sh
CMAKE_PREFIX_PATH=/Users/erich/Projects/opt/quest cargo test --doc --workspace
```
