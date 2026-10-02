# Independent benchmark fixture review

Reviewed the current `mp-backends`, `mp-solvers`, and `static-architecture/project` fixtures by source inspection, including the archived Astro/Rug primitive source and frozen pre-port API definitions. No builds or timing runs were performed by this reviewer.

## Resolved findings

1. **Endpoint API mismatch:** the Newton fixture previously compared interval-valued `lower()`/`upper()` results with scalar values. `mp-solvers/main.rs` now uses `lower_endpoint()`/`upper_endpoint()`, which return the required native endpoint type in both the baseline and current APIs.
2. **Incomplete mutation fingerprints:** `mp-solvers/run.py` now includes transitive `quest-language` and `quest-symbolic` Rust/manifests alongside the five direct numerical crates, so their changes invalidate the comparison.
3. **Archive Git identity:** `static-architecture/project/run.py` now checks that `git rev-parse --show-toplevel` equals the supplied repository directory before recording HEAD. Extracted archives and nested snapshots instead receive an explicit archive identity tied to the frozen source hashes. They neither abort for absent Git metadata nor inherit the surrounding checkout's HEAD as their own identity.

All three corrections were re-read in the latest source. No remaining fixture/API blocker was identified by inspection; compilation and successful runs remain coordinator validation responsibilities.

## Measurement interpretation

- Deterministic primitive bit strings, input pairing, operation counts, directed endpoint pairs, matrix multiplication, and modified Gram-Schmidt match the archived fixture. Historical ratios compare nominal precision and were measured in different runs. The Dashu primitive fixture does not independently establish matched QR/matrix residual accuracy; its README now states that limit.
- Complete Remez and offline QSP requests use the same source and acceptance bounds across backends and check the resulting numerical certificates. Newton checks its split result, and exact synthesis checks reconstructed equality. This supports matched accepted outcomes rather than identical intermediate rounded values.
- Solver timing includes correctness assertions and exact reconstruction inside each iteration. Both implementations use the same allocation counter, whose atomic operations add allocation-dependent timing overhead. These are instrumented timings, not estimates with that overhead removed.
- Allocation counts are totals over the printed iteration count. Peak additional live bytes are measured above a warmed baseline; they exclude allocator metadata, stack storage, internal realloc overlap, and native allocations. Process RSS is a separate whole-process metric. The README now records these distinctions.
- The exact-synthesis precision label changes a resource limit; the 128/256 settings execute the same exact integer workload. They do not measure floating-point precision scaling.
- Before/after source hashes, input/resolved locks, executable hashes, and toolchain receipts provide reproducibility evidence. Successful receipts still require stable sources and an exclusive local measurement window. None establishes Linux, MPI, accelerator, or cross-machine performance.

Verdict: the inspected fixtures are suitable for their documented comparisons after the three fixes, subject to the coordinator's builds and actual result checks. No fixture or production source was modified by this review.
