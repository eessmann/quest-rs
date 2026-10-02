# Performance measurements

**Final MP solver and project measurements are complete.** The final MP run includes the paired-trigonometry and pi-hoisting changes, with identical before/after source fingerprints. The initial measurements remain below as historical evidence. Final project/native-build receipts are recorded separately as `project-final`, with unchanged source fingerprints across the run.

The final complete-solver comparison favors Dashu for Remez (1.395x/1.176x baseline-to-Dashu ratios at 128/256 bits), this split-Newton case (13.878x/14.292x), and exact synthesis (1.141x/1.157x). Offline QSP remains slower: 51.967 ms versus 46.902 ms at 128 bits and 84.367 ms versus 55.897 ms at 256 bits, taking 1.108x/1.509x baseline time. These are workload-specific outcomes; the paired-call changes did not remove the QSP regression.

The final project run shows the finite/exact-capture compiler case 2.176x faster. Binary64 QSP and native medians remain close with overlapping ranges; compiler verify/lower/plan has a 7.56% slower median on this small case. The corrected unchanged build takes 0.55 s versus 28.19 s for the frozen baseline, and the touched-source build takes 17.19 s versus 28.56 s.

## Final complete MP solver results

Source and measurement receipts are preserved in `mp-solvers-final/receipts`, published as `mp-solvers-final.tar.gz`. The baseline is the frozen pre-port static architecture described below. The final run uses the same fixture, admission checks, precision settings, iteration counts, host/toolchain, and three-trial procedure as the initial run. All measured source fingerprints match before and after the final run. Tables show per-iteration wall time in microseconds: median [minimum, maximum]. `Baseline/Dashu` above one favors Dashu.

| Workload | Bits | Baseline µs median [min, max] | Final Dashu µs median [min, max] | Baseline/Dashu |
| --- | --- | --- | --- | --- |
| Remez exp degree 3 | 128 | 29,850.733 [29,698.058, 30,073.483] | 21,398.658 [20,976.250, 21,746.900] | 1.395x |
| Remez exp degree 3 | 256 | 44,664.125 [44,223.858, 44,807.700] | 37,969.450 [37,760.783, 38,258.042] | 1.176x |
| Offline QSP degree 16 | 128 | 46,902.183 [46,706.975, 47,408.183] | 51,966.575 [51,965.942, 52,370.742] | 0.903x |
| Offline QSP degree 16 | 256 | 55,897.000 [55,595.992, 56,045.667] | 84,367.475 [84,284.425, 84,502.783] | 0.663x |
| Split Newton | 128 | 53.128 [53.059, 53.418] | 3.828 [3.799, 3.843] | 13.878x |
| Split Newton | 256 | 54.301 [53.661, 54.504] | 3.799 [3.790, 3.827] | 14.292x |
| Exact synthesis | 128 | 92.882 [91.872, 95.262] | 81.428 [80.383, 82.333] | 1.141x |
| Exact synthesis | 256 | 92.237 [91.293, 95.740] | 79.730 [77.872, 80.567] | 1.157x |

Allocation counts are divided by iterations (Remez/QSP 5, Newton 500, exact synthesis 25). Peak extra live bytes are the maximum above the warmed starting baseline across the measured loop, not divided by iterations. All allocation counts and peaks are identical across the three final trials.

| Workload | Bits | Baseline alloc./iteration | Final Dashu alloc./iteration | Baseline peak bytes | Final Dashu peak bytes |
| --- | --- | --- | --- | --- | --- |
| Remez exp degree 3 | 128 | 726,499 | 265,151 | 12,734 | 7,966 |
| Remez exp degree 3 | 256 | 969,886 | 543,175 | 15,710 | 13,814 |
| Offline QSP degree 16 | 128 | 940,990 | 699,041 | 230,304 | 176,888 |
| Offline QSP degree 16 | 256 | 989,522 | 1,323,255 | 273,304 | 246,576 |
| Split Newton | 128 | 1,305 | 2 | 1,096 | 320 |
| Split Newton | 256 | 1,305 | 2 | 1,592 | 320 |
| Exact synthesis | 128 | 199 | 199 | 4,840 | 4,456 |
| Exact synthesis | 256 | 199 | 199 | 4,840 | 4,456 |

