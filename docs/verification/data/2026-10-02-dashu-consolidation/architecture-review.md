# Independent Dashu architecture and consumer review

Reviewed 2026-10-02, read-only except this report. Compared the compiler/worker migration with `.superpowers/sdd/2026-10-02-rug-migration/source-before` so the earlier static-architecture migration was not mistaken for new Dashu work. Read the consolidation plan and exact/numerics handoff reports. No builds or tests were run by this reviewer; the parent owns final-tree validation. Interval, floating-point conversion, and exact mathematical algorithms belong to the separate numerical review.

## Findings

No correctness or compatibility blocker found in the reviewed Dashu changes. There is no concrete failing consumer example to report.

Resolved during review: the worker manifest declared an unused direct `dashu-base` dependency. The parent removed it and refreshed `Cargo.lock`; this reviewer verified both files. Dashu base remains a necessary transitive dependency, so this cleanup does not change runtime behavior. The parent reports a successful worker all-features check after removal.

The parent also fixed a remaining `Rational::from_integer` tutorial example and removed a test-only `Rational = RBig` alias while running the integrated gate. A final reviewer search of active crate Rust sources finds only the intentional `ExactConstant::Rational` enum variant, not old rational types or aliases.

## Contract and API checks

- Optimizer protocol is version 3. Client request creation and response admission use the shared `VERSION`; worker request admission and response creation use the same constant. Active scripted response fixtures use 3. No legacy decoder was added.
- Structured artifact production and loading both use version 3. Both finite and structured compilation-evidence producers emit 3, and artifact evidence admission requires 3. Repository-wide searches found no other old version-2 producer in these formats.
- Frontend template production/loading deliberately remain version 2. That DTO contains syntax and SSA, not the changed arbitrary-precision payloads, so retaining this version is consistent with the stated plan.
- Public compiler, language, symbolic, and math rational interfaces expose native `RBig`. The top-level runtime reexports the compiler API. There is no compatibility `BigRational`/`Rational` alias, scalar wrapper, or backend selector in active production sources.
- Compiler reconstruction in finite import, beam source replay, and bound parity retains the exact rational-pi and affine-pi branches using `RBig::from_parts_signed`; it does not route exact targets through binary64. Signed raw denominator support remains at the existing checked boundaries. Bound-region producer invariants continue to supply valid raw exact targets to private replay functions.
- Removing denominator-sign and renormalization checks for already-native `RBig` values in optimizer reuse/approximation admission is appropriate. Native positive-denominator/canonical rational invariants replace the old unchecked `Ratio::new_raw` possibility. Positive numerator and coefficient-bit limits remain checked.
- `ParityOptions::max_coefficient_bits` and its arithmetic/storage accounting consistently use `usize`, matching native bit lengths and allocation sizes. Remaining public `u64` resource boundaries use checked/saturating conversions. Reviewed signed parity reduction and quarter-turn consumer paths preserve the explicit negative-remainder correction.
- Exact JSON interchange uses deliberate canonical decimal strings, positive denominators, and reduced rational pairs. Structured `AngleData` decoding delegates to the same canonical rational parser. `AngleTarget` serialization normalizes signed local pairs; decoding rejects noncanonical/unreduced pairs. The intentional raw-target round-trip normalization is covered by exact-wire tests in the handoff and is not a loss of mathematical identity.
- The decimal serde helpers own temporary strings/vectors at interchange boundaries; no new text conversion was introduced into compiler scoring/parity arithmetic. Existing exact target clones and reconstruction reductions are boundary operations, not a generic per-operation adapter layer.

## Cleanup and provenance checks

Workspace dependencies pin the four Dashu components directly, with default features disabled. Active project source/manifests contain no Rug, Astro, Malachite, num-bigint, or num-rational implementation dependency. Lockfile num-bigint/num-rational entries remain under upstream `num`, consistent with the optional external-engine allowance.

Old `BigRational` names found in optimization-roadmap fixtures are documented frozen September reproduction sources requiring their matching historical checkout. The active primitive benchmark manifest/source is Dashu-only. Its old Astro/Rug result records are explicitly historical and link to the archived source/manifest/lock tarball; preserving those records is correct provenance rather than unfinished cleanup.

## Verdict

Spec review: passes for the reviewed compiler/worker/API/serialization and cleanup scope, with final workspace configuration gates still pending at the parent.

Quality review: suitable to proceed; the identified manifest cleanup is resolved. No unnecessary compatibility layer or new hot-path text/serialization adapter found. This review makes no claim about final tests, measured HPC performance, Linux/MPI/accelerator execution, or the numerical algorithms assigned to the other reviewer.


## Final coordinator-results crosscheck

At delivery, this reviewer read the coordinator's [final validation receipt](validation-summary.json): all 17 recorded gates have exit code zero. The saved workspace log reports 1,010 passed tests and eight documented skips; saved pure-compiler, synthesis-only, workers-only, and direct-synthesis logs also report success. These are coordinator-run results inspected by the reviewer, not additional reviewer-run tests. Earlier pending/queued validation language in this review records the state at that earlier review and is superseded by the final receipt.

A final source/documentation scan found no direct legacy arbitrary-precision dependency or affected format-version mismatch. The migration ledger preserves the approved native Dashu, exact-angle, independent-verifier, canonical-interchange, and historical-provenance boundaries. [Performance results](performance.md) identify the final measured snapshots separately from initial measurements. Linux/MPI/accelerator execution remains explicitly unverified locally. No builds or tests were run during this delivery check.
