# Owned diagnostics

A diagnostic owns its machine-readable cause, human-readable message, primary
occurrence, labels, notes, suggestions, and provenance. Stable codes do not depend
on wording. Applications should branch on typed causes or codes, not parse prose.

| Code | Category |
|---|---|
| QL0001 | Syntax |
| QL0002 | Unknown symbol |
| QL0003 | Type mismatch |
| QL0004 | Arity mismatch |
| QL0005 | Resource limit |
| QL0006 | Unsupported capability |
| QL0007 | Invalid control flow / IR |
| QL0008 | Invalid source |
| QL0009 | Numerical failure |
| QL0010 | Lifecycle |
| QL0011 | Include failure |

Resource diagnostics retain the resource kind, requested quantity, and limit when
those values exist. Allocation failures and generic checked arithmetic failures use
`ResourceFailure`; they do not claim a compile-node overflow or invent requested
quantities. Stages identify parsing, admission, verification, optimization,
preparation, execution, and export boundaries.

Source snapshots retain immutable text independently of the caller's original buffer. `SourceId` identifies content custody rather than a filename. Spans use checked byte ranges and UTF-8 boundaries; a display label cannot authorize slicing a different source. Multiple labels can distinguish a failing use from a declaration. Definition, call, and include frames preserve the path to an error.

The optional `codespan-reporting` feature provides plain text rendering without terminal hooks. Optional serde support preserves the same owned data model; deserialization checks source-map identity and span invariants. The compiler does not install a global error handler or reread changed files to render an old diagnostic.

Native execution can fail after a completed prefix. `Error::report()` preserves an
owned diagnostic beside the typed runtime failure. It identifies the owning
program, block-local instruction, interpreter call frames, completed quantum
prefix, and source occurrence. Valid text spans become labels. When an inline Rust
macro has compiler coordinates but no source snapshot, the report emits the
retained file, line, and column as a location note instead of inventing source
text. Reports survive dropping the program, prepared plan, and environment.

```rust
{{#include ../../../crates/quest-circuit/tests/tutorials.rs:owned_diagnostic}}
```
