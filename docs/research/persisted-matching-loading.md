# Persisted matching loader payload admission

The collective `load_matching` API keeps its file format, return type and portable replay admission. It agrees on partition shape, all raw persistence and replay limits, and an initial payload/work floor before opening the manifest. A small loader budget therefore rejects before incidental file IO. Source-dependent count/work admission and requested allocation checks remain.

Native execution can instead call `load_matching_resource` with `ResourceLoadLimits`, which has independent storage, work and communication limits. The returned `LoadedMatchingResource` verifies the same bucket byte/semantic hashes, coefficients, frozen rotation/phase consistency, cyclic ownership, unique source keys, global count/digest and closed bijective inverse directory. It retains source-owned records and destination-owned inverse entries, with no replicated global coefficient vector. Its consuming `into_prepared_matching` uses the same alias rejection, collective metadata, capacity and whole-unitary native validation as the compatibility path. This resource has no portable gate recipe. Calling `admit_replay` explicitly performs both gate-stream dry runs and enforces the separate replay limits; `load_matching` composes these two operations while preserving its pre-IO limit agreement.

Both returned types expose `load_statistics()`: rank-local wall time for metadata/admission, reading/validation, reverse-directory construction and optional portable admission, plus exact logical directory-call/broadcast counts and actual local record lengths/capacities. `replay_admitted` distinguishes an admitted recipe from an unrequested one. Zero replay counters on the resource-only path report work not performed; they are not a zero-cost admitted recipe. Clones preserve immutable load observations. Timings include collective waiting and exclude caller idle time, later shard cloning and native preparation. Logical broadcast counts exclude MPI-internal checks and are not measured network bytes.

The loader admits the following conservative simultaneous Rust payload envelope, with checked arithmetic:

```text
max_manifest_bytes + max_buffer_bytes
+ records.capacity() * sizeof(ResourceMatchingRecord)
+ reverse.capacity() * sizeof(ReverseRecord)
+ 16 KiB control/stack allowance + 4096 bytes bookkeeping
+ sizeof(Data) + sizeof(LoadedMatching)
+ sizeof(MatchingManifest) + sizeof(MatchingBucketInput)
+ 2 * sizeof(usize)
```

The fixed allowances are models, not measurements of compiler stack or allocator overhead. Both declared IO envelopes remain charged even when some temporary owners have dropped. The requested reverse size is used before that allocation; its actual capacity replaces that prediction immediately after reserve. Record capacity is agreed before bucket IO/fill/sort, and reverse capacity is agreed before the first reverse-directory broadcast. Failed local reserve/admission/IO stages agree before peers advance. Loading does not mutate a quantum register.

Adjacent IO guards reconcile manifest input capacity before resize/read. The bounded Serde visitor charges actual receipt-vector growth and String capacity before push/fill; it caches incremental charges rather than rescanning accumulated receipt owners. Manifest admission retains its existing `3 * input capacity + 8192 + sizeof(MatchingManifest)` scratch/error allowance, plus owned receipt/String payload. Typed chunk capacity is checked before dataset reads; actual returned disk-vector capacity is checked before conversion. The read-buffer envelope is `typed capacity * sizeof(PersistedMatchingRecord) + disk capacity * sizeof(DiskRecord) + 8192`. Existing schema, ASCII/escape, hash, ordering and file-size checks remain.

An allocator or native call can return a larger allocation before its actual capacity is inspectable. That temporary rejected allocation may exceed a modeled cap and is then dropped. These are downstream-work admission checks, not a hard allocator quota. HDF5 handles/caches, metadata discovery and returned header/owner metadata, filesystem/native/MPI internals, allocator overhead and process RSS are outside this Rust payload model. Disabling the raw-data chunk cache does not bound the HDF5 metadata cache. Use separate OS/process limits when those costs matter.

`LoadedMatching::retained_bytes()` still reports conservative returned source accounting; it does not report the transient loader peak. Its replay limits remain independent of loader limits. A caller using a collective runtime reserves the declared loader envelope while loading, alongside existing register/child owners, then accounts the returned owner during [consuming native preparation](persisted-matching-preparation.md). An external reservation does not enforce an allocator quota. Aliases and native preparation have their own admission contract. Publication uses its separate producer/storage admission.

The approved persisted weighted consumer declares a 1 MiB loader cap and a matching 1 MiB external reservation, with 256 KiB manifest, 32 KiB chunk-buffer and 16-record chunk limits. Those declarations must actually admit the inputs; no automatic increase follows rejection. Replay and native preparation retain separate 4 MiB limits. No weighted consumer campaign is established by this loader correction.

Test-only real reserve inflation at one rank checks record/reverse rejection before bucket/routing work, unchanged register/accounting, and continued use of an already prepared child. Additional bounded subprocesses check manifest input, receipt, String, typed-chunk and disk-chunk capacity before downstream fill/conversion. The original persisted replay/restart and bridge matrices retain full unitary tests at 1/2/4/8 ranks and independent split groups. Historical bridge/checkpoint receipts keep their original source identities; this correction is a separate stage.
