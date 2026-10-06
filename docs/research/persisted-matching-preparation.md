# Consuming persisted matching preparation

The `quest-rs` `qsvt-io` + `mpi` endpoint `LoadedMatching::into_prepared_matching` collectively transfers an exclusively owned local persisted snapshot into the existing CPU/MPI `PreparedMatching` owner. It returns `(PreparedMatching, PersistedPreparationResources)`. The child borrows its environment; it retains neither a persisted directory nor a replay recipe. This endpoint provides a safe handoff for the existing [distributed weighted matching](../verification/2026-10-06-distributed-weighted-matching.md) and [prepared LCU transform](prepared-lcu-transform.md) APIs. It does not add another execution backend, tensor implementation or complete matrix representation.

```rust,ignore
let loaded = load_matching(comm, manifest_path, bucket_directory, io_limits)?;
let (child, resources) = loaded.into_prepared_matching(
    environment,
    register_width,
    physical_targets,
    PersistedPreparationLimits {
        max_bytes: rank_envelope,
        capacity: RoutingCapacity {
            ranks_per_node,
            node_budget,
        },
        ..PersistedPreparationLimits::default()
    },
)?;
```

All peers use a common operation order. Load and admit any caller-owned IO/source storage before this call. The bridge does not retroactively admit loading and cannot transfer or retire a caller's reservation. A transient IO/decoder reservation may be released after those temporary owners have dropped; a retained source reservation remains live through handoff. Aliases retained by the caller remain the caller's accounting responsibility after rejection.

## Exclusive ownership and identities

A loaded handle contains `Arc<Data>` and a scalar admitted replay recipe. The recipe contains no `Data` reference. The consuming bridge discards that recipe and uses `Arc::try_unwrap` to establish exclusive payload ownership atomically. A second strong loaded handle on any rank rejects collectively before the column clone or native preparation. Cloning only the scalar recipe does not create a payload alias. Existing `Clone`, local `matching_shard` and replay APIs remain available with their existing caller orchestration contracts.

The bridge validates relative rank/partition ownership and compares a fixed versioned frame containing the complete ten-word matching header, all 32 canonical manifest SHA-256 bytes, register width, ordered targets, every preparation limit, placement and the current numerical policy's `max_bytes`. It does not fabricate an original communicator identity. A compatible loaded partition can be used with another communicator having the same rank ownership; native digest and permutation-closure checks still run.

The receipt separates the persisted manifest SHA-256 from the native descriptor's source and construction identities. A file integrity identity, represented operator identity and complete unitary construction identity have different meanings. The prepared child keeps the existing descriptor and whole-unitary behavior, including failure and padded sectors, signed outer controls and adjoints.

## Resource contract

`PersistedPreparationLimits` defaults to 1,048,576 local records, 256 MiB whole-live rank storage, 1,073,741,824 modeled constructor work units, 1 GiB application communication payload and the existing 64 MiB numerical policy. Default `ranks_per_node=1` and an unrestricted supplied node budget are caller premises, not detected placement or a measured node capacity.

`max_bytes` admits the environment's live owners plus source, reverse directory, copied snapshot, target capacity, 16 KiB control/stack allowance and future native preparation overlap. The numerical policy separately admits source plus copied snapshot. Record, reverse, column and target Vec capacities are accessible payload measurements; conservative scalar/native allowances remain modeled. The input register, earlier children and external reservations stay in the environment ledger. Temporary duplicate charging of the snapshot and targets during native owner establishment is deliberate. The source/reverse payload and bridge guard drop only after the native child has established its own reservations.

Before cloning, both requested overlap and rank/node maxima are checked. Immediately after reserve, actual column capacity is reconciled **before filling**. The native preparation entry also reconciles the actual incoming validation Vec capacity and re-admits rank/node overlap before filling or its permutation protocol. Its later routing/scratch boundaries retain their existing actual-capacity admission. A derived native node ceiling carries the bridge's rank cap into those later native checks; it can conservatively reject more tightly than a standalone native call. This is not a promise that allocator metadata, native library/MPI overhead, process configuration or RSS is bounded by the payload ledger.

The receipt reports `loaded_source_bytes`, actual-capacity `snapshot_bytes` and `target_bytes`, the stack allowance, local record count and identities. `planned_rank_peak_bytes` is the maximum planned source/clone/native-scratch envelope admitted at bridge boundaries. `rank_peak_bytes` and `node_peak_bytes` are conservative admitted whole-stage **caps**, not measured high-water marks. `native_preparation_scratch_bytes` is the shared requested-capacity prediction; actual native capacities can be larger and must still fit the caps. The receipt exists only after success.

For common record count R, partition count P, register width W and h=ceil(log2(R+1)), the checked rank-local constructor envelope is:

```text
256 * (R+1) * (P+h+W+16) + 512*P + 4096
```

It bounds the local copy/validation/sort/search and the existing P×R native permutation/coordinator loops in modeled scalar/protocol work units. Cheap count, shape, arithmetic and work rejections precede new allocation or P-sized reductions. It is not instruction timing, native state-initialization work or a gate count.

The separate checked aggregate application payload ceiling is:

```text
(P-1) * (24*R + 128*P + 16384)
```

The 24R(P−1) term covers native permutation frames, with a conservative additional allowance for comparisons, summaries and node/control broadcasts. Allreduce internals, coordinator traffic, MPI protocol overhead and native QuEST initialization wire are excluded. This is not measured traffic.

## Failures and focused evidence

Rank-local conversion/allocation/model failures and conversion panics are caught and agreed before peers enter the next collective phase. The parent lane is released before the existing native preparation acquires its own lane. Ordinary admission failures leave any existing register unchanged and return no success receipt; guards drop on rejection. Native MPI fatal failures preserve the existing bounded job-abort contract and are not recoverable errors or a rollback mechanism.

Focused tests cover the consuming handoff at 1/2/4/8 ranks and independent split groups, with a bounded 128-amplitude arbitrary-state reference for complete controlled U and standalone U†. The fixture has a rectangular complex source, reordered targets, spectator/control bits and failure/padding sectors. The cold serial matrix/state references exist only in tests. The same valid source/operator/layout is loaded again while the first child remains prepared. A rank-local later conversion-budget rejection preserves the entire live ledger and input; subsequent first-child controlled U and standalone U† checks prove it remains usable. Separate bounded fault jobs cover rank-local aliases, source/header/manifest/policy/layout disagreements, too-small live/work/record limits, excess target Vec capacity, arithmetic overflow and a caught conversion panic. A real test-only 128 KiB native incoming-vector reserve proves late capacity rejection and unchanged register/accounting; the injection has no production API.

Using matching MPI/native installations on `PATH`:

```sh
cargo test -p quest-rs --features qsvt-io,mpi --test matching_persisted_preparation
cargo test -p quest-rs --features qsvt-io,mpi --lib qsvt::persisted_matching::preparation::failure_tests -- --test-threads=1
cargo test -p quest-rs --features qsvt-io,mpi --lib qsvt::matching::collective::preparation_tests -- --test-threads=1
cargo clippy -p quest-rs --features qsvt-io,mpi --lib --tests --no-deps -- -D warnings
```

This is bridge evidence. A fixed immutable publication/restart→three-source weighted LCU→reciprocal transform/readout campaign requires its own protocol and evidence. Multihost deployment, genuinely large transport counts and distributed tensors remain separate requirements.
