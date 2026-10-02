# Numerical performance comparison

Status: final portable reproduction completed successfully after the final 989-test workspace gate, 56 doctests, strict all-target Clippy, rustdoc and formatting checks. Final receipts are `performance/final-portable-v2/receipts/`; their before/after source hashes agree. Earlier measurements remain preserved as diagnostic evidence.

## Protocol and provenance

Baseline source is archived commit `001a2b656a5a80a60659a408f87f57670309a09b`. Both historical canonical `function!` (dynamic expressions) and `typed_function!` are measured; the new canonical static API is compared against both. The prior archived `solver_measurement.rs` used typed expressions. Its original receipts remain unchanged.

The portable fixture is checked in at `docs/verification/fixtures/static-architecture/numerical/`, with a shared counting allocator one directory above. It generates standalone scratch packages from explicit repository paths. Neither package is a workspace member. All builds use project `devenv`, Rust `1.100.0-nightly` commit `6bb1652a020e80cef79332741d89e996d71933c9` (2026-09-22), LLVM 23.1.1, Darwin ARM64, release thin-LTO, two Cargo jobs. This compiler exactly matches the original baseline environment receipt. Full source hashes, input/resolved lockfiles, executable hashes, byte/section sizes, build timing logs, actual solver bounds and three rotated runtime trials are retained under `performance/final-portable-v2/receipts/`. The fixture README specifies workloads and measurement limitations. The host is an Apple M3 Pro with 11 CPUs and 18 GiB RAM; this is local Darwin evidence, not a Linux/HPC campaign.

The principal whole-solver workload is ten full degree-three approximations of `exp` on `[-1,1]`, each requiring certified minimax gap at most `1e-8` and uniform error below `0.006`, including the actual binary64 coefficient boundary. Both initial algorithms actually achieved a gap near `6e-15`, with uniform error approximately `0.00552837010869`. The extended Newton workload repeats 10,000 times on `x²-1`, box `[-2,2]`, center zero; both return the same two branches to `1e-14` endpoint tolerance. Function timing is reported separately and never extrapolated to solver performance.

## Initial regression investigation

Initial current source measured 46.4 ns per scalar second-order jet, versus 24.0 ns for baseline canonical dynamic and 8.6 ns for baseline typed. Source inspection found that AD's zero/one/two seeds flowed through the same large adaptive MP exact-constant importer as arbitrary rationals. Thin direct checked binary64 `point` overrides and inline fast constant branches now separate those hot operations from adaptive rational/decimal import. This preserves numerical checks and exposes fixed AD seeds to optimization. Final measurements must confirm the effect.

Initial Remez latency improved from 38.08 ms (canonical dynamic) and 51.48 ms (typed) to 0.612 ms, but allocations rose from 75 to 362 per solve and peak extra live Rust allocator storage rose from 3,024 to 524,840 bytes. The peak was directly explained by reserving `max_boxes=32768` interval slots before any root-cover work: 524,288 bytes. Root coverage now grows its queue only when needed, with a fallible pre-commit reserve and the invariant that pending capacity also admits previously unresolved boxes on recovery. Actual capacities are re-admitted. The allocation regression test failed on the original 10,000-slot reservation and passes with at most 1,024 bytes per allocation for a one-box continuum.

The remaining count of small allocations comes from contractor branch/step vectors, retained coverage/candidate evidence, and QR normalization/residual buffers. Those are separate from the eliminated worst-case queue reservation. Final receipts will retain any remaining allocation regression rather than interpreting lower latency as lower allocation cost.

Initial build costs were 61.77 s baseline versus 65.01 s current for clean artifact directories; touched-source rebuilds were 4.30 s versus 5.36 s. Executable sizes were 1,016,528 bytes baseline canonical, 977,344 baseline typed and 1,205,248 current. These preliminary build costs include the new dependency/capability closure and are not whole-workspace build timings. No OS cache flush was performed.

The separate initial MP workload computed an exact rational `1/3` degree-zero approximation at 256 bits with certified uniform error `1e-50`: approximately 0.243 ms, 4,725 allocations and 2.62 MB peak extra storage per solve before the queue fix. This task has no equivalent binary64 baseline and cannot authorize a binary64 export. It is capability evidence, not a speedup comparison.

## Correctness checks for performance changes

