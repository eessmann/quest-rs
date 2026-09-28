# Optimization measurement fixtures

`run_baseline.py` runs `baseline_compiler.rs` against a selected repository tree.
Choose a fresh output directory; previous evidence is never overwritten:

```sh
python3 run_baseline.py --repository /path/to/quest-rs \
  --output /tmp/quest-baseline-run --target-dir /path/to/cargo-target
```

The fixed version-1 corpus includes six-wire QFT, Pauli evolution, an alternating
QSVT oracle circuit, deterministic Clifford+T, signed controls, and branch-heavy
structured SSA. The QSVT workload represents projector phases around retained
forward/adjoint signal-oracle calls; it does not assert approximation of a
particular polynomial. The Clifford+T seed is part of the source.

There are 200 samples: five construction measurements per corpus and five per
applicable existing compiler pass or pass combination. Input clones occur before
the measured interval. Construction includes admission; existing pass timings
include binding and planning. Rust allocation counts include allocation and
reallocation requests. Retained and peak bytes are additional live requested
bytes relative to the input at the start of the sample. Allocator metadata,
native allocations, GPU memory and process RSS are excluded.

Each run preserves its exact source, source checksum, repository commit, build
log, JSONL samples, diagnostics, and `completion.json`. A run is complete only
when all 200 samples report success. Failed builds and samples retain their
records and produce a failed completion manifest. This fixture establishes the
unchanged/existing-pass baseline; additional optimization-stage and native
measurements are separate so their resource and timing boundaries stay explicit.

`run_native_deployment.py` builds a direct bridge consumer with the final-target runtime-path helper and clears loader overrides before launch. Six fresh processes explicitly select CPU, OpenMP or GPU, each with a state vector and density matrix. The witness verifies allocated-register deployment flags and every complex entry of a phased Bell state (or its outer-product density matrix), plus total probability. `readelf`, `ldd`, all per-mode logs and a completion manifest are retained. This checks deployment and native adapters; it does not measure optimization speed.

Pass `--mpi-only` to run state vectors and density matrices at two and four ranks instead. MPI rank/node/local-amplitude checks use the allocated native register; all ranks inspect the same complete complex output in collective order. This direct bridge fixture uses QuEST-owned MPI. It does not expand the high-level collective facade beyond its state-vector contract.

`run_baseline.py --fixture roadmap` selects the new-stage fixture (150 samples):
commutation proposal, terminal fusion, expansion-capable linear/parity candidates,
and combined beam across the five ideal corpora, plus QuantumFlow, structured
terminal fusion, and structured beam on the branch corpus. Completion reasons
and logical reservations are recorded separately from allocator measurements.
Ten additional samples exercise shared exact symbolic construction and binding.
The original baseline source is shared as a module; archive hashes identify the
exact harness version used. The mathematical workload remains corpus version 1.
The schedule, linear and parity rows time proposal generation; they do not
include the binding/planning work measured by baseline executable-pass rows,
so those medians are separate stage measurements, not direct speedup ratios.

`run_native_optimization.py` measures the six corpora with unchanged, existing
combined, terminal, terminal with declared reuse 100, and beam variants. Each
CPU/OpenMP/GPU mode and SV/DM kind has a fresh process. State vectors have 12 wires
and density matrices have six (4096 complex state entries each), with the same six
active wires. Register deployment must match the requested mode. Every stage
records source construction/search, five preparation samples, and five batches
of ten warm executions. GPU work completes and MPI barriers finish through
`sync_quest_env` before stopping execution timers. Native resource admission is
reported separately from requested Rust allocator bytes. Complete complex outputs
from zero and plus inputs are compared against the original circuit before warm
execution, with an explicit 1e-10 tolerance; this is a numerical differential
check, not independent ideal certification. Final probability drift is bounded
by 1e-9. Search status includes exhausted results. A mode expects 330 measurement
rows, and its completion manifest retains all errors, timeouts, and missing rows.

Use `run_native_optimization.py --mpi-only` for two/four-rank coherent state-vector
measurements. The five ideal corpora together produce 275 samples per rank; the
runner retains every rank and emits maximum-rank elapsed times for each sample.
The current collective facade cannot execute structured SSA or density matrices,
so those optimizer benchmarks are explicitly excluded; the separate direct
bridge MPI witness above covers density execution. MPI timings include collective
facade admission/dispatch and completion barriers. Each candidate is checked
against the unchanged stage's one-qubit probabilities on zero and plus inputs;
these marginals cannot certify full complex amplitudes or global phase. The
separate direct bridge MPI witness checks complete complex output. The model
leaves unproved communication unknown and may retain unchanged circuits as
unscorable.

`summarize.py --compiler RUN --native RUN --output FRESH.json` computes medians
while preserving each run's completion manifest and failed/exhausted records.
It emits comparative medians and reuse break-even only for campaigns with
complete measurement keys and successful timing rows. Failed or partial groups
retain their raw rows without performance comparisons. An admitted optimizer
best result stopped by a deterministic limit remains a valid completed timing
sample, and its stop reason is shown beside the measurements. Break-even uses
exact rational arithmetic before rounding to an integer reuse count.
Reuse break-even includes incremental source/search and preparation time divided
by the measured positive per-execution saving, rounded upward. A null break-even
means this run observed no positive execution saving; it is not a statistically
established performance regression. These small fixed-sample experiments are
reproducible local measurements rather than cross-machine performance claims.

`run_baseline.py --fixture workers --worker /absolute/quest-optimizer-worker`
adds 100 optional-worker samples. It compares baseline and expanded ZX proposals,
then a small worker-enabled beam (width two, two rounds, sixteen candidates, two
requests) on each ideal corpus. Separate exact one/two-wire and approximate
dyadic/rational-pi/affine-pi MITM targets use depth four and 2048 states; all other
MITM/process limits retain their defaults. Every candidate is parent-certified.
No-candidate, incomplete, exhausted, unresolved and process failure remain
separate records. This fixture measures bounded proposals, not complete search
coverage. Build the worker with `--features synthesis,zx,mitm` and retain its
binary hash with the campaign.
