# Sparse record fingerprints, version 2

Completed matching columns and canonical producer entries use domain-separated SHA-256 **per record**, truncate each result to a 64-bit word, then add those words modulo 2^64. Record order and communicator partition do not change the result. The completed-column domain is `0x4d434f4c554d4e32`; the sparse-entry domain is `0x5350454e54525932`. The domain and every field are streamed as little-endian u64 words; the first eight SHA output bytes are interpreted in little-endian order. No complete source, record-byte vector, gate stream or global state is created by hashing.

The previous byte-wise FNV construction had an ordinary structured collision: conjugating all imaginary phases of certain small permutations changed individual hashes by balanced positive and negative increments. Their sum was unchanged. This allowed altered columns to pass a header's accidental-integrity check. It was separate from the earlier whole-word FNV sign-bit issue. The maintained four-column regression checks rejection of the altered payload, and the distributed test checks the same family before native mutation.

These remain **64-bit accidental-integrity fingerprints**, not authentication or collision-free proofs. Source and construction IDs are compact compatibility/provenance checks. Persisted file and semantic SHA-256 checks retain their separate full cryptographic digest contract. Correctness of an encoding and mathematical equivalence of two sources require their own evidence.

## Persisted compatibility

New matching manifests require schema version `2` and construction `completed-matching-columns-v2`. Version `1` is rejected rather than interpreted with new fingerprint rules. Historical manifests, HDF5 files and scientific receipts remain unchanged evidence. A new dataset requires an explicit rebuild; there is no implicit migration or compatibility adapter. Record layout and replay angles are unchanged.

## Admission and work

`record_fingerprint` streams caller-supplied words. Its model covers the digest engine, not arbitrary iterator callback work, retained input ownership or allocations. Production consumers supply fixed-size semantic arrays. The fixed 1024-byte scratch allowance models the digest state, padding and software compression schedule; allocator overhead, platform/backend stack frames and process RSS are not claimed to be bounded by this number.

`record_fingerprint_work(n)` charges

```
8192 * ceil((8 + 8*n + 9)/64)
```

logical scalar-operation units. Eight bytes are the domain, and nine bytes are the SHA padding marker and encoded message length. The per-block allowance covers 64 rounds at at most 64 modeled scalar operations, 48 expanded words at at most 32 operations, and 2560 units for input/state/output handling. Hardware acceleration does not lower the deterministic admission charge. This is a conservative algorithmic model, not timing, native instruction counts or measured cycles. Canonical source entries have four words (8192 units); completed columns have seven words (16384 units). Counts, products and aggregates use checked arithmetic.

The distributed producer charges its completed-column summary, canonical source-entry hashes, and the additional complete-shard verification at one rank before those hashes execute. Its retained envelope includes the fixed scratch. Pure shard construction checks scratch plus actual record capacity; its `NumericalPolicy` is a storage policy, not a work budget. Standalone summary/header methods expose fallible representation checks but no caller work ceiling.

A resource replay constructor charges each verification hash before execution. Its existing 4096-byte workspace allowance includes the fixed hash scratch. Persisted loading's preparation stage adds the complete record-summary work; its later resource-replay admission independently charges its own scan. Consuming native conversion includes one native payload-summary pass and, at one rank, the extra complete-shard constructor pass. Its existing 16384-byte control envelope includes the hash scratch. Native matching preparation separately reserves scratch before its collective payload check. Native matching APIs without a work-cap parameter do not acquire an implicit work guarantee.

The loader preparation, resource-replay construction and consuming bridge retain their existing separate stage allowances. Their maxima must not be reported as a single aggregate lifetime work bound. A caller requiring such a bound must add all stages and repeated uses. This correction increases computed work where appropriate; it does not increase user limits, campaign ceilings, or automatically retry rejected work.

## Focused verification

Maintained tests cover opposite phases, conjugation, signed zero, domain separation, an independent SHA test vector, padded-block/overflow accounting, record reorder and partition invariance, and version-1 rejection/version-2 HDF5 roundtrips. Pure source cases include dimensions 2, 4, 8 and 32. Distributed producer, phase-substitution and restart tests exercise 1, 2, 4, 8 ranks and split communicators with small explicit references. This is not a rerun of the persisted weighted-transform scientific campaign.
