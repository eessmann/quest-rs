# Reproducible production-stage benchmark campaign

The campaign preserves exact input data and reports failures as results. A
successful accounting marker does not imply successful performance coverage.
The historical 1,488-row campaign is separate and contributes no rows here.

The pinned sources are Rust `0f95954e7951dca9a8d88a10c1a9764ecb58d91f`,
quest-qsvt `4fc35983138d07a990862a4d83ad16f2b737c98f`, and SoftwareX
`245938a575deac7f232797efdade97ba442a366d`. The frozen 62-case corpus must have
SHA-256 `806b1557498debe5d409b214609ad30a74a86d529ae36045a2b3c428b9546e6d`.

## Inputs and scope

`campaign.py prepare` verifies the corpus before extracting either source or
canonical coefficients. Each fixture retains both representations, signed
source offsets, source degree, and hashes of the little-endian IEEE754 complex
coefficients. The harness reconstructs values from unsigned 64-bit IEEE754 payloads,
including signed zero and subnormals; decimal JSON conversion is not the
measurement input boundary. The inventory distinguishes archived source degree from canonical
coefficient count and degree. It contains all 17 named inverse orders through
1,000,000, nine named solvers, 62 SoftwareX cases with separate original solver
and kernel boundaries, five root workloads, two roots-of-unity workloads,
four circuit degrees and five stages, four unitary dimensions, and degree-8105
coordinate preparation under separate fixed-three and Criterion protocols.

The Rust NLFT harness provides five distinct measurements on the exact frozen
canonical inputs: `inverse_only`, `forward_only`, `completion_inverse`,
`validated_roundtrip`, and `full_pipeline`. The opt-in `benchmark-support`
module invokes existing production kernels; it contains no copied transform.
Forward timing includes production reflection normalization and the production
control product tree. Full pipeline includes public admission, completion,
synthesis, and production reconstruction checks. Validated roundtrip includes
completion, inverse, forward, and coefficient comparison inside the operation.
The separate preflight is always outside timing.

These Rust stage measurements do **not** claim identical operation boundaries
to the original SoftwareX public solver or kernel. Their manifests retain
`source_boundary_equivalent=false`. Exact native solver/kernel measurements
remain separate source operations. No seed-based Rust replacement is used for
C++ fixtures. The native inverse exporter includes the pinned source fixture file
unchanged to preserve its RNG, normalization and preparation. Named-solver
contract export remains unavailable in its Laurent-only transport schema.
Its seed is fixed at 2723225244 for the sequential legacy inverse fixtures;
the source solver fixtures retain their original seed 424242.

## Run

