# Compiler consolidation

Baseline: `001a2b656a5a80a60659a408f87f57670309a09b`. Implementation in the shared worktree; no commits made by this agent.

## Changes

- Removed `quest-circuit`; moved its integration tests, UI fixtures, and compiler bench to `quest-compile`. `quest-compile` exports `circuit!` / `circuit_file!` under `macros`, and macro crate resolution supports renamed `quest-compile` dependencies. Root owns downstream dependency migration and workspace manifest/lock changes.
- Removed compiler `model`, `program`, `payload`, `provenance`, and `rational` forwarding modules and their injected unused extension-trait imports. Shared language types are imported from their owners; actual compiler extension traits are imported where used.
- Native `synthesis` and external `workers` features are independent. `NativeSynthesis` calls `quest_synthesis::approximate_rotation` directly. Removed optimizer-client native synthesis relay and dependency. Rotation passes and candidate admission use generic `impl RotationGenerator`; independent `quest_math::certify_rotation` recertification remains mandatory.
- Removed boxed erased structured/worker variants from the language semantic error. Search returns compiler-owned `CompilerError`, retaining concrete semantic, structured-terminal, and worker errors. Ordinary language capability construction continues to use the language error.
- Added acyclic typed historical certificate records in `quest-language::quantum::evidence`: mathematical `Target`, `Sequence`, and `Limits`, plus a typed finite/SSA location enum. Provenance graphs retain typed `Arc<CompilationEvidence>` instead of encoded byte blobs. Finite snapshot/provenance identifiers in compiled publications are typed. This requires the one-way dependency `quest-language -> quest-math`; math has no language dependency. Compiler artifact loading remains the certificate verification authority, and historical local records do not certify current executable equivalence.
- Replaced finite-region synthetic gate/conditional AST reconstruction and lexical string rewriting with dedicated `FiniteOperation` semantic data and checked direct SSA insertion. The lowering checks lexical handle ownership, operand shapes and bounds, uses the common SSA verifier for aliases/arity/effects, retains exact capture targets and source/bound provenance, and creates one dominating definition per scalar capture in each finite fragment. Concrete noncaptured finite gates/effects still export to QASM.
- Oracle captures and scalar captures have independent typed banks. Builder/finite imports and macro expansion no longer allocate dummy floating values for oracle captures. Macro scalar index compaction preserves Rust capture evaluation order and evaluates each capture once.

## Format migration

Compiled publication envelope v1 -> v2; historical compilation evidence v1 -> v2; frontend template v1 -> v2 because retained syntax now includes finite semantic fragments. Prior versions are rejected, with no legacy decoder. Worker protocol remains at its preexisting version because its schema did not change.

## Validation

All commands use the project `devenv shell`.

- `cargo check -p quest-compile --lib --no-default-features`: passed.
- `cargo check -p quest-compile --lib --features synthesis,macros`: passed.
- `cargo check -p quest-compile --lib --no-default-features --features workers`: passed.
- Focused artifact, finite semantic, generator error/provenance, snapshot, and unified-program tests: 36 passed before final additional regressions.
- Strict library Clippy (`-D warnings`) for compiler, language, macros, optimizer-client, and qasm with all features: passed.
- Full migrated compiler + language + macro + process client + QASM tests with all features: **364 passed across 56 test binaries**, including 20 macro compile/pass cases. Logs: `compiler-all.log`.
- Final focused artifact, finite semantic and generator regressions: **16 passed**, including the additional malicious-generator request-recertification regression and evidence-v1 rejection (`compiler-final-regressions.log`).
- Renamed dependency fixture: `cargo run --manifest-path .superpowers/sdd/2026-10-02-static-numerical-core/renamed-compiler/Cargo.toml --target-dir target` passed; renamed `compiler::circuit!` constructed and verified its executable. Log: `compiler-renamed.log`.
- `cargo tree -p quest-compile --no-default-features --features synthesis --edges normal --prefix none` contains no optimizer-client or optimizer-protocol package (`compiler-native-tree.log`).
- Final no-feature / native-only / workers-only library checks and strict all-feature library Clippy: passed (`compiler-final-gates.log`). The native-only check exposed an unused process-only `ToPrimitive` import; it is now correctly gated by `workers`.
- Scoped cargo formatting and `git diff --check`: passed.

Linux worker execution tests are platform-gated and are not Linux runtime evidence on this Darwin host. Native execution/build evidence belongs to the native owner; no native numerical kernels or binding-generation files were edited here. Root owns whole-workspace validation and independent review.
