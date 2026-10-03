# Matched-accuracy numerical measurements

The active launcher requires Python 3.11+ and `tomli-w` to serialize scratch
Cargo manifests. From the repository root, install its declared dependency in
an isolated environment:

```sh
python3 -m venv /tmp/quest-measurement-tools
/tmp/quest-measurement-tools/bin/python -m pip install -r docs/verification/fixtures/requirements.txt
```

Use that environment's Python for the launcher and its Python regression tests.
Generated packages also use the Rust `csv` crate; fetch their dependencies before
an offline build when the Cargo cache does not already contain them.

Run from the current repository's project-local `devenv` after stopping other
builds and measurements. Supply an untouched checkout or archive of baseline
`001a2b656a5a80a60659a408f87f57670309a09b` and a fresh output directory:

```sh
devenv shell -- /tmp/quest-measurement-tools/bin/python docs/verification/fixtures/static-architecture/numerical/run.py \
  --repository "$PWD" \
  --baseline-repository /path/to/baseline \
  --output /path/to/fresh-numerical-results
```

The runner requires at least three trials and refuses to overwrite existing
output. It generates two standalone scratch Cargo packages under the output,
using path dependencies resolved from the supplied repository roots and copies
of their lockfiles. All build artifacts and receipts remain under the output.
No repository source is edited. Both packages use release thin-LTO and two Cargo
jobs. Run inside `devenv` so `rustc`, Cargo and system dependencies match the
project environment. The tool records compiler/environment details, input and
resolved lockfiles, source hashes before/after measurement, binary hashes and
Mach-O/ELF section sizes. Source mutation during a run invalidates its result.

The primary baseline is the old canonical dynamic `function!`; a separate
baseline `typed_function!` binary also measures the historical static path.
Current uses the canonical static `function!`. The shared allocator at
`../allocator.rs` counts successful Rust allocation/reallocation calls and peak
extra live allocation bytes above a warmed starting point. It includes its
counting overhead equally in both versions. It excludes stack storage, allocator
metadata and native allocations bypassing Rust's global allocator.

Measured workloads are:

- `ln(1+x*x)` construction (10,000), scalar value and second-order jet (1,000,000),
  interval value and interval jet on `[0.2,0.3]` (10,000).
- Ten complete degree-three Remez approximations of `exp` on `[-1,1]`. Every solve
  must certify minimax gap at most `1e-8` and total uniform error below `0.006`.
  Current explicitly selects the binary64 export before certification and creates
  the same complex64 polynomial boundary as baseline inside the timed call. Actual
  final uniform/lower/gap bounds are recorded outside the timed region.
- Extended Newton for `x²-1` on `[-2,2]` with center zero (10,000), verifying two
  output branches `[-2,-0.25]` and `[0.25,2]` to `1e-14` endpoint tolerance.
- Separately, ten current-only 256-bit approximations of exact `1/3`, degree zero,
  with certified uniform error `1e-50`. The result cannot implicitly export
  binary64. This has no equivalent binary64 baseline and is never a speedup ratio.

Every runtime workload warms once, then records elapsed nanoseconds, allocation
count, peak extra live bytes and logical output scalar count (interval endpoints
count twice; Remez counts real coefficients, represented with zero imaginary
components in the exported complex64 polynomial). Trials rotate binary order. Summarize runtime receipts with:

```sh
python3 docs/verification/fixtures/static-architecture/numerical/summarize.py \
  --receipts /path/to/fresh-numerical-results/receipts
```

Build receipts distinguish an empty Cargo artifact directory, unchanged Cargo
invocation and touched-source rebuild (source contents unchanged). Supplemental
baseline typed/current MP executables reuse their package's built dependencies.
Operating-system caches are not flushed. Single build timings are observations,
not distributions. These are numerical caller/dependency-closure costs, not
whole-workspace or native compiler costs. Do not extrapolate function timings to
solver or application speedups; report their independently measured results and
retain regressions in time, allocations, storage and executable size.
