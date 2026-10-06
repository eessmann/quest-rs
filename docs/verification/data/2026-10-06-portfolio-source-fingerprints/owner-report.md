# Portfolio commutative-source fingerprint repair — frozen handoff

All four confirmed source families now use the CFD-owned domain-separated SHA256-per-record helper before commutative addition. The owned eight-file snapshot is portfolio-source-fingerprints-source.json, aggregate2e9a6d9343ca15fa68d8745fe8e297f22c35bab0d2e1940126f9ec738d86726d, manifest SHA5065fe0f67b69d6986906cfa18e907faf3ffdaea5057ba88a5ae1bb1dafcf5a5. Root/native QuEST/shared library exports/Cargo/record helper/matching/IO files were not edited by this owner. No campaign, tolerance, operator, scientific premise or historical receipt was changed.

## Actual reproduced scope

The standalone public API probe <private-artifacts>/quest-portfolio-identity-review/src/main.rs and <private-artifacts>/quest-portfolio-identity-review-behavior.log reproduced: LcuPlan and WeightedLcu q2 offsets0/1 with(+i,−i) versus(−i,+i); ArithmeticStencil Base/PREP q2 offsets0/2; StructuredStencil q5 offsets12/16; TensorShift q16 disjoint8-bit ranges offsets(1,129) versus(129,1). Each old source identity collided while operators differ (at a declared matrix entry, or image0=33025 versus385). All old ordered construction identities differed and descriptors were unequal; this was a source provenance defect, not the matching wrong-shard descriptor-substitution case. SHA256 persisted integrity was not bypassed.

The maintained commutative_identities target first failed all four operator-identity tests behaviorally (<private-artifacts>/quest-portfolio-commutative-red.log,4 failed). Same assertions now pass, with reorder-invariant source IDs and different ordered construction IDs. Numerical shift/add/angle/PREP/SELECT/adjoint streams were left unchanged. No claim is made that a64-bit fingerprint is collision-free/authentication, or that all mathematically equivalent decompositions canonicalize identically. Sequential construction FNV was retained; these fixtures do not reproduce a sequential construction collision.

## Ownership/work admission

owned_replay.rs uses distinct version2 record/source domains for shift and legacy stencil. structured.rs and structured_stencil.rs cache immutable source IDs once during construction, retain source_fingerprint_work() on clones, and include their two added scalar fields through sizeofSelf accounting. The getter records only that constructor's SHA component: stencil prior child shift preparation is separate. Byte-only NumericalPolicy adds1024 digest-stack allowance but still imposes no work ceiling. Descriptor reads reuse cached source IDs; original source construction is not free or erased from receipts. No generic work trait or hard bound on arbitrary caller descriptor loops was added.

LcuPlan pre-admits checked SHA metadata work/cardinality/input-live bytes before its hash loop. Its existing4096 general allowance includes digest scratch; previously constructed child sources/replay are separate. Raw signed weights, exact-zero provenance and nonzero underflow SELECT branches remain intact. ArithmeticStencil hashes full weight/axis-count/width/offset records and precharges both its own records and all child shift SHA it constructs; PREP/counting consumes remaining aggregate work. The original input tuple work is retained even for filtered zero terms.

Root found an inherited offset-owner gap during review: all later offset Vec allocations are live during the first term. The new maintained invalid-first-offset +64KiB-later-capacity probe under32KiB first failed budget-priority assertion (<private-artifacts>/quest-portfolio-offset-owners-red.log). Checked total offset capacities are now admitted after cheap cardinality/work checks and before any record validation/hash, with incremental doublecharge removed; same test passes (<private-artifacts>/quest-portfolio-offset-owners-green.log).

Digest scratch/work scope is engine+these bounded fixed iterators, not arbitrary user callbacks, allocator/RSS/native overhead. Under tight byte policies the extra stack can intentionally reject construction; the existing40-bit compact-source test now explicitly allows2048 versus its old1024 to cover this new declared allowance, with unchanged gate/width checks. No experiment acceptance cap was changed.

## Final validation

41 tests across10 affected targets pass, including7 new collision/reorder/cache/work/simultaneous-owner regressions. Strict scoped QSVT lib+same10test-target Clippy passes; scoped rustfmt --check passes. Commands:

```sh
cargo test -p quest-qsvt --test commutative_identities --test lcu_plan --test owned_replay --test portfolio_composition --test portfolio_structured --test structured_stencil --test structured --test portfolio_comparison --test portfolio_resource_curves --test portfolio_review_regressions
cargo clippy -p quest-qsvt --lib --test commutative_identities --test lcu_plan --test owned_replay --test portfolio_composition --test portfolio_structured --test structured_stencil --test structured --test portfolio_comparison --test portfolio_resource_curves --test portfolio_review_regressions --no-deps -- -D warnings
```

Final logs <private-artifacts>/quest-portfolio-identities-focused-freeze.log, <private-artifacts>/quest-portfolio-identities-clippy-freeze.log and <private-artifacts>/quest-portfolio-identities-format-freeze.log. Earlier failed logs are preserved: initial API probe compile typo, nonexistent test-target invocation, initial expected new-stack rejection, temporary wrong-scope shift identifier, helper visibility lint and deliberate clone-test lint; they are not described as green. The quest-polynomial generic_const_exprs/next-solver warning is dependency output, not a scoped Clippy failure.

Public method docs/research/portfolio-source-fingerprints.md links the shared helper contract; generic paths/no private path references checked. Source manifest includes five implementation files, two tests and this method document. Shared SHA engine/export/Cargo and matching/native/IO/v2 tests belong to CFD's separate reviewed18-file snapshot. Root performs independent review/integration. No N32 production, inverse/readout, multihost capacity or new scientific accuracy is claimed.