Relative to the initial Dashu run, final offline-QSP allocation counts fall from 710,389 to 699,041 per iteration at 128 bits (1.60%) and from 1,338,933 to 1,323,255 at 256 bits (1.17%); both peak-live values remain unchanged. These are measured allocation savings. Final Dashu QSP medians are about 2.92%/2.26% below the initial Dashu medians, but the runs are separate and their baselines also moved. This is not a controlled attribution of all timing movement to the optimization. The paired calls preserve represented arguments, both native error checks, directed endpoint rounding, and certification requirements. The remaining regression cannot be assigned to any one library path without profiling; the source-visible costs discussed below are hypotheses, not measured attribution.

Final MP builds use fresh Cargo target directories. These are single observations, not repeated-build statistics; CPU is user plus system time and Darwin RSS is converted from bytes to MiB.

| Tree | Wall s | CPU s | Peak RSS MiB |
| --- | --- | --- | --- |
| astro | 68.78 | 100.72 | 1038.22 |
| dashu | 67.66 | 102.48 | 1038.09 |

| Executable | Baseline bytes | Final Dashu bytes | Change |
| --- | --- | --- | --- |
| Solvers | 1,608,192 | 2,130,672 | +32.49% |

Executable SHA-256 identities are recorded in the final `binaries.json`.

## Final project compiler, binary64 QSP and native results

Final `project-final` receipts use the same workloads and three-trial procedure described below, after the native build-watch correction. The complete `frozen-before.sha256` and `frozen-after.sha256` files are byte-for-byte identical. The preserved archive is `project-final.tar.gz`. Times are per-iteration wall microseconds, median [minimum, maximum]; ratios above one favor Dashu.

| Workload | Baseline µs median [min, max] | Final Dashu µs median [min, max] | Baseline/Dashu |
| --- | --- | --- | --- |
| Binary64 QSP degree 256 | 1,047.434 [1,039.059, 1,076.646] | 1,078.750 [1,050.823, 1,451.087] | 0.971x |
| Binary64 QSP degree 1024 | 5,047.250 [4,982.792, 5,374.135] | 5,050.489 [4,997.385, 5,605.573] | 0.999x |
| Compiler construction/admission | 3.579 [3.361, 3.626] | 3.545 [3.494, 3.955] | 1.010x |
| Compiler verify/lower/plan | 5.450 [5.188, 5.621] | 5.862 [5.583, 5.911] | 0.930x |
| Compiler finite/exact captures | 1,168.349 [1,095.240, 1,214.411] | 536.970 [532.897, 564.209] | 2.176x |
| Native preparation | 77.667 [76.910, 82.027] | 79.202 [77.872, 79.811] | 0.981x |
| Native zeroed execution | 1,107.815 [1,074.346, 1,128.979] | 1,096.314 [1,075.096, 1,119.795] | 1.010x |

Allocation counts are per iteration; peak extra live bytes are not divided by iterations. Both are invariant across all three final project trials. Native measurements count Rust allocations only.

| Workload | Baseline alloc./iteration | Final Dashu alloc./iteration | Baseline peak bytes | Final Dashu peak bytes |
| --- | --- | --- | --- | --- |
| Binary64 QSP degree 256 | 8,731 | 8,731 | 248,856 | 248,856 |
| Binary64 QSP degree 1024 | 34,141 | 34,141 | 1,002,664 | 1,002,664 |
| Compiler construction/admission | 157 | 157 | 19,147 | 19,147 |
| Compiler verify/lower/plan | 180 | 180 | 16,107 | 16,107 |
| Compiler finite/exact captures | 17,433 | 13,849 | 597,930 | 583,594 |
| Native preparation | 1,837 | 1,837 | 210,183 | 210,183 |
| Native zeroed execution | 4,704 | 4,704 | 121,312 | 121,312 |

The finite/exact-capture compiler case is 2.176x faster and reduces Rust allocations from 17,433 to 13,849 per iteration (20.56%). Its peak extra live bytes also decrease. Binary64 QSP medians are within about 3% and native medians within about 2%, with overlapping trial ranges and identical allocation counts/peaks. The current binary64-QSP first trial is noticeably slower than its other trials, especially at degree 256; small differences should not be generalized. Compiler verify/lower/plan has a 7.56% slower median (5.862 versus 5.450 µs), with a small overlap in trial ranges and unchanged allocation measurements. This small absolute regression is retained explicitly; three instrumented trials do not establish its cause or statistical significance.