`devenv shell -- cargo test -p quest-numerics`: 57 runtime tests and four doctests pass, including MPFR-oracle arithmetic, exact constant imports, invalid domain/nonfinite inputs, subnormal and signed-zero point inputs, derivative formulas, split/repeated roots, partial coverage on errors and byte/work limits. Strict all-target numerics Clippy passes. Logs are `performance/numerics-fixes-green.log` and `performance/numerics-fixes-clippy.log`; allocation red gate is `performance/root-allocation-red.log`.

## Intermediate reviewed candidate and assembly diagnosis

The first portable reproduction is retained unchanged in `performance/final-portable/`. It passed the source-freeze check and matched the materialized complex64 output boundary. Median scalar jet time fell to 13.36 ns from the initial 46.4 ns, but remained above historical typed's 7.04 ns. Its release assembly showed the current scalar jet expression calling outlined generic `JetBackend<F64Backend>::add` and `mul`, with a 272-byte frame, while the typed baseline expression used a 64-byte frame. Both paths perform finite checks; this difference must not be described as checked versus unchecked arithmetic. Narrow plain `#[inline]` annotations on those two operations and the typed-expression forwarding boundary were subsequently approved to expose the existing arithmetic to optimization without removing checks.

Intermediate Remez medians were 37.83 ms dynamic, 35.32 ms typed, and 0.551 ms current; allocation counts were 75, 73 and 376, and peak additional live bytes 3,024, 2,784 and 2,240. Current's certificate reported uniform error `0.00552837010869145027`, lower error `0.00552837010868545420`, and gap `5.99607169471383372e-15`. Baseline reports meet the same prescribed error/gap contract, but their proof implementation and owning outputs differ from current's separately typed uniform/minimax certificates and retained request/coverage. Matching these numerical requirements does not establish identical proof machinery.

The intermediate snapshot predates the final faer QR workspace admission correction, narrow inline hints, and bulk modeled-work reservation. Final measurements therefore use another fresh portable output directory and rebuild both baseline and current from empty Cargo artifact directories. The intermediate data and assembly remain evidence for the diagnosis, not the final-source performance claim.


## Final paired measurements

Values below are medians of three trials, per call. Parentheses give the full
observed trial range. Function/contractor units are nanoseconds; solver units
are milliseconds. Each runtime workload warms once before its measured loop.
The recorded output counts are respectively one function/value, three jet
components, two interval endpoints, six interval-jet endpoints, four real
coefficients (materialized as complex64), and four contractor endpoints.

| Workload | Baseline dynamic | Baseline typed | Current canonical |
|---|---:|---:|---:|
| Construct function, ns | 109.75 (107.71–326.86) | 0.275 (0.275–0.275) | 0.275 (0.271–0.296) |
| Scalar value, ns | 14.763 (14.347–15.733) | 3.025 (3.016–3.039) | 3.015 (2.978–3.099) |
| Scalar second-order jet, ns | 23.817 (23.495–24.007) | 7.225 (7.183–7.350) | 7.027 (6.976–7.076) |
| Interval value, ns | 107.229 (106.971–108.092) | 95.363 (93.800–96.563) | 85.767 (83.854–89.546) |
| Interval second-order jet, ns | 839.033 (828.133–847.954) | 837.688 (827.217–852.896) | 773.746 (763.200–796.188) |
| Full exp degree-three Remez, ms | 36.198 (36.087–36.331) | 36.683 (36.078–36.741) | 0.5568 (0.5515–0.5580) |
| Extended Newton two branches, ns | 534.950 (531.350–542.438) | 537.583 (532.246–538.392) | 545.058 (542.321–557.267) |

The whole Remez solve is 65.0× faster than the old canonical path and 65.9× faster
than the old typed path for this workload and checked error contract. This is a
measured complete solve, not an extrapolation from expression timings. Static
construction approaches the harness/loop floor; its subnanosecond measurement
is not a portable estimate of useful application latency. The approximately
1–2% Newton difference is small compared with trial variation and three trials
do not establish a persistent regression.

