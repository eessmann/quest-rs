The remaining source inventory contains 33 workloads: five root-isolation cases,
two roots-of-unity cases, twenty circuit stage cases, four certified unitary
admissions, and two coordinate-preparation protocols. `run.py source-blocked`
retains each row separately when an actual unmodified native configuration fails
for the required `autodiff` 1.1.2 package. It verifies the pinned source revision,
hashes the source files and failure log, and records the precise Rust contract
that cannot substitute for each source workload. This produces complete failure
accounting, with `performance_coverage_complete=false`. These 33 rows are a
diagnostic subset of the overall native inventory; the combined campaign counts
each native workload once and attaches these contract diagnostics separately.

The four Rust unitary measurements use the source's exact identity matrices of
dimensions 64, 128, 256, and 512, and require a zero Gram residual. Their measured
boundary is `NumericalOperator::from_view` followed by `admit_unitary`. They are
separate comparisons: numerical Gram admission does not reproduce the C++
outward-certified proof or its scalar-work counters. The adapter constructs the
matrix and checks the residual before timing. Failed admissions abort the
measurement. Discovery does not construct a matrix, read an input file, or run a
numerical kernel.

Run each baseline/current lane from an immutable source tree, with separate
target and result directories. Copy only the `source_unitary.rs` benchmark and
its `[[bench]]` manifest entry into the original checkout. Preserve its production
code and record every instrumentation change. The controller hashes the Rust
source before each workload and after each measurement, refusing to publish a
complete lane when the measured tree changes.

Capture the actual controller and its imported helper before launch, then execute
the captured copy. Keep these snapshots unchanged for both lanes:

```sh
python3 - <<'PY'
import hashlib, json
from pathlib import Path
source = Path("benchmarks/replication")
snapshot = Path("target/unitary-controllers")
(snapshot / "source_suites").mkdir(parents=True, exist_ok=False)
files = {}
for name in ("source_suites/run.py", "campaign.py"):
    content = (source / name).read_bytes()
    (snapshot / name).write_bytes(content)
    (snapshot / name).chmod(0o444)
    files[name] = hashlib.sha256(content).hexdigest()
pin = {"files": files, "sha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()}
(snapshot / "controller-pin.json").write_text(json.dumps(pin, indent=2) + "\n")
PY

systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  env TMPDIR="$PWD/target/quality-tmp" CARGO_BUILD_JOBS=4 \
  python3 target/unitary-controllers/source_suites/run.py unitary \
  --workspace /path/to/immutable/quest-rs \
  --target "$PWD/target/unitary-original" \
  --output "$PWD/target/campaign/unitary-original" \
  --implementation rust_original \
  --controller-pin "$PWD/target/unitary-controllers/controller-pin.json"
```

The runner verifies the pinned controller/helper hashes before each case, after
each measurement and before publishing completion. Every lane retains the pin
as `controller-identity.json`; any controller drift prevents publication.

The controller serializes builds and measurements using the campaign lock and
invokes actual `cargo nextest bench`. Every successful row retains ten or more
Criterion samples, their hashes, the requested and achieved residual, and the
effective memory limits. Its four-hour deadline includes build, preparation,
preflight, warmup, and measurement; time waiting for the shared lock is excluded.
GNU `time` separately records the largest child process peak RSS across all
phases. This is neither aggregate cgroup memory nor kernel allocation accounting;
missing or interrupted observations are `null`. The wrapper remains outside
Criterion's timed kernel.
Native failure records require the real configuration log:

```sh
systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G \
  python3 benchmarks/replication/source_suites/run.py source-blocked \
  --source /path/to/quest-qsvt \
  --log "$PWD/target/campaign/native-configure.log" \
  --output "$PWD/target/campaign/source-suites"
```

The source coordinate benchmark takes exactly three samples. Its `fixed_3` row
remains distinct from the `criterion_10` row even when both are blocked before
execution. Random Eigen coefficients are never replaced with same-seed Rust
coefficients, and source resource policies are never silently remapped to a
different Rust root-cover API.

Contract checks:

```sh
python3 -m unittest discover -s benchmarks/replication/source_suites -p 'test_*.py'
```


The later authorized [devenv follow-up](../../../docs/verification/2026-10-08-native-devenv.md)
uses `../native/run_source_suites.py` to execute the five unchanged native Catch
selections. Thirty-two workloads execute; the extra coordinate ten-sample
protocol remains unsupported. This follow-up preserves the initial dependency
failure diagnostics and the separate Rust numerical-unitary measurements.
