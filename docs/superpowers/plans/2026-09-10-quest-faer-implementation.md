# QuEST workspace implementation with faer

Approved by the user on 2026-09-10. Implements the architecture in
[the design](../specs/2026-09-10-quest-rust-design.md) and its dated bridge audit.

1. Move the facade to `crates/quest`, expose `quest`, create a virtual resolver-3
   workspace, centralize dependency constraints and lints, and pin nightly-2026-09-06.
2. Repair bridge safety, owner-thread lifecycle admission, matrix adapters, 4.3
   metadata, native discovery and final-target runtime paths (M0).
3. Implement environment-bound strong runtime types and pure owning circuit/DAG
   stages, with immutable faer matrices and explicit numerical budgets (M1–M2).
4. Implement binding, lowering, transactional preparation, execution and an
   OpenQASM-style Rust macro pinned to the 3.1.0 gate semantics (M3–M4).
5. Implement conservative exact transformations and bounded faer fusion (M5).
6. Review integration and verify pure/native, compile-fail, numerical, generator,
   package and standalone direct/wrapped downstream acceptance.

Use faer 0.24.4 with only std/linalg and explicit Par::Seq. Numeric residual checks
never confer exact unitary semantics. Matrix packing uses logical values, gate
buffers row-major and density buffers column-major, preserving ordered targets.
Use cargo-edit for dependency additions; workspace inheritance is maintained in
manifests. Libraries have typed errors; source rendering is optional.

Track the actual execution and evidence in implementation-status.md. Preserve the
original audit as dated baseline evidence, adding follow-up evidence separately.