Final project build costs are single observations. CPU is user plus system time; RSS is Darwin maximum resident bytes converted to MiB. A cold Cargo target is not a cold operating-system cache or a rebuild of the installed QuEST library.

| Tree | Condition | Wall s | CPU s | Peak RSS MiB |
| --- | --- | --- | --- | --- |
| baseline | cold | 129.19 | 232.37 | 1037.77 |
| baseline | unchanged | 28.19 | 46.88 | 346.47 |
| baseline | touched-source | 28.56 | 47.50 | 346.36 |
| current | cold | 132.49 | 237.02 | 924.53 |
| current | unchanged | 0.55 | 0.18 | 77.70 |
| current | touched-source | 17.19 | 32.34 | 346.95 |

The final unchanged current build takes **0.55 s versus 28.19 s** for the frozen baseline and has no `Compiling` entries. The touched-source current build takes **17.19 s versus 28.56 s** and compiles only the fixture package; baseline unchanged and touched builds both recompile `quest-sys`, `quest-rs`, and the fixture. This supports removal of the observed generated-output watch loop for this build path. The improvement is a build-input tracking correction, not an arithmetic speedup. The cold current build is 132.49 s versus 129.19 s; a single build per tree does not establish a repeatable cold-build regression. The focused build-watch review separately covers retained invalidation behavior.

| Executable | Baseline bytes | Final Dashu bytes | Change |
| --- | --- | --- | --- |
| Project | 4,354,432 | 4,420,720 | +1.52% |

Executable hashes and section reports are preserved in the final project archive. Binary size is unchanged from the initial project run; hashes remain specific to each measured build.

## Historical initial measurements — before remediation

**Status of the tables below: historical initial Dashu results.** These measurements precede the paired-trigonometry/pi-hoisting optimization and native build-watch remediation. Their source identities and results remain separate from the final MP measurements above. Final project measurements appear above. Nothing in these initial tables claims a measured benefit from the later changes.

The initial results favor Dashu for complete MP Remez, this split-Newton case, exact synthesis, and the compiler workload with exact captures. Offline MP QSP is slower at both tested precisions. Binary64 QSP and native runtime medians are close, with overlapping trial ranges. The initial unchanged native builds also repeat substantial work; that diagnosed watch-set defect is recorded separately from arithmetic performance.

## Provenance and method

The baseline is the frozen **uncommitted static architecture using Astro floats and its prior exact arithmetic**, preserved in [pre-port-source.tar.gz](pre-port-source.tar.gz). It is not the earlier committed `001a2b6` tree. Current means the initial Dashu port identified by the run's before/after source hashes. The [archive inventory](archives.json) records source archive identities.

Both same-run comparisons use the project-local environment, `rustc 1.100.0-nightly` revision `6bb1652a020e80cef79332741d89e996d71933c9`, LLVM 23.1.1, aarch64 Darwin on the local Apple M3 Pro, release thin LTO, and two Cargo build jobs. Solver and project runners make three trials and reverse backend order in trial two. Runtime tables show the median and full minimum–maximum range of **per-iteration wall time**, in microseconds. These descriptive ranges are not confidence intervals. Build measurements are one observation per condition.

Preserved raw evidence:

- [Complete solvers, before optimization](mp-solvers-pre-optimization.tar.gz): runtime CSVs, process-time/RSS receipts, build logs, input/resolved locks, source hashes, toolchain, executable sizes/hashes.
- [Project workloads, before optimization](project-pre-optimization.tar.gz): binary64 QSP, compiler and native CSVs/diagnostics, cold/unchanged/touched-source build logs, sections, locks, source hashes, executable identities.
- [Dashu primitive run](primitives.tar.gz): raw/summary CPU and wall results and build/source receipts. Historical Astro/Rug rows retain their original evidence under the primitive fixture; their [source archive](historical-astro-rug-primitive-source.tar.gz) is separate.

The benchmark runners reject changed frozen inputs. They use identical workload source across compared checkouts. See [benchmark review](benchmark-review.md) for reviewed corrections and limitations.

