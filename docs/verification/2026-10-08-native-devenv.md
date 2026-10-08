# Native benchmark follow-up using devenv

This follow-up uses the existing locked `quest-qsvt` devenv after the user
explicitly authorized it. It preserves the earlier installed-dependency
[811-row campaign](data/2026-10-08-workspace-quality/benchmark-completion.json)
as a historical attempt. Those earlier Rust timings and dependency failures
are not overwritten or counted twice.

## Environment and build

The unchanged pinned source is `4fc35983138d07a990862a4d83ad16f2b737c98f`.
Both lockfiles and the upstream checkout remain unchanged. The already cached
shell was entered offline with remote Nix builders disabled. All native
configuration, compilation and execution use the requested systemd memory
scope, and heavy work is serialized with the shared local lock.

The fresh native Release build uses GCC 15.3.0, HPX 1.11.0, Catch2 3.15.3,
MPICH 5.0.1 and serial HDF5 1.14.6. The local devenv selects QuEST with
CUDA, cuQuantum, MPI and subcommunicator support. Compiled capabilities do
not establish which backend QuEST automatically chooses for a particular
small workload. Runtime uses the environment's existing `quest-with-nvidia`
wrapper. This native environment is separate from the installed-dependency
Rust acceptance environment.

All four selected upstream targets built successfully: `unit_tests_nlft`,
`critical_point_benchmarks`, `unit_tests_poly` and `circuit_benchmarks`.
The selected NLFT fixture checks passed 23 assertions in six cases. Circuit
runtime initialization and Catch reporter discovery passed.

The [build/export receipt](data/2026-10-08-workspace-quality/native-devenv/build-export-receipt.json)
records the original artifacts. The separate exact-input exporter initially failed because its adapter did
not link the supported `quest_qsvt::transforms` target used by the upstream
fixture tests. Declaring that dependency fixed compilation. No upstream
include paths, private QuEST interfaces or numerical algorithms were changed.
The exporter transports exact inverse scattering pairs. It explicitly declines
the separate named-solver contract, whose source type and solver route are not
represented by this Laurent-only input schema. The unchanged native solver
benchmarks are executed separately.

## Measurement contracts

Four unchanged Catch selections expose 26 named NLFT workloads. Five further
selections expose five root-isolation workloads, two roots-of-unity workloads,
four unitary dimensions, twenty circuit stages, and the original degree-8105
coordinate preparation with three samples. Individual loop workloads have no
upstream selector. A four-hour ceiling therefore applies to each complete
selectable test case, including setup and its enclosed benchmarks. Unreached
workloads after a failure remain unattempted.

Catch runs request ten samples, 100 ms warmup and a 95 percent confidence
interval. The installed XML reporter exports aggregate timing statistics,
not raw sample arrays. These native summaries are distinct from Criterion
samples collected by actual `cargo nextest bench`. The coordinate fixture
prints its original three nanosecond samples. Its additional requested
ten-sample protocol is unsupported by unchanged upstream. Source assertions
remain the validation authority; some timed upstream operations return a
result without asserting success on every measured iteration.

The frozen SoftwareX runner requires `--pipeline solver|kernel` and `--repeat`.
The pinned current executable instead exposes five `--scope` contracts and
attested input bundles. Independent source review confirmed that no unchanged
supported executable provides the original timing boundary. All 124 legacy
solver/kernel rows therefore receive explicit capability diagnoses. Surviving
numerical helper functions and newer scopes do not establish equivalence to
that original executable contract.

## Results

All nine selected native test-case processes reached terminal outcomes. The
separate [183-row receipt](data/2026-10-08-workspace-quality/native-devenv/native-completion.json)
accounts for these observations and the unavailable interfaces; it does not
claim complete successful performance coverage. The
[per-case observations](data/2026-10-08-workspace-quality/native-devenv/native-observations.csv)
retain raw status and its scoped diagnosis.

| Native observation | Rows |
| --- | ---: |
| Timing summaries from passing source test cases | 51 |
| Original coordinate preparation with three raw samples | 1 |
| Earlier timing summaries retained from the failed random-solver test case | 5 |
| Laurent solver preflight failed before timing | 1 |
| Unavailable SoftwareX executable boundaries | 124 |
| Unavailable coordinate ten-sample protocol | 1 |

All 17 native inverse workloads passed, including order 1,000,000. All three
industrial solvers and all 32 additional executable source workloads passed.
The random-solver group measured monomial, Chebyshev, Hermite, Laguerre and
Jacobi cases, then failed `REQUIRE(preflight.has_value())` for Laurent at
`benchmark_fixture_tests.cpp:387`. The source test does not print the underlying
error. Its five earlier summaries remain observable, but the final whole-group
assertion gate failed. The original recorder's five `accuracy_failure` statuses
mean conservative group invalidation, **not five proven numerical accuracy
failures**; its Laurent `unattempted` status refers to timing, not preflight.
Derived diagnoses preserve this distinction without rewriting execution logs.

