# Persisted matching load phases

This follow-up measures the work inside `load_matching` and separates immutable resource loading from portable gate-replay admission. The [machine-readable summary](data/2026-10-06-matching-load-phases/summary.json) records source identities, exact logical counters, repetitions and hashes of private raw receipts. The Cray baseline and paired comparisons completed on two, four and eight nodes; the separate GNU paired comparison completed on two nodes.

## Measured baseline

The input is the same immutable eight-bucket resource at N=65,536, with 131,072 records, used by the earlier Cray capacity experiment. Each job uses exclusive nodes, one MPI rank per node and 288 requested OpenMP threads. The reload-only probe does not execute native QuEST kernels. Three independent launches reload the same files; input hashes and the frozen executable are checked before and after. The table reports the median of three per-repetition rank maxima, in seconds.

| Nodes / ranks | Read and validate | Reverse directory | Portable admission | Total load | Observed admission broadcasts / rank |
|---:|---:|---:|---:|---:|---:|
| 2 | 0.124 | 1.084 | 22.345 | 23.554 | 3,014,676 |
| 4 | 0.071 | 1.870 | 59.132 | 61.079 | 4,325,416 |
| 8 | 0.048 | 3.060 | 159.814 | 162.990 | 6,946,896 |

Portable admission accounts for about 95–98% of these measured loads. This establishes the dominant phase on these cases. It does not separately profile MPI latency, packet transport, local lookup work or filesystem effects within a phase.

Build 537278 used the admitted Cray profile and unchanged native installation, with an explicitly recorded Rust `opt-level=2`, `debug=0`, nonincremental example build. It starts from source `8d85c0e…`, adding only three instrumented loader files, the reload example and the producer retained-payload accessor needed by publication to compile. Every overlay file is hashed. This optimized diagnostic profile differs from the earlier capacity executable; comparing its totals with those historical launcher/load timings is not a controlled speedup measurement.

Jobs 537315, 537316 and 537317 completed the accepted two-, four- and eight-node repetitions. Job 537317 completed in 8 minutes 27 seconds. Its walltime and the later paired eight-node job 537402 were reduced from 60 to 20 minutes while retaining low-priority QoS, eight exclusive nodes and one rank per node. Original and updated scheduler records are retained. The earlier 537279/537280/537281 recipes each completed their first reload but omitted the rank-zero, nonce-tagged Slurm step announcement required by the supervisor. Their coordinators rejected success. Those failed recipes and raw diagnostics remain evidence, and their timings are excluded from the table.

## Why the counters grow

The inverse-directory construction performs one sender-count broadcast per rank and one owner broadcast per global record: M+P checked broadcasts per rank. It retains only source-owned records and destination-owned inverse entries. The balanced fixture has 2N/P entries in each local vector; general inputs can be skewed and must be described as O(local records), not universally O(N/P).

Portable admission traverses the records five times: an integrity scan, then corrections and permutations in each gate-stream orientation. Each ordered-next lookup broadcasts one candidate from every rank, and successful lookups fetch a record by another broadcast. The constant-workspace permutation routine walks a complete cycle from every pivot to find its minimum before emitting that cycle once. If S is the sum of squared cycle lengths, its cycle work is O(S), with a quadratic worst case for long cycles.

For this fixture M=131,072, C=2 and S=196,608. Observed per-rank admission counts are 655,370 ordered-next calls, 1,179,648 forward lookups and 524,288 reverse lookups. The corresponding checked broadcast count is:

```text
5P(M + C) + 7M + 4S
```

The counter is incremented at the actual directory call sites. A next-record lookup's nested forward fetch is included in the forward count. These are logical calls, not measured wire bytes. Checked MPI broadcasts also perform metadata agreement internally; this instrumentation does not remove those safety checks or instrument the global MPI implementation.

## Resource-only native execution

`load_matching_resource` returns `LoadedMatchingResource` with independently bounded loading work. It retains the same local coefficient/frozen-angle validation, file and semantic hashes, cyclic ownership, duplicate-key rejection, global count/digest and closed bijective reverse directory. Consuming native preparation retains collective metadata, alias ownership, whole-live capacity and whole-unitary validation.