## Historical initial complete solver outcomes

Remez approximates exp(x) on [-1,1] with four coefficients. Both backends request uniform bound 0.006; the minimax gaps are 1e-20/1e-35 and root widths 1e-25/1e-45 at 128/256 bits. QSP uses the same 17 deterministic complex coefficients, one producer precision attempt, and an independently certified binary64 export. Newton checks its two split images; exact synthesis reconstructs the exact target matrix. Thus comparisons match admitted outcomes and requested accuracy, rather than identical intermediate floating-point values.

`Baseline/Dashu` above one means Dashu is faster. The iteration counts per trial are Remez 5, offline QSP 5, Newton 500, and exact synthesis 25.

| Workload | Bits | Baseline µs median [min, max] | Dashu µs median [min, max] | Baseline/Dashu |
| --- | --- | --- | --- | --- |
| Remez exp degree 3 | 128 | 30,237.808 [30,204.692, 30,281.925] | 21,482.142 [21,421.367, 21,546.025] | 1.408x |
| Remez exp degree 3 | 256 | 45,099.592 [44,645.158, 46,131.908] | 37,949.192 [37,796.900, 38,596.175] | 1.188x |
| Offline QSP degree 16 | 128 | 47,724.608 [47,484.342, 47,758.500] | 53,529.067 [52,864.783, 54,026.458] | 0.892x |
| Offline QSP degree 16 | 256 | 56,778.933 [56,202.217, 58,758.458] | 86,321.550 [86,086.458, 87,507.258] | 0.658x |
| Split Newton | 128 | 53.942 [53.611, 54.035] | 4.151 [4.136, 4.244] | 12.996x |
| Split Newton | 256 | 54.889 [54.247, 55.087] | 4.059 [4.041, 4.122] | 13.524x |
| Exact synthesis | 128 | 92.113 [91.762, 94.485] | 77.447 [77.410, 78.280] | 1.189x |
| Exact synthesis | 256 | 92.177 [91.597, 94.223] | 77.362 [77.105, 77.703] | 1.192x |

Rust allocation counts below are **per iteration**, computed by dividing raw totals by the stated iteration count. Peak extra live bytes cover the complete measured loop above its warmed starting baseline and are **not divided by iterations**. Allocation counts and peaks were identical across all three trials for every row; their minima, medians and maxima therefore coincide.

| Workload | Bits | Baseline alloc./iteration | Dashu alloc./iteration | Baseline peak bytes | Dashu peak bytes |
| --- | --- | --- | --- | --- | --- |
| Remez exp degree 3 | 128 | 726,499 | 265,151 | 12,734 | 7,966 |
| Remez exp degree 3 | 256 | 969,886 | 543,175 | 15,710 | 13,814 |
| Offline QSP degree 16 | 128 | 940,990 | 710,389 | 230,304 | 176,888 |
| Offline QSP degree 16 | 256 | 989,522 | 1,338,933 | 273,304 | 246,576 |
| Split Newton | 128 | 1,305 | 2 | 1,096 | 320 |
| Split Newton | 256 | 1,305 | 2 | 1,592 | 320 |
| Exact synthesis | 128 | 199 | 199 | 4,840 | 4,456 |
| Exact synthesis | 256 | 199 | 199 | 4,840 | 4,456 |

Offline QSP takes 1.122x baseline time at 128 bits and 1.520x at 256 bits. Its allocation count **decreases at 128 bits** but increases at 256 bits; both measured peak-live values decrease. Higher allocation count is therefore not a blanket explanation of this regression. Remez improves despite the primitive multiplication result. Newton's large reduction is specific to this small exact/dyadic contraction workload; it is not a claim of a universal 13x root-solver speedup. Exact synthesis uses the same exact integer workload at both precision-limit settings.

## Historical initial project compiler, binary64 QSP and native outcomes

These comparisons include ordinary binary64 QSP at degrees 256/1024, compiler construction/admission and planning, a 256-operation finite circuit with 128 exact angle captures, and native preparation/execution of the verified 10-qubit Bell/full-phase workload. QSP checks coefficient response at four unit-circle points and records diagnostics; these project QSP checks are not independent certification. Native validation checks all 1,024 amplitudes before timing. The baseline/current native preparation uses the same installed QuEST 4.3.0 environment.

