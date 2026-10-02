# Imports and canonical export

Text import starts with an immutable `SourceSnapshot` and an explicit `IncludeResolver`. The pure compiler performs no filesystem reads and does not install global source hooks. An application chooses whether include names refer to files, embedded strings, a package store, or no source at all.

`SourceId` is an opaque identity independent of a filename. Distinct snapshots may share a display name; a resolver must not reuse an identity for different content. The importer retains source snapshots and include edges, checks cycles, and applies independent source-byte, include-count, and depth limits before admitting expanded syntax.

The bundled `StandardLibrary` resolves only the pinned `stdgates.inc` snapshot. Its source provenance and deliberate phase corrections are recorded in the repository's `docs/openqasm-provenance.md`. Supplying a standard-library resolver is explicit; an include name does not grant arbitrary host filesystem access.

```rust
{{#include ../../../crates/quest-compile/tests/tutorials.rs:import_export}}
```

`quest_qasm::export` writes canonical root syntax while retaining its include directives. `export_typed` writes the expanded admitted syntax. `export_syntax` accepts structured syntax directly. Formatting is normalized and expressions are parenthesized; comments and original whitespace are not promised. The test reimports the exported root and verifies canonical stability.

Structured optimization preserves the original admitted syntax as export authority. Export therefore expresses the same structured program, not the particular optimized SSA schedule. Rust captures cannot be reconstructed as source expressions from a captured floating value; choose explicit text inputs when source-level interchange is required.

## Compiler-tracked files

`circuit_file!` takes a literal path relative to its Rust source file. The macro reads the entry and resolved include files at compilation and emits compiler-visible dependencies. Cargo rebuilds when tracked source changes. This frontend feeds the same admission and SSA pipeline as text import and `circuit!`.

```rust
{{#include ../../../crates/quest-compile/tests/tutorials.rs:compiler_file}}
```

Its fixture is actual source included by that test:

```qasm
{{#include examples/bell.qasm}}
```

The in-memory text APIs remain useful when a caller needs a custom resolver, detailed owned include provenance, or import before a native environment exists.