The coordinate samples were 8,928,650, 9,050,542 and 9,011,102 ns. Its reported
4,748,068 bytes of modeled prepared host storage is separate from measured RSS.
No source tolerance, iteration limit or native algorithm was changed.

The exporter produced all 17 exact inverse pairs in 497.86 seconds, with
497,036 KiB maximum child-process RSS. Original and current Rust then ran
`cargo nextest bench` on the same hash-verified byte snapshot. Each lane has
13 successful ten-sample Criterion measurements through order 50,000 and four
modeled resource-limit rejections at 100,000, 200,000, 500,000 and 1,000,000.
The order-100,000 rejection requested 281,018,368 modeled bytes against
149,261,440 remaining admitted bytes during preflight. This is neither an OOM
nor evidence of whole-machine capacity. Budgets were not raised.

Rust also checks forward-roundtrip coefficients outside timing, and exported
reflection coefficients where present. This is stronger than the native
low-order fixture's finite, size-preserving inverse preflight. Only matching
original/current Rust medians are compared. Native Catch summaries are not
substituted for raw Criterion distributions or used for cross-language ratios.
The [17-case comparison](data/2026-10-08-workspace-quality/native-devenv/named-inverse-comparison.csv)
contains 13 paired medians. Observed original/current ratios range from
0.9839 to 1.0109; these descriptive observations establish neither aggregate
speedup nor statistical significance. The
[Rust completion receipt](data/2026-10-08-workspace-quality/native-devenv/named-inverse-completion.json)
and [provenance](data/2026-10-08-workspace-quality/native-devenv/provenance.json)
identify inputs, controllers and original artifacts. The prior Rust production
source identities and benchmark bytes remain intact; all 925 files in the
existing workspace acceptance manifest still match. No Rust production code
changed during this follow-up.

Independent final audit reproduced the 183-row native accounting and every
published CSV row, checked source/controller/executable identities, all 17
exact-bit inputs, all raw Criterion sample hashes, accuracy records and RSS
sidecars. All 27 new native-controller tests passed. The original execution
artifacts remain under `target/quality-native-devenv/` and
`target/quality-campaign/`; compact receipts contain generic paths only.

## Reproduce

From the pinned upstream checkout, configure into a fresh external directory:

```sh
systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  flock /tmp/quest-quality-build.lock \
  env TMPDIR=/path/to/disk-backed-tmp \
  devenv --clean TMPDIR --offline --max-jobs 1 --cores 4 \
    --nix-option builders '' --no-tui --no-reload shell -- \
    cmake --preset nix-release -S /path/to/quest-qsvt \
      -B /path/to/native-build -DBUILD_TESTING=ON \
      -Dquest_qsvt_ENABLE_QUEST=ON -Dquest_qsvt_ENABLE_BENCHMARKS=ON \
      -Dquest_qsvt_ENABLE_DOCS=OFF
```

Use the same wrapper to build the four targets with `cmake --build
/path/to/native-build --target unit_tests_nlft critical_point_benchmarks
unit_tests_poly circuit_benchmarks -j2`. The benchmark controllers acquire
the shared lock themselves; do not hold another copy around their invocation:

```sh
systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  env TMPDIR=/path/to/disk-backed-tmp \
  devenv --clean TMPDIR --offline --max-jobs 1 --cores 4 \
    --nix-option builders '' --no-tui --no-reload shell -- \
    python3 /path/to/quest-rs/benchmarks/replication/native/run_source_suites.py \
      --source /path/to/quest-qsvt --build /path/to/native-build \
      --output /path/to/new-source-results --launcher quest-with-nvidia
```

Invoke `run_named.py` with the same arguments and a separate new output
folder for named NLFT cases. Offline shell entry requires the locked
shell's dependencies to be present in the local Nix store.

The exact-input Rust follow-up uses `native/run_named_inverse.py prepare`
with `--exports`, `--output`, `--exporter`, `--export-log` and
`--export-receipt`. The completed export receipt records the pinned source
revision, actual exit status and SHA-256 for the executable, log and each
`named-fixtures/orderN.json`; only an exit-zero export is admitted. The input
bundle copies and verifies those artifacts before either Rust lane starts.

Run its `run` command inside the same memory scope with `--bundle`,
`--workspace`, `--target`, `--output`, `--implementation original|current`
and `--reference-lane`. The reference lane must contain the previously
verified source identity for that role. Use separate original/current build
and output directories. This command uses the installed Rust environment;
devenv supplies the native reference dependencies only. The controllers own
the serialization lock, so no outer `flock` is needed.