| Workload | Baseline µs median [min, max] | Initial Dashu µs median [min, max] | Baseline/Dashu |
| --- | --- | --- | --- |
| Binary64 QSP degree 256 | 1,044.524 [1,034.799, 1,053.694] | 1,049.278 [1,030.701, 1,052.823] | 0.995x |
| Binary64 QSP degree 1024 | 5,021.656 [5,013.479, 5,143.833] | 4,989.812 [4,981.198, 5,518.636] | 1.006x |
| Compiler construction/admission | 3.390 [3.364, 8.452] | 3.364 [3.332, 3.441] | 1.008x |
| Compiler verify/lower/plan | 5.356 [5.209, 7.975] | 5.416 [5.291, 5.925] | 0.989x |
| Compiler finite/exact captures | 1,114.501 [1,093.023, 1,227.786] | 540.295 [535.542, 542.316] | 2.063x |
| Native preparation | 77.849 [77.453, 78.323] | 78.233 [78.181, 79.983] | 0.995x |
| Native zeroed execution | 1,079.830 [1,071.812, 1,093.532] | 1,070.574 [1,069.257, 1,091.522] | 1.009x |

The same allocation/peak conventions apply. Counts and peaks again did not vary across trials.

| Workload | Baseline alloc./iteration | Dashu alloc./iteration | Baseline peak bytes | Dashu peak bytes |
| --- | --- | --- | --- | --- |
| Binary64 QSP degree 256 | 8,731 | 8,731 | 248,856 | 248,856 |
| Binary64 QSP degree 1024 | 34,141 | 34,141 | 1,002,664 | 1,002,664 |
| Compiler construction/admission | 157 | 157 | 19,147 | 19,147 |
| Compiler verify/lower/plan | 180 | 180 | 16,107 | 16,107 |
| Compiler finite/exact captures | 17,433 | 13,849 | 597,930 | 583,594 |
| Native preparation | 1,837 | 1,837 | 210,183 | 210,183 |
| Native zeroed execution | 4,704 | 4,704 | 121,312 | 121,312 |

The finite compiler workload improves by about 2.06x with fewer Rust allocations. The other compiler, binary64 QSP, and native median differences are small relative to the displayed variability and should not be presented as established speedups/regressions. In particular, the first baseline compiler construction/planning trial is markedly slower than the other two. Native allocation counts and peaks are unchanged; they exclude C++/QuEST allocation. The initial native detail receipt records 723,094 admitted environment bytes for current preparation, which is a resource model rather than a measured heap total.

## Historical initial build and executable cost

Fresh target directories measure a dependency-cold Cargo build, not a cold operating-system cache or native QuEST rebuild. `CPU s` is user plus system time from `/usr/bin/time`; it is not the runtime table's wall time. RSS is reported in MiB from Darwin's byte-valued maximum resident set size. No repeated-build statistical claim is supported by these single observations.

| Fixture | Tree | Condition | Wall s | CPU s | Peak RSS MiB |
| --- | --- | --- | --- | --- | --- |
| Solvers | astro | build | 69.37 | 100.39 | 937.39 |
| Solvers | dashu | build | 67.08 | 102.16 | 979.89 |
| Project | baseline | cold-build | 126.54 | 228.70 | 913.69 |
| Project | baseline | unchanged-build | 29.76 | 48.09 | 344.52 |
| Project | baseline | touched-source-build | 29.57 | 47.85 | 346.36 |
| Project | current | cold-build | 126.11 | 228.89 | 890.28 |
| Project | current | unchanged-build | 28.74 | 47.66 | 345.88 |
| Project | current | touched-source-build | 29.43 | 48.54 | 347.52 |

The initial project unchanged builds take about 29 seconds and rebuild native consumers. Investigation identified generated CXX/CMake outputs being registered as Cargo inputs, making subsequent unchanged builds stale. The [native build-watch review](native-build-review.md) records the correction. **The initial table above predates that fix and is not the expected final no-op-build cost.** The final `project-final` measurements above record the corrected unchanged-build cost; the focused review records retained invalidation checks. Touched-source builds update only the generated fixture main.rs timestamp.

