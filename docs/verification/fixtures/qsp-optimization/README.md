# Staged QSP optimization measurements

The active launcher requires Python 3.11+ and `tomli-w` to serialize scratch
Cargo manifests. From the repository root, install its declared dependency in
an isolated environment:

```sh
python3 -m venv /tmp/quest-measurement-tools
/tmp/quest-measurement-tools/bin/python -m pip install -r docs/verification/fixtures/requirements.txt
```

Use that environment's Python for the launcher and its Python regression tests.

Requires Python 3.11+, the repository's Rust toolchain and GNU `/usr/bin/time`.

This fixture compares immutable source snapshots of baseline, completion-only A,
A+B (root transfer pruning), and A+B+C (shared transforms). It uses scalar
binary64 at degrees 256/1024 and offline synthesis at degrees 16/256 with fixed
128/256-bit computation and certification. Every workload checks the response;
offline results must independently certify, and repeated exports must agree.

Build each variant before running measurements. Do not run competing builds or
benchmarks during the trial phase. Use the same recorded compiler/environment
for all variants:

Each variant builds in its own `LABEL/build` target directory. Cargo artifacts
are never shared across snapshots. `toolchain.txt` is queried from the copied
source directory, and `cargo-config.json` records applicable home/ancestor Cargo
config files and their build, target, and unstable tables, plus compiler/target
environment overrides. This includes target-specific `target-cpu=native` flags.

```sh
/tmp/quest-measurement-tools/bin/python run.py build baseline /path/to/baseline /path/to/observations
/tmp/quest-measurement-tools/bin/python run.py build a /path/to/a /path/to/observations
/tmp/quest-measurement-tools/bin/python run.py build ab /path/to/ab /path/to/observations
/tmp/quest-measurement-tools/bin/python run.py build abc /path/to/abc /path/to/observations
/tmp/quest-measurement-tools/bin/python run.py run /path/to/observations baseline a ab abc
```

The driver copies each input into a private source tree. Because offline
completion is private, it injects an identical, benchmark-only stopwatch and
work-counter hook around that call. Registered begin/end callbacks measure its
allocations without resetting the enclosing end-to-end counters; an RAII guard
closes the phase on both success and error. Binary64 hooks observe completion,
inverse, and independent response-reconstruction convolution work.
The production checkout and its public API are unchanged. Original and
instrumented source fingerprints, fixture hashes, build logs, binary hashes,
individual trial exit statuses, and `/usr/bin/time -v` observations are retained.
The post-build `build-identity.json` includes instrumented source, resolved
observer package/lockfile, and binary hashes. Before any trial, all variants must
match that identity and their original `sources.json`/`binary.json` build records.
The same check runs after measurements. All three trials interleave variants in
forward/reverse order. Each trial must contain exactly the six expected successful
workloads. The driver compares their export fingerprints and accepted grids
across every variant and trial. Missing, duplicate, failed, or mismatched records
make `workloads_valid` false in `measurement-completion.json`; exit statuses and
source identity are separate mandatory gates. Existing completion/trial receipts
cannot be overwritten; use a new observation directory for a fresh campaign.

JSON lines report completion and end-to-end duration separately, accepted grids,
work units where observed, response checks and output fingerprints. Binary64
completion timings exclude synthesis; its end-to-end timing starts from an
admitted target. Offline end-to-end timings include source admission, synthesis,
export and independent certification. Offline completion timings exclude these
other stages. Fixed precision precludes silently timing extra successful retries.
Durations and allocation counts are totals over the reported `iterations`;
charged work counts describe one solve. Binary64 `synthesis_work_units` sums
inverse and response-reconstruction convolution work, and `work_units` adds
completion work. Offline `work_units` is the existing solve-attempt computation
counter; independent certification has its own resource accounting. The two
backends' stage boundaries and work-unit definitions differ, so compare variants
within a workload rather than treating these as cross-backend operation counts.

Allocation counts include Rust allocations/reallocations. Peak additional live
bytes exclude baseline live storage, allocator metadata, stacks, native
allocation, and transient overlap within realloc. These counters add overhead to
allocation-heavy paths equally. They do not measure the whole process memory
quota; the process RSS observation is separate. Timings include fixture checks.
Both backends report `completion_allocations` and
`completion_peak_extra_live_bytes` in addition to the outer sample. Offline
completion peaks are measured relative to storage already live on entry to
completion; its nested phase never resets the enclosing sample's peak. The
observer and phase hooks require sequential execution.

A failed workload is emitted as an error record and makes the trial fail. A
failed build, timeout, interruption, or changed source is not a speedup result.
Timeout/interruption cleanup escalates to SIGKILL for the entire process group
even if `/usr/bin/time` exits before a surviving child.
The earlier binary64 full-ratio completion undercounted one FFT; compare charged
work with that accounting correction in mind, not as an instruction count.

Lightweight controller and allocator checks (no Cargo builds):

```sh
/tmp/quest-measurement-tools/bin/python -m unittest discover -s . -p test_run.py
rustc --edition=2024 allocator_probe.rs -o /tmp/qsp-allocation-probe
/tmp/qsp-allocation-probe
```

After the four-variant, three-trial campaign completes successfully, generate
its summary without rerunning observers:

```sh
python3 summarize.py /path/to/observations
/tmp/quest-measurement-tools/bin/python -m unittest discover -s . -p test_summarize.py
```

`summarize.py` requires all three campaign gates to be true and exactly 72
validated workload records for `baseline`, `a`, `ab`, and `abc`. It independently
checks every trial exit receipt, the six expected records, and matching
fingerprints/grids across all variants and trials. Invalid or incomplete data
is rejected before summary files are written.

The generated `summary.json` contains per-workload/per-variant median, min, and
max for completion and end-to-end timing, allocations, peak additional live
bytes, and charged work. Timings and allocation counts are per invocation;
peak bytes and charged work are never divided by the iteration count. Offline
completion getters already describe one solve. `results.md` presents median
timing and resource tables, with min/max retained in JSON. The summary includes
hashes of its input receipts and leaves all raw campaign files unchanged.
