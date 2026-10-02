# Whole-project matched performance comparison

Final runner exited **0**. The exclusive measurement window is complete and released. All 18 trial CSVs are present; `frozen-before.sha256` and `frozen-after.sha256` are byte-identical. This report uses only the fresh `performance/project/final-portable-v2` receipts. The earlier `final-portable` candidate is retained as failed fixture preparation evidence; its timings are excluded.

The current uncommitted worktree and immutable baseline `001a2b656a5a80a60659a408f87f57670309a09b` were measured in the same devenv on arm64 Darwin (<host>), Rust 1.100.0-nightly `6bb1652a0`, QuEST 4.3.0. Both standalone generated packages use release thin LTO and two Cargo jobs. The portable checked-in fixture is `docs/verification/fixtures/static-architecture/project/`; it preserves the canonical final-consumer native path policy and requires successful runtime-path configuration. Full resolved scratch Cargo locks and repository/fixture/input hashes are retained with receipts.

## Runtime and allocation tradeoffs

Each runtime cell is the median **microseconds per iteration**, followed by the minimum/maximum across three interleaved trials. Baseline/current trial order reverses in the second pair. Percent change compares medians; three trials establish observed ranges rather than statistical significance.

| Workload | Baseline µs [min, max] | Current µs [min, max] | Current change |
|---|---:|---:|---:|
| QSP completion + inverse NLFT, degree 256 | 1,078.875 [1,050.014, 1,088.424] | 1,049.941 [1,037.455, 1,052.549] | -2.68% |
| QSP completion + inverse NLFT, degree 1024 | 5,012.729 [4,962.594, 5,124.635] | 5,053.146 [5,025.417, 5,065.708] | +0.81% |
| Compiler construction/admission | 3.527 [3.483, 3.541] | 3.442 [3.397, 3.581] | -2.41% |
| Compiler verify/lower/plan | 5.417 [5.307, 5.548] | 5.531 [5.316, 6.781] | +2.10% |
| Compiler finite insertion + pipeline, 256 ops | 1,206.294 [1,202.662, 1,215.868] | 1,159.533 [1,110.705, 1,194.846] | -3.88% |
| Native matrix/oracle preparation | 99.917 [98.890, 101.566] | 78.426 [77.939, 78.852] | -21.51% |
| Native zeroed warm execution | 1,093.745 [1,088.331, 1,097.105] | 1,097.685 [1,084.630, 1,112.420] | +0.36% |

Successful Rust allocations/reallocations per iteration and peak additional live Rust bytes above the warmed baseline were identical across all three trials for each checkout. Peak bytes are **not** divided by iteration count. Native C++ allocations are outside this allocator.

| Workload | Allocations baseline → current | Peak extra Rust bytes baseline → current |
|---|---:|---:|
| QSP completion + inverse NLFT, degree 256 | 8,731 → 8,731 | 248,856 → 248,856 |
| QSP completion + inverse NLFT, degree 1024 | 34,141 → 34,141 | 1,002,664 → 1,002,664 |
| Compiler construction/admission | 157 → 157 | 19,147 → 19,147 |
| Compiler verify/lower/plan | 180 → 180 | 16,107 → 16,107 |
| Compiler finite insertion + pipeline, 256 ops | 17,899 → 17,433 | 686,326 → 597,930 |
| Native matrix/oracle preparation | 1,844 → 1,837 | 213,456 → 210,183 |
| Native zeroed warm execution | 4,704 → 4,704 | 121,312 → 121,312 |

Native preparation improves by 21.51% in this fixture, with seven fewer Rust allocations per preparation. Its admitted prepared-environment resource model decreases from **922,989 to 723,118 bytes** (−21.66%); this is distinct from measured Rust heap peaks and excludes the register state vector. The two signed matrix profiles and shared matrix/oracle aliases retain identical scientific behavior. Execution timing ranges overlap and execution allocation/peak profiles are unchanged. This consolidation does not introduce a new native execution algorithm.