| Executable | Baseline bytes | Initial Dashu bytes | Change |
| --- | --- | --- | --- |
| Solvers | 1,608,192 | 2,129,824 | +32.44% |
| Project | 4,354,432 | 4,420,720 | +1.52% |

Executable hashes and section reports remain in the corresponding archives. These fixture binaries have different dependency/feature sets; their sizes are not a statement about every production executable. The solver binary grows more than the full project binary in percentage terms.

## Historical primitive comparison

Each cell below is **historical backend median process CPU time / initial Dashu median process CPU time** at 128 / 256 / 512 nominal bits. Values above one favor Dashu; values below one favor the historical backend. Inputs and algorithms match the archived fixture, but these are not contemporaneous interleaved comparisons. There is **no whole-solver Rug measurement**. Do not apply these ratios to Remez, QSP, compilation, or native execution.

| Workload | Historical Astro / Dashu, 128 / 256 / 512 | Historical Rug / Dashu, 128 / 256 / 512 |
| --- | --- | --- |
| add | 0.666x / 0.528x / 0.551x | 0.353x / 0.309x / 0.331x |
| mul | 0.496x / 0.402x / 0.494x | 0.328x / 0.282x / 0.347x |
| div | 1.457x / 0.702x / 0.892x | 0.396x / 0.237x / 0.301x |
| exp_endpoints | 1.989x / 1.590x / 1.656x | 0.087x / 0.085x / 0.114x |
| ln_endpoints | 1.099x / 0.615x / 0.330x | 0.031x / 0.027x / 0.020x |
| sin_endpoints | 1.038x / 0.819x / 0.757x | 0.059x / 0.057x / 0.058x |
| cos_endpoints | 0.755x / 0.652x / 0.616x | 0.041x / 0.042x / 0.046x |
| matmul16 | 0.539x / 0.426x / 0.501x | 0.322x / 0.272x / 0.319x |
| qr16 | 0.703x / 0.562x / 0.644x | 0.315x / 0.278x / 0.317x |

Primitives have three trials; raw CPU/wall minima and maxima are retained in the archived summary. The historical comparisons favor Rug broadly, while Dashu's exponential endpoint pairs improve on historical Astro. Matrix multiplication and modified Gram-Schmidt compare nominal precision, not independently matched matrix residual accuracy. The primitive executable has no allocation-counting global allocator, unlike the complete solver/project fixtures.

## Interpretation and limits

The [source-grounded investigation](performance-investigation.md) identifies plausible costs: native integer-product normalization/splitting, borrowed-addition clones, and Dashu's protected below-one logarithm path. These are **hypotheses about contribution**, not profiling-based attribution. The actual complete-solver observations demonstrate why isolated multiplication ratios cannot be extrapolated to application speed.

Combined sine/cosine calls and hoisting invariant pi retrieval remove duplicated work without changing represented arguments or relaxing directed errors. Their implementation is reviewed in [the numerical review addendum](numerical-review.md). Neither their speedup nor the native-watch remediation's final build benefit is contained in these initial receipts. The optional exact-argument ln1p route is not a measured result here.

Solver and project timings include Rust allocator atomics; the overhead depends on allocation activity. Complete solver timing also includes its correctness assertions and exact reconstruction on every iteration. The project fixture performs its main numerical checks before timing and retains lightweight output checks in the measured loop. No subtraction of instrumentation cost is attempted.

Peak additional live Rust bytes exclude allocator metadata, stack storage, internal realloc overlap, and native allocations. They are not resident memory or an allocator-enforced limit. Whole-process peak RSS, including code and native storage, is retained separately in trial detail/time receipts and is not a per-workload native heap breakdown. Warmed caches and single-host scheduling/thermal behavior remain part of the measured conditions. Three trials do not establish statistical significance for small differences.

These local Apple Silicon CPU measurements do not establish Linux, HPC, MPI, accelerator, cross-compiler, or cross-machine performance. The Dashu-only architecture and the mathematical contracts remain mandatory regardless of measured relative speed; no guarantee weakening is justified by these results.

## Final evidence status

Final MP solver and project figures are reported above from source-stable `mp-solvers-final` and `project-final` receipts. Their before/after source fingerprints match independently within each run. The initial tables remain historical evidence. Benchmark completion and source stability establish the identity of these measurements; integrated correctness/test evidence is recorded separately and should not be inferred from timing alone.
