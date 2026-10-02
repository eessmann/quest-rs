# Final architecture review addendum

Date: 2026-10-02.

Scope: read-only cleanup audit of current workspace/crate manifests, lockfile ownership, active Rust sources and aliases, user-facing documentation, benchmark scripts, and optimizer protocol/compiled-artifact/evidence version consistency. Historical plans, measurements, and reproduction sources were distinguished from active dependencies using their explicit provenance. No source or fixture edits, builds, benchmarks, or new tests were performed by this reviewer. Integrated validation results belong to the root agent's final receipts.

The final audit found one documentation issue: `crates/quest-optimizer-protocol/README.md` described version 1 and omitted affine-pi targets. The root corrected it. This reviewer subsequently read the file and verified version 3, all three exact target forms, canonical decimal integer strings, and reduced rational pairs with positive denominators.

No outstanding actionable issue remains from this audit:

- Active project manifests and production sources contain no Rug, Astro, GMP/MPFR, Malachite, or project-owned num-bigint/num-rational implementation dependency. Remaining lockfile num arithmetic belongs to optional upstream QuiZX/OpenQASM dependencies.
- `Binary = FBig<HalfEven, 2>` aliases in numerics, QSP, and the active primitive benchmark are direct native type specializations, not compatibility wrappers. Rational APIs reexport native `RBig`. Remaining `ExactConstant::Rational` occurrences are the intended exact-constant enum variant.
- Protocol, compiled-artifact, and compilation-evidence producers/consumers consistently use version 3. The unchanged syntax/SSA frontend template remains version 2.
- Historical backend names in measurement comparison scripts and frozen fixtures retain explicit provenance; they do not reintroduce an active project arithmetic backend.
- The earlier unused direct worker `dashu-base` declaration has been removed from its manifest and lockfile dependency list, as verified in the main architecture review.

Verdict for the cleanup audit above: passes. This addendum does not establish numerical correctness, measured performance, final test success, or Linux/MPI/accelerator validation.

## Follow-up feature-gating review

The root's no-default CLI check subsequently exposed an import guard in `quest-compile/src/beam.rs`. Read-only inspection verifies the correction: `Occurrence` and `SemanticOperation` are needed by unconditional source reconstruction/equivalence, whereas `ParameterId` is used only by worker-gated `bind_candidate`.

The follow-up scan found the same defect in `crates/quest-compile/src/workers.rs:10`: the model import was worker-gated even though this module is enabled by either `workers` or `synthesis`. Unconditional `rotation`, `semantic`, and replacement helpers require these names. The root removed the import guard; this reviewer verified that the model import is now unconditional within the already feature-gated module while the optional process-client import retains its worker guard. The finding is resolved at source level. Other inspected gated imports in `beam.rs` and `structured_workers.rs` match their guarded call sites. The root owns the queued pure, synthesis-only, and workers-only compiler checks and native direct-synthesis test; their results are not claimed here. No builds or tests were run by this reviewer.

## Native incremental-build follow-up

Read-only inspection identified inherited self-invalidation in both baseline and current native build scripts: bridge include watches include the script's own `OUT_DIR`, and CMake input watches include generated compiler/system files. Baseline fingerprints show these outputs newer than Cargo's saved run timestamp. This explains the observed unchanged `quest-sys` and downstream recompilation without attributing it to Dashu.

The `emit_input_watches` helper filters both emission sites and leaves CMake bridge arguments intact. The reviewer found that its initial lexical-prefix check incorrectly excluded external paths expressed through parent components or an inside-output symlink. The owner corrected this edge. Read-only inspection of the final source verifies that only successfully canonicalized inputs contained within canonical `OUT_DIR` are excluded; unresolved paths remain watched. Thus ordinary external files, sibling outputs, `OUT_DIR/../external.cpp`, and symlinks resolving to external headers retain invalidation. With no `OUT_DIR`, the helper retains every input watch. Existing source/environment watches remain in place.

The expanded regression fixture explicitly checks parent-relative external sources, unresolved future external includes, an inside-output symlink to external headers, native package headers/configuration/library files, and sibling outputs, while rejecting canonical own-output watches. This reviewer inspected the fixture but did not run it. Final source-level verdict: the identified self-invalidation and external-path preservation issues are resolved, with no further actionable finding. Actual no-op timing, owner test/Clippy evidence, and integrated validation remain root-owned; this reviewer performed no builds or tests.


## Final coordinator-results crosscheck

At delivery, this reviewer read the coordinator's [final validation receipt](validation-summary.json): all 17 recorded gates have exit code zero. The saved workspace log reports 1,010 passed tests and eight documented skips; saved pure-compiler, synthesis-only, workers-only, and direct-synthesis logs also report success. These are coordinator-run results inspected by the reviewer, not additional reviewer-run tests. Earlier pending/queued validation language in this review records the state at that earlier review and is superseded by the final receipt.

A final source/documentation scan found no direct legacy arbitrary-precision dependency or affected format-version mismatch. The migration ledger preserves the approved native Dashu, exact-angle, independent-verifier, canonical-interchange, and historical-provenance boundaries. [Performance results](performance.md) identify the final measured snapshots separately from initial measurements. Linux/MPI/accelerator execution remains explicitly unverified locally. No builds or tests were run during this delivery check.