Portable gate admission is explicit through `admit_replay`. The compatibility `load_matching` entry point performs both operations and still agrees all original persistence/replay limits before file I/O. A resource-only result has `replay_admitted=false`, with zero observed replay work; it does not claim an admitted zero-cost gate recipe. See the [loader contract](../research/persisted-matching-loading.md).

On the new source `8e02c49a…`, focused local tests passed for both loaders, malformed/missing resources, immutable alias observations, replay-work rejection, native alias rejection and controlled forward/adjoint action across all tested register sectors. Five focused library tests also passed, including pre-I/O limit rejection and native handoff failures, and focused strict Clippy passed. Independent read-only review found no defect.

A local two-rank paired probe at N=65,536, with two OpenMP threads, measured 10.472 seconds for compatibility loading and 0.889 seconds for resource-only loading. Both consuming native paths produced identical forward-state hashes and checked every local coordinate after U followed by U†; maximum roundtrip error was 6.51e-19 and norm was 0.9999999999999999. This is a local comparison, not a Cirrus result.

Cray build 537399 and paired jobs 537400/537401/537402 completed with one frozen executable and optimized profile for both loading modes, followed by identical native execution. GNU build 537408 and paired job 537409 repeated the two-node comparison using the corresponding GNU profile. Three pairs per node count alternate mode order and reload the same immutable input. The table reports medians of per-repetition rank maxima, in seconds.

| Compiler | Nodes / ranks | Compatibility load | Resource-only load | Resource-only read | Resource-only reverse directory |
|---|---:|---:|---:|---:|---:|
| Cray | 2 | 23.590 | 1.222 | 0.122 | 1.097 |
| Cray | 4 | 59.105 | 1.918 | 0.077 | 1.844 |
| Cray | 8 | 163.712 | 3.158 | 0.043 | 3.091 |
| GNU | 2 | 23.020 | 1.179 | 0.117 | 1.059 |

Every resource-only run recorded zero replay-admission calls and time, with replay explicitly unadmitted. Both routes retained 131,072/P forward records and the same number of reverse entries per rank; their reverse-directory broadcast counts were identical. The remaining resource-only load is dominated by reverse-directory construction on these cases. This phase result does not identify a lower-level MPI or filesystem cause.

All twelve pairs produced bitwise-equal forward-state hashes per rank. Each run checked all 4N/P local coordinates after U followed by U†; maximum error was 6.51e-19 and norm was 0.9999999999999999. Native multithreading was enabled with 288 requested OpenMP threads. The paired probe uses a plus-state input; arbitrary-state and controlled whole-unitary equivalence are covered by the separate local tests, rather than inferred from this one numerical input.

These remote runs use frozen source `8e02c49a…`, which predates native communication-buffer reuse. They validate the resource-only loader split and its then-current two-register native path, not the later buffer-reuse implementation. Both compiler builds freeze their example binaries with hashes. The companion v4 capacity runs reuse these builds and are reported separately in the [capacity follow-up](2026-10-06-sparse-capacity-followup.md).

## Reproduction

Build `matching_load_probe` with matching MPI and installed QuEST, then launch it through the existing bounded MPI supervisor. Its arguments are an immutable input directory containing `manifest.json`, a fresh output directory, a Rust payload budget in bytes and a query/work/wire/gate limit. Omitting the final mode performs compatibility loading only. Supplying `legacy` or `resource` additionally consumes the result into native execution, checks all local roundtrip coordinates and records a forward-state hash for pairwise comparison.

```sh
cargo build --locked --offline -p quest-rs --all-features --example matching_load_probe
mpiexec -n 2 target/debug/examples/matching_load_probe \
  /shared/immutable-matching /shared/results/legacy 268435456 68719476736 legacy
mpiexec -n 2 target/debug/examples/matching_load_probe \
  /shared/immutable-matching /shared/results/resource 268435456 68719476736 resource
```

The displayed commands assume fresh output directories and a launcher deadline supplied by the surrounding supervisor. On Cirrus, run one rank per exclusive node, bind physical cores and use a rank wrapper that announces the owned Slurm step before native entry. Per-rank JSON contains active load timings and exact call counters; caller idle time and later native preparation are outside the load timings. The payload budget excludes native/MPI/HDF5 internals and process overhead and is not a whole-node memory cap.
