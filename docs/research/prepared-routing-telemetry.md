# Routing telemetry for prepared matching composition

Collective prepared matching LCU and LCU transform owners expose `last_apply_telemetry() -> Option<RoutingTelemetry>`. The fixed receipt describes application matching routing across every successful child event in one complete apply. It complements the existing modeled `MatchingLcuResources` and `TransformExecutionResources` returned by `apply`; those remain admission bounds, not measurements.

```rust,ignore
let bounds = prepared.apply(&mut register, false, mask, value)?;
let actual = prepared.last_apply_telemetry()
    .ok_or("apply did not produce a completed routing receipt")?;
let sent_lower_bound = actual.routing.point_to_point_sent_bytes;
let all_included_counters_exact = actual.exact;
```

The type is `quest::qsvt::matching_lcu::telemetry::RoutingTelemetry`, available with `qsvt` and native `mpi`. CPU-only prepared owners have no routing measurement getter. Shared execution work models conservatively include the rollup allowance on CPU too.

## A receipt belongs to the current apply attempt

Construction starts with `None`. Every `apply` clears the old receipt before whole-operation admission. A recoverable admission rejection therefore leaves `None` and preserves the input state; it cannot present a preceding success as the current result. Only complete execution installs `Some`. Errors or panics after emission retain the MPI job-abort contract and install no success receipt. The underlying unitary, controls, adjoint and lane ordering are unchanged.

Standalone immutable `admit_apply` performs admission only. It neither executes nor resets telemetry, even if that standalone admission rejects. Callers that save a copied old receipt own its historical interpretation; the getter does not certify an independently executed call or another register's current state.

## Fields and exactness

`child_events` counts selected matching child applies actually dispatched. A retained zero-weight child is still admitted and charged but contributes no event. A direct LCU receipt has `source_queries=0`; a transform counts each actual source apply separately, summing that source's complete child-event routing receipt immediately after success. A degree-zero transform can execute response/projector primitives while reporting exact zero source and matching-routing counters.

`routing` contains the existing `RoutingStatistics` fields. Batch, local candidate, coordination, indexed read/write and directional application-byte fields are summed. `maximum_batch_pairs` and `maximum_routed_amplitudes` are maxima across events, rather than sums. These are local-rank facts; checked global reductions and their resources are a separate consumer responsibility.

`exact=true` says every included counter is exact under the inherited instrumentation model. Original routing counters saturate without a flag. Any inherited field equal to `usize::MAX`, any aggregate reaching that limit, or checked aggregate overflow sets `exact=false`. Saturation retains the maximum as a conservative lower bound. When false, all reported numbers may safely be treated as lower bounds, although some remain exact; the receipt does not invent zeros or claim exact huge-count transport. This is metadata arithmetic handling, not demonstrated physical capacity at that count.

The measured scope is matching application routing, including its documented count frames. PREP, response/projector, native internal, constructor, separate coordinator and MPI protocol traffic are excluded. A router coordination counter is not the total number of native or outer collective calls. An excluded quantity is unknown here, even when all included counters are zero. Matching sends/receives are directional payload facts, not total network wire. Process RSS, allocator metadata and native library/MPI overhead require separate observations.

## Bounded accounting and validation

Rollup uses a fixed Copy receipt and scalar arithmetic, with no new allocation or communication. Existing constructor reservations include the changed owner `size_of`, so both retained Option fields are charged before apply; existing stack/scratch allowances cover the local accumulator overlap. Execution preflight adds 128 modeled arithmetic units per selected child at the LCU layer and 128 per source query at the transform layer. Aggregate work remains derived with checked rank counts. These conservative units are neither measured instructions nor native gate/protocol costs. Existing resource caps can consequently reject a call that previously fit exactly.

The new bounded fixture uses genuinely origin-sharded four-column sources, three surviving weights and one retained zero-weight owner. At 1/2/4/8 ranks and independent split groups, it compares LCU totals with independently dispatched native children and transform totals with separately executed source queries. It checks both adjoints and signed control values, actual live-budget rejection reset, unchanged state/accounting, and exact no-query counters. Separate unit tests cover every inherited limit field, aggregate overflow/at-limit handling and the admitted arithmetic allowance. Existing arbitrary whole-U/U† and fatal-boundary regressions are reused; this bookkeeping does not replace their semantic evidence.

Using matching native QuEST and MPICH installations:

```sh
cargo test -p quest-rs --features qsvt,mpi --test routing_telemetry
cargo test -p quest-rs --features qsvt,mpi --lib qsvt::matching_lcu -- --test-threads=1
cargo test -p quest-rs --features qsvt,mpi --test matching_lcu --test matching_lcu_collective --test matching_lcu_transform --test matching_lcu_transform_collective
cargo clippy -p quest-rs --features qsvt-io,mpi --lib --tests --no-deps -- -D warnings
```

The [persisted preparation bridge](persisted-matching-preparation.md) and [prepared transform](prepared-lcu-transform.md) can use these receipts in an independently admitted integrated consumer. No same-file persisted inverse campaign, multihost capacity, physical error certificate or total MPI/native communication claim follows from this telemetry stage.
