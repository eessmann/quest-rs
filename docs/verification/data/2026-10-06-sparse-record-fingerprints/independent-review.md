# Matching fingerprint v2 independent review

Approved within the frozen matching scope; no remaining actionable finding. This is not full workspace or N32 scientific acceptance. The four separately owned portfolio families are outside this verdict. No production source, QuEST checkout or historical campaign artifact was changed by this reviewer. Public candidate preparation ran no tests or scientific jobs.

Final binding: `source.json`, SHA256 `5ff27a818e1aa6e32e2a2f53690bccea5fdcf44036fd44c792c0186319abf41d`. All18 entries match before/after final checks. Initial manifest54023f8c remains unchanged. The final delta touches only matching_digest.rs test arithmetic/style/justified lint allowance and preprocess.rs cfg(test) lint allowance. Removing that exact preprocess allowance reproduces its initial cde58495 hash; production preprocessing is byte-identical. The changed digest tests were rerun after the delta. Source and log hashes are in `independent-focused.json`.

## Semantic and ownership audit

The old four-column public Rust probe and maintained phase/signed-zero failures establish the actual accidental-integrity defect: balanced phase changes cancel after summing byte-wise FNV records. The correction uses fixed-field, domain-separated SHA256 per record before wrapping addition. Source records use row/column/re/im bits and exclude post-canonicalization ordinals; completed columns use color/source/destination/cosine/sine/phase bits. The distinct domains and canonical LE framing agree with an independent Python hashlib/struct oracle. At N4/8/32 the opposite-i column digests differ; all three analytic source families differ at N2/4/8/32. This reference calculation does not invoke distributed source construction, synthesis or quantum execution.

Partition/reorder independence remains valid because only completed record fingerprints are added modulo2^64. Cardinality is checked separately. This remains64-bit accidental provenance rather than authentication or collision-free operator identity. Persisted full file/semantic SHA256 remains separate. Native collective payload checking catches altered balanced phases before source/state allocation; global permutation closure checks are retained. All matching rotation, padding, control, gate and routing formulas remain unchanged.

Persisted schema2/construction completed-matching-columns-v2 explicitly rejects version1 before bucket reads. Bucket numeric layout and replay angles remain unchanged; old files/manifests/campaign receipts are immutable historical evidence, requiring explicit fresh publication for a future v2 run. There is no silent migration. Older consumer compiler-closure/lock snapshots do not attest this intentionally changed source tree; a new stable build pin is required before any scientific campaign.

## Resource audit

The helper charges8192*ceil((8+8*n+9)/64): source4words=8192, column7words=16384, with checked byte/block/work arithmetic. Fixed SHA input arrays and pinned SHA state introduce no record-sized vector or new heap collection. The1024-byte engine allowance is an explicit payload model; arbitrary iterator callbacks, input ownership, platform/backend stack and RSS are excluded as documented.

Producer admission covers summary plus P1 from_parts rehash and canonical source hashes before hashing, with1024 scratch in the retained model. Resource scan charges each record hash before execution, within its4096 workspace envelope. Restart preparation covers reduce_summary; subsequent AdmittedMatchingReplay admission separately charges its verification scan. Bridge covers native payload-summary plus P1 clone verification and retains its16KiB stack envelope. Native preparation admits and reserves1024 scratch before hashing; its later incoming-capacity reconciliation releases that scratch only after hashing returns. Actual-capacity/source overlap guards remain present. These separate work ceilings are not an aggregate lifetime bound; native/pure APIs lacking work limits do not gain an implicit guarantee.

## Independent checks

Commands executed under QUEST_ROOT and matching MPICC/PATH:

- cargo test -p quest-qsvt --test matching_digest --test matching_shards --test matching_resource --locked:10 passed.
- cargo test -p quest-qsvt-io --test sharded_matching --locked:11 passed, including v1 rejection and v2 roundtrip.
- cargo test -p quest-rs --features qsvt-io,mpi --test matching_preprocess --test matching_collective --test matching_persisted --test matching_persisted_preparation --locked -- --nocapture --test-threads=1: four parents passed. Whole-U15.51s; restart8.14s; bridge3.01s; producer3.57s. MPI1/2/4/8/split passed; restart4→2/8 passed and unsupported3 rejected. This includes conjugate source distinction/reversal identity, phase substitution rejection, whole-U/U† controls and retained-prior-child bridge rejection.
- Final changed matching_digest3 and private producer source1 passed. The latter computes fixed digests only and does not initialize MPI or execute the N32 operator.
- Selected strict --no-deps Clippy passed for quest-qsvt lib+three tests, quest-qsvt-io lib+sharded_matching, and quest-rs native lib+four affected tests. The first independent pure-Clippy attempt failed the three assertion tests' panic_in_result_fn; it is preserved and followed by the justified test-only correction and green final check. The standard upstream nightly generic-const trait-solver warning remains separate.

No broad tests, production N32 source/synthesis/replay, campaign retry or native QuEST change was performed. The final18-file scope stayed unchanged during final binding checks; separately moving portfolio cleanup is not represented as a whole-tree freeze.
