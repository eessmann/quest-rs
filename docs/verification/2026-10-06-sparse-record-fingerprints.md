# Matching record fingerprints, version 2

Focused matching verification and independent review passed on 2026-10-06. The [method](../research/sparse-record-fingerprints.md) defines domain-separated SHA-256 per semantic record, truncated to 64 bits before commutative addition. This corrects systematic cancellation of ordinary phase signs in the prior byte-wise FNV sums. It remains accidental-integrity provenance, not authentication or a collision-free operator certificate. Native QuEST, matching routing and the numerical gate/layout formulas are unchanged.

The [18-file final source manifest](data/2026-10-06-sparse-record-fingerprints/source.json) has SHA-256 `5ff27a818e1aa6e32e2a2f53690bccea5fdcf44036fd44c792c0186319abf41d`. All 18 entries matched final before/after checks. The [independent review](data/2026-10-06-sparse-record-fingerprints/independent-review.md), [independent focused receipt](data/2026-10-06-sparse-record-fingerprints/independent-focused.json) and [owner focused receipt](data/2026-10-06-sparse-record-fingerprints/owner-focused.json) preserve separate scopes. This manifest includes Cargo.lock and the shared helper but excludes a complete compiler/dependency closure; it does not attest an executable build. Four separately corrected portfolio families are outside this matching verdict.

## Failure and compatibility evidence

The [historical diagnosis](data/2026-10-06-sparse-record-fingerprints/diagnosis.md) retains exact formulas, old hashes, a bounded four-column public API source/log and the genuine version 1 rejection-test failure. These are behavioral failures. The owner receipt also preserves the misnamed `quest-sparse-digest-resource-final-green.log` as a compile failure: it contains E0425 for then-moving structured source, and executed zero tests. The initial independent pure-Clippy assertion diagnostics are retained separately from the green corrected check. Artifact [provenance](data/2026-10-06-sparse-record-fingerprints/provenance.json) records original and path-normalized hashes; historical originals remain unchanged.

New persisted manifests require schema 2 and construction `completed-matching-columns-v2`. Readers reject v1 rather than silently reinterpreting its record digests. Old datasets and campaign receipts remain historical evidence; a future v2 dataset requires explicit fresh publication. Bucket numeric layout, replay angles and persisted full file/semantic SHA-256 checks remain separate and unchanged.

## Admission scope

The shared helper charges 8192 modeled scalar units per padded SHA block: four source fields cost 8192; seven column fields cost 16384. Checked work arithmetic includes domain/padding. The fixed 1024-byte digest-engine scratch is a model of payload state/schedule, not measured compiler stack or RSS; arbitrary iterator callbacks, input owners and native/platform overhead remain separate.

Producer costs include summary, canonical entry hashes and the one-rank complete-shard recheck. Replay scans charge each record. Restart preparation, later resource-replay admission and consuming native conversion each retain separate stage ceilings; their maxima are not one aggregate lifetime work bound. Native matching preparation reserves the added hash scratch before payload checking. Existing actual-capacity and simultaneous-owner checks remain. No campaign limit was raised, and APIs without work-cap parameters do not gain an implicit work guarantee.

## Executed focused checks

Independent pure checks passed 10 tests and IO checks passed 11. Four small native MPI parents passed: whole-unitary 15.51 seconds, restart 8.14 seconds, consuming bridge 3.01 seconds and producer 3.57 seconds. They cover 1/2/4/8 ranks and split groups, repartition 4→2/8, unsupported 3 rejection, balanced-phase substitution rejection before native allocation, source conjugation/reorder identities and whole-U/standaloneU† with controls. Final changed test-only paths passed 3 digest tests plus 1 private source test; scoped strict pure/IO/native Clippy passed. The owner separately passed 17 pure tests and its focused matrices; receipts retain those scopes without double-counting them as independent.

With installed native QuEST and matching MPI configured:

```sh
cargo test -p quest-qsvt --test matching_digest --test matching_shards --test matching_resource --locked
cargo test -p quest-qsvt-io --test sharded_matching --locked
cargo test -p quest-rs --features qsvt-io,mpi --test matching_preprocess --test matching_collective --test matching_persisted --test matching_persisted_preparation --locked -- --nocapture --test-threads=1
```

The pure dimension 32 identity checks do not construct, synthesize or execute the N32 scientific inverse. No production N32 campaign or retry was performed by this correction/review. A new stable executable source pin is required before such work. Whole-workspace acceptance, integrated inverse accuracy and multi-host capacity remain outside these focused checks.