Every timed Remez call checks gap `<=1e-8` and uniform error `<0.006`. The separate
untimed accuracy receipts show baseline uniform upper
`5.52837010869122823e-3`, minimax lower `5.52837010868456602e-3`, and gap
`6.66220550948892765e-15`; current reports `5.52837010869145027e-3`,
`5.52837010868545420e-3`, and `5.99607169471383372e-15`. Both variants pass the
same mathematical accuracy requirements with the same exported polynomial
representation. The certificate/ownership differences noted above remain.

| Workload | Allocations per call: dynamic / typed / current | Peak extra live bytes: dynamic / typed / current |
|---|---:|---:|
| Construction | 5 / 0 / 0 | 600 / 0 / 0 |
| All warm function evaluations | 0 / 0 / 0 | 0 / 0 / 0 |
| Full Remez | 75 / 73 / 376 | 3,024 / 2,784 / 2,112 |
| Extended Newton | 3 / 3 / 2 | 160 / 160 / 64 |

Remez retains a real allocation-count regression: 301 more calls than dynamic
and 303 more than typed per solve. Source inspection identifies repeated small
contractor branch/result vectors, QR normalization/residual storage, and owning
candidate/coverage/certificate data. Those allocations now have a smaller live
peak, and the full solve remains about 0.557 ms. These are different metrics:
the result does not claim every allocation measure improves. The final 2,112-byte
peak is about 250× smaller than the initial preallocated-queue peak; the final
workload also explicitly materializes its C64 polynomial, which the initial
scratch workload did not, so initial-to-final allocation-count changes cannot
be assigned solely to the queue fix. These allocator metrics exclude native
allocations bypassing Rust, allocator metadata, and stack storage.

## Final build and executable costs

Each build timing is one observation. Empty Cargo artifact directories were used
for both cold builds; OS caches were not flushed. These are numerical caller and
dependency-closure builds, not complete workspace/application builds. Darwin
`time -l` maximum resident-set measurements are recorded as bytes; they are
process/resource-usage observations, not summed concurrent build memory.

| Build | Baseline seconds | Current seconds | Baseline max RSS bytes | Current max RSS bytes |
|---|---:|---:|---:|---:|
| Cold | 63.78 | 66.75 | 941,932,544 | 931,102,720 |
| Unchanged | 0.27 | 0.30 | 65,781,760 | 66,715,648 |
| Touched caller source | 4.45 | 5.36 | 311,033,856 | 316,456,960 |

The supplemental typed/MP executable builds, reusing package dependencies, took
4.18 / 4.00 seconds. Final executable sizes are dynamic **1,016,528 bytes**, typed
**977,344 bytes**, current **1,206,944 bytes**, and current MP **871,472 bytes**.
The current binary is 18.7% larger than dynamic and 23.5% larger than typed.
The numerical request/certificate machinery and expanded backend/dependency
closure add linked code and compile work. This fixture does not isolate their
individual contributions; its section receipts and binary hashes preserve the
observed cost. Cold build time increased 4.7% and the touched caller rebuild
20.4% in this observation. Those are retained tradeoffs, not improvements inferred
from runtime speed.

## Final MP capability and inline resolution

The current-only exact rational `1/3` degree-zero task at 256 bits, certified to
uniform error `1e-50`, took median **0.2268 ms** (0.2245–0.3363 ms), with **4,664
allocations** and **4,756 bytes** peak extra live storage. Its single coefficient
remains MP, with no implicit binary64 export. There is no binary64 comparison for
this below-floor task, and no speedup is claimed.

Final release assembly is retained in
`final-portable-v2/receipts/current-scalar-jet-assembly.txt` and
`current-scalar-jet-ln-assembly.txt`. Add and multiply are now visible directly
in the scalar-jet entry point; no outlined scalar JetBackend add/mul calls remain.
The entry point uses a 48-byte frame and calls the existing jet logarithm helper.
The finite checks remain visible. Against the intermediate candidate's 272-byte
expression frame and outlined add/mul calls, this supports the diagnosed cause
of the 13.36-to-7.03 ns improvement. It is not a claim that these frame sizes
represent total call-stack usage. The overall executable grew another 464 bytes
between the intermediate and final snapshots, which also include the QR
admission/bulk-work corrections, so this size change is not attributed solely
to inline annotations.

No production arithmetic or tests changed during final measurement. The portable
runner completed with exit zero and identical source hashes. The final raw
receipts, exact inputs, checks, and reproduction command are preserved; future
Linux/HPC or high-degree claims require their own measurements.