Use cargo-nextest 0.9.117 or newer. The harness uses actual
[`cargo nextest bench`](https://nexte.st/docs/features/benchmarks/), with Criterion
10 samples, 100 ms warmup, and a 200 ms requested measurement window. Criterion
may extend that window for slow operations. These distributions are separate
from the original coordinate workload's fixed three samples.

```sh
python3 benchmarks/replication/campaign.py prepare \
  --corpus /path/to/SoftwareX/data/benchmarks/nlft/20260302T151100Z/corpus_full.json \
  --output /path/to/artifacts/source

# Build once before applying source per-case deadlines.
flock /tmp/quest-quality-build.lock \
  systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  env CARGO_TARGET_DIR=/path/to/build CARGO_BUILD_JOBS=4 \
  cargo nextest bench -p quest-qsp --bench replication \
    --features benchmark-support --offline --no-run

systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  python3 benchmarks/replication/campaign.py run-rust \
    --source /path/to/artifacts/source --output /path/to/artifacts/current \
    --workspace /path/to/checkout --target /path/to/build \
    --implementation current
```

Use a separate original checkout with only the same gated wrappers/harness
added; preserve the pristine original archive. Build into separate original
and current target directories. The original canonical lane uses
`--implementation original_0f95954`, and the current lane uses
`--implementation current`; these roles are checked by final accounting.
Every row is serial under the shared build
lock. The controller verifies effective cgroup memory limits, source identities
before locking, after locking, after every measurement, and before completion.
Exact input hashes are checked against the manifest. All preparation, preflight, warmup,
and measurements fit within the source deadline: `min(14400,90+degree//80)`
for SoftwareX solver-derived rows and `min(14400,120+degree//80)` for kernel-
derived rows. Other source workloads have a four-hour ceiling unless their
source defines a lower cap. Lock wait time is excluded. Child process groups
are terminated on timeout or interruption, with the last phase retained. An
interrupted lane retains its attempted result and cannot publish a completion
marker for unattempted rows.

Source accuracy requests are 1e-12 through degree 10000 and 1e-10 above it.
The public Rust policy uses one eighth of response tolerance for completion.
Completion-only benchmark calls set that policy field to eight times the source
completion tolerance, preserving the exact source completion threshold; the
independent preflight remains at the original source tolerance. Full-pipeline
calls keep the original response tolerance and therefore use a stricter internal
completion threshold. Both thresholds are explicit in the manifest.

`manifest.json`, `source-identity.json`, exact fixtures, logs, phase files,
`results.jsonl`, and per-row Criterion raw samples are the evidence. A build or
discovery failure stops the lane before a completion marker. `completion.json`
requires exactly one terminal result for every expected lane row. Complete
Rust-lane accounting is distinct from completion of the entire source inventory.

## Verification

```sh
python3 -m unittest discover -s benchmarks/replication -v
python3 benchmarks/replication/check_discovery.py /path/to/replication-binary
cargo test -p quest-qsp --features benchmark-support --test benchmark_support
```

Discovery checks both ordinary and ignored nextest queries with an intentionally
nonexistent fixture; it must neither read inputs nor run numerical work.

Native configure/build/execution commands must all run under the same systemd
memory limits and shared lock. Use the pinned source's installed dependencies;
missing packages are retained as configure failures. The exporter under
`native/` has not been validated when the required native dependency set is
unavailable. Missing fixture exports must never be replaced by approximate
coefficients or labeled as successful measurements.

## Memory and final accounting

When `/usr/bin/time` is installed, the controller records maximum resident set
size in KiB for the largest child process across build, discovery, preflight,
and measurement. This instrumentation wraps the nextest process and lies outside
Criterion timing. It is neither cgroup peak memory nor steady kernel allocation.
Missing, timed-out, or interrupted observations remain null. Allocation regression
checks elsewhere in the workspace are separate evidence.

The final accounting gate expects exactly five roles: native source (183 rows),
original and current canonical Rust stages (310 rows each), and original and
current numerical unitary admission (four rows each). It verifies exact workload
identities, matched operation contracts, terminal outcomes, and hashes of logs, RSS evidence,
and raw samples. The 33 additional source-contract diagnoses explain capability
gaps and add no workloads to the total of 811. A native configure failure is a
terminal prerequisite failure, never a successful timing or a mathematical
unsupported result.

`report.py` revalidates complete lanes and computes per-case medians directly
from hashed raw samples. It emits ratios only for matching successful operation
contracts. Ratios are descriptive; they establish neither aggregate speedup nor
statistical significance. The compact checked-in receipt references local raw
artifacts under `target/quality-campaign/`; earlier aborted attempts remain
separate and do not contribute to final accounting.

With the exact upstream dependencies installed, configure and build the optional
named-fixture exporter outside the upstream tree:

```sh
flock /tmp/quest-quality-build.lock \
  systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  timeout 4h cmake -S benchmarks/replication/native -B /path/to/export-build \
    -DQUEST_QSVT_SOURCE=/path/to/quest-qsvt
flock /tmp/quest-quality-build.lock \
  systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  timeout 4h cmake --build /path/to/export-build --target export_named -j4
flock /tmp/quest-quality-build.lock \
  systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  timeout 4h /path/to/export-build/export_named /path/to/named-fixtures
```

The initially blocked installed-dependency attempt is retained. The separately
authorized [devenv follow-up](../../docs/verification/2026-10-08-native-devenv.md)
built the upstream targets and exported all 17 exact inverse fixtures. See [remaining source suites](source_suites/README.md)
for their precise adapter boundaries and commands. After all final lanes finish:

```sh
python3 benchmarks/replication/report.py \
  --original /path/to/artifacts/original --current /path/to/artifacts/current \
  --output /path/to/artifacts/canonical-comparison.json
python3 benchmarks/replication/accounting.py combine \
  --lane /path/to/artifacts/native-source \
  --lane /path/to/artifacts/original --lane /path/to/artifacts/current \
  --lane /path/to/artifacts/unitary-original --lane /path/to/artifacts/unitary-current \
  --diagnostics /path/to/artifacts/source-contract-diagnostics \
  --output /path/to/artifacts/combined
```

## Recorded campaign

The [compact receipt](../../docs/verification/data/2026-10-08-workspace-quality/benchmark-completion.json),
[per-case comparisons](../../docs/verification/data/2026-10-08-workspace-quality/benchmark-comparison.csv),
and [provenance](../../docs/verification/data/2026-10-08-workspace-quality/benchmark-provenance.json)
record 811 terminal workloads and 560 successful measurements. Accounting is
complete; native performance coverage is not. The receipt preserves raw status
counts and separately interprets verified numerical admission and modeled-budget
diagnostics. An admission upper bound is not a proven violation, and a modeled
byte-budget rejection is not an observed system OOM. The executed canonical
controller snapshot and the corrected future classifier have distinct hashes.


The separate [devenv follow-up](../../docs/verification/2026-10-08-native-devenv.md)
executes the unchanged native Catch groups through `native/run_named.py` and
`native/run_source_suites.py`. Their aggregate-only evidence uses a distinct
schema. `native/reconcile.py` checks all 183 source inventory identities without
replacing the original 811-row receipt. `native/run_named_inverse.py` seals
receipt-bound source exports and runs matched original/current nextest inverse
measurements. Source group failures, unobserved timings, unsupported interfaces
and modeled resource rejections remain explicit. Run the added controller tests
with `python3 -m unittest discover -s benchmarks/replication/native -v`.