Finite insertion/pipeline improves by 3.88%, removes 466 allocations per iteration and lowers peak extra Rust storage by 88,396 bytes (12.88%). Construction/admission and ordinary verify/lower/plan retain their allocation and storage profiles. Degree-256 QSP has a 2.68% lower median; degree-1024 has a 0.81% higher median, with overlapping ranges. Both QSP allocation/storage profiles are unchanged.

## Build cost and executable size

These are single matched build observations in separate initially empty Cargo target directories, not repeated estimates or a claim of cold filesystem/OS caches. The unchanged invocation actually recompiles `quest-sys`, `quest-rs` and the fixture in both checkouts, as its log records; it is neither a no-op nor a pure caller rebuild timing. The native build scripts watch bridge-generated sources/include directories and recursively collected CMake/native inputs; those watch sets can include build-owned paths, but the exact Cargo invalidation cause was not captured. This existing rebuild behavior is recorded separately and does not establish a new regression or a native incremental improvement. The touched-source invocation updates only generated `main.rs` timestamps, with identical contents. Cargo release incremental compilation is not explicitly enabled.

| Build invocation | Baseline wall s | Current wall s | Change | Baseline maximum RSS bytes | Current maximum RSS bytes |
|---|---:|---:|---:|---:|---:|
| cold | 131.96 | 127.52 | -3.36% | 974,782,464 | 945,979,392 |
| unchanged | 30.27 | 29.27 | -3.30% | 367,034,368 | 364,478,464 |
| touched-source | 30.95 | 29.47 | -4.78% | 366,182,400 | 361,496,576 |

The combined fixture executable grows from **4,233,952 to 4,353,792 bytes** (+119,840; +2.83%). Mach-O `__text` grows from 2,626,212 to 2,720,164 bytes (+3.58%). This is one linked executable containing all three project workloads, not the size of a single crate or a stripped-distribution comparison. Executable hashes and section breakdowns are retained.

## Whole-process storage

Each cell reports median maximum resident set size in bytes [minimum, maximum] across the three process trials. It includes loaded code, Rust storage and native allocations, and is not an isolated native heap measurement.

| Process workload | Baseline maximum RSS bytes | Current maximum RSS bytes |
|---|---:|---:|
| qsp | 6,619,136 [6,602,752, 6,668,288] | 6,799,360 [6,733,824, 6,897,664] |
| compiler | 8,257,536 [8,241,152, 8,257,536] | 7,847,936 [7,798,784, 7,929,856] |
| native | 16,695,296 [16,662,528, 16,760,832] | 16,367,616 [16,220,160, 16,433,152] |

## Mathematical witnesses and scope

- QSP uses identical periodic complex Laurent coefficients and power-of-two scaling at degrees 256/1024, strict ℓ₁ contractivity, tolerance `1e-11`, margin `1e-12`, and inverse NLFT divide-and-conquer. Before timing, four unit-circle direct coefficient sums independently agree with the reconstructed upper-left response within `2e-10`. Both checkouts record 257/1025 source and control counts, grids 2048/8192, completion residuals `2.89907934808216977e-15`/`5.57306207879878839e-15`, and reconstruction residuals `8.86031314223782743e-16`/`2.44428537577268106e-14`. Diagnostics are not rigorous independent certificates.
- Compiler witnesses retain four-qubit width, 64 structured loop iterations, 24 SSA instructions; the finite path retains 256 operations, 128 captures, and 128 exact captures. The baseline finite path formats/parses the region while current insertion uses direct checked semantic operations; the comparison measures the same admitted source behavior through construction, verification, lowering and planning.
- Native CPU witnesses check all 1024 amplitudes independently for a Bell state plus final Rz phase (nonzero indices 0 and 513) to tolerance `1e-12`, before timing. The circuit has 259 operations, 128 shared matrix/oracle alias pairs and two signed control profiles. Warm execution starts from a zeroed register each iteration. The environment builder defaults disable GPU, threading and distribution in both versions.

This is local Darwin serial CPU evidence. Linux, MPI, OpenMP scaling, accelerator execution and native C++ allocation breakdowns were not measured. The matching raw receipts and portable fixture allow separate platform campaigns without extrapolating from these results. No production changes followed this run.
