# Optimization roadmap validation and measurements

The exact mathcore foundation and optimizer passed the workspace checks and
native execution measurements below. Performance is mixed: terminal fusion is
useful on several workloads, the prescribed Native V1 score misranks small CPU
QFT windows, and default beam limits often stop before the existing combined
passes' result. These measurements do not establish a universal speedup.

See the [implementation map](2026-09-28-optimization-roadmap-implementation.md),
[migration guide](2026-09-28-optimization-roadmap-migration.md), and
[correctness regressions](2026-09-28-optimization-roadmap-review.md).

## Environment and evidence

The [data directory](data/2026-09-28-optimization-roadmap/README.md) retains
numerical samples, completion status, environment metadata and summaries.
The tables below use measurements after the cache-dependent binding-work
accounting fix. Earlier samples remain separately labeled and do not establish
performance for the corrected implementation. Reproduction uses the maintained
[measurement fixtures](fixtures/optimization-roadmap/README.md).

Linux GNU x86-64; AMD Ryzen 9 7950X (32 logical CPUs); NVIDIA RTX 4080 (16 GiB,
compute 8.9), driver 615.71.09; Rust nightly-2026-09-06 (`f248f4038`), GNU 16.2.1,
CMake 4.3.0, MPICH 5.0.1 and the installed QuEST 4.3.0 fork. Serial HDF5 came from
the existing local installation recorded in the environment metadata.
OpenMP used four threads. Native processes ran outside the sandbox; direct
binary launches cleared `LD_LIBRARY_PATH`, `LD_PRELOAD` and `LD_AUDIT`.

## Validation

| Check | Result |
| --- | --- |
| Workspace all-feature locked build | Passed |
| Frozen-source workspace all-feature Nextest | 789 passed, 3 explicitly ignored scale tests skipped |
| Separate workspace all-feature doctests | 58 passed, 1 ignored application build-script example |
| Workspace formatting and strict all-target/all-feature Clippy (`-D warnings`) | Passed |
| Generated binding freshness | Passed |
| Circuit and worker without default features; workers separately with synthesis, ZX and MITM | Passed |
| CLI without default features; QSP certification/offline synthesis; IO HDF5; facade MPI | Passed |
| Standalone mathcore exact-only tests | 17 passed |
| Standalone mathcore legacy plus exact tests | 51 passed |
| Standalone fork formatting and exact-only normal dependency tree | Passed; only bigint/rational/traits/integer dependencies |
| Public `optimize` and `optimize_workers` examples | Built and executed successfully |
| Measurement harness regressions | 7 passed |
| Independent direct, facade, wrapped and renamed consumers | All four passed numerical execution, RUNPATH and dependency resolution |

The skipped tests are the QSP degree-8105 parallel acceptance test, degree-8105
dense catalog certification test, and degree-8192 interval-FFT scale test. They
remain unrun by this final normal suite. The ignored doctest is the facade's
application `build.rs` snippet; downstream consumer tests exercise that usage.
No remote CI or non-Linux platform run is claimed.

## Final campaign completion and measurement boundaries

| Campaign | Recorded outcome |
| --- | --- |
| Historical compiler baseline at `a9bafa4` | 200/200 successful samples |
| Current unchanged/existing compiler passes | 200/200 successful samples |
| New compiler stages and shared symbolic construction/binding | 150/150 successful samples |
| Optional worker proposals | **Failed campaign:** 90 successful rows, 10 unsupported-QFT ZX declines |
| CPU/OpenMP/GPU × state vector/density matrix | 6 complete configurations, 1,980 successful timing rows |
| Two/four-rank MPI state vectors | 550 + 1,100 successful raw rank rows; 275 maximum-rank rows per configuration |
| Separate native bridge deployment witnesses | CPU/OpenMP/GPU SV/DM and two/four-rank SV/DM all passed full-complex checks |

The tables and performance discussion use attempt 2 after the cache-accounting
fix; attempt 1 samples are retained separately. Every beam group now has identical logical work
and completion status across its five samples. The first worker campaign had
reported different signed-control stop reasons for a cold and warmed source.

Each compiler group contains five samples. Native source construction/search is
one sample per stage; preparation has five, and warm execution has five batches
of ten completed executions. Local state vectors have 12 wires and density
matrices have six, both 4,096 complex entries with six active wires. Compare
within deployment and register kind. GPU timers include completion. MPI warm
timers include barriers; search/preparation maxima are per-rank observations,
not a synchronized end-to-end wall path. Fixed small samples describe this
machine and are not significance tests.

The local native optimizer fixture compares complete complex outputs from zero
and plus inputs with tolerance `1e-10` and final probability drift `1e-9`. MPI
facade comparison checks one-qubit marginals on zero and plus inputs; it cannot
certify correlations or global phase. Full-complex MPI bridge checks are separate.
The collective facade lacks structured-program and density execution, so those
MPI optimizer timings are unavailable. Native numerical comparisons supplement,
but do not replace, the independent exact and interval certificates.

## Compiler construction and retained storage

The historical source hash differs from the current harness: reusable measurement
and width helpers were added while preserving the version-1 mathematical corpus.
These are before/after source comparisons, not repeated runs of identical source.
Allocation counts cover requested Rust allocations/reallocations; retained bytes
are additional live requested bytes at the sample boundary. Native allocations,
allocator metadata, GPU memory and RSS are excluded.

| Construction corpus | Median µs before → after | Allocations before → after | Retained bytes before → after |
| --- | ---: | ---: | ---: |
| qft6 | 9.05 → 16.39 | 118 → 208 | 15,584 → 21,656 |
| pauli_evolution6 | 38.51 → 62.64 | 781 → 1,021 | 123,280 → 114,896 |
| qsvt_oracle6 | 38.97 → 50.52 | 422 → 539 | 18,768 → 22,704 |
| clifford_t6 | 32.23 → 31.33 | 739 → 739 | 117,032 → 80,168 |
| signed_controls6 | 31.55 → 82.63 | 449 → 1,025 | 63,696 → 113,616 |
| branch_heavy6 | 1127.09 → 1127.21 | 21,712 → 21,712 | 547,264 → 547,264 |

Exact affine storage and retained conversion obligations add construction cost on
angle-heavy inputs; the Clifford+T payload is smaller. There is no blanket
compactification claim. Existing combined-pass allocation counts changed from
1,115 to 513 (QFT), 7,076 to 5,556 (Pauli), 899 to 386 (QSVT), 5,827 to 6,211
(Clifford+T), 5,039 to 784 (signed controls), and 17,062 to 19,888 (branch-heavy).
These counts describe the whole pass boundary, not one isolated algebra change.

The shared symbolic fixture builds six occurrences from twelve shared doublings:
construction median 126.101 µs, 196 allocations, 6,381 retained bytes and 8,509
peak additional bytes; binding median 6,442.173 µs, 1,241 allocations and 7,816
peak additional bytes. Source obligations still require checked traversal; shared
storage does not imply constant-time binding.

## New compiler stages

Medians below are µs. Scheduling, linear and parity rows measure proposal
creation; they omit the binding/planning work included by executable existing-pass
rows and therefore are not direct speedup comparisons.

| Corpus | Schedule | Linear candidate | Parity candidate | Terminal | Beam | Beam completion |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| qft6 | 7.90 | 3.34 | 0.14 | 107.65 | 131.38 | Complete |
| pauli_evolution6 | 122.44 | 99.25 | 27.96 | 1806.01 | 1870.00 | WorkLimit |
| qsvt_oracle6 | 4.44 | 3.59 | 7.03 | 24.08 | 1189.12 | RoundLimit |
| clifford_t6 | 110.54 | 88.47 | 28.39 | 1372.81 | 1529.52 | WorkLimit |
| signed_controls6 | 58.84 | 12.26 | 0.21 | 437.01 | 685.22 | GeneratorLimit |

Branch-heavy QuantumFlow took 188.121 µs, structured terminal fusion 3,803.879 µs,
and the structured optimizer 8,528.814 µs (`WorkLimit`). Algebraic ideal beam
samples consumed 1,316,605–10,000,000 logical work units with reported retained
reservations of approximately 1.86–4.44 MB; these reservations are not measured
RSS. Work/round exhaustion returned admitted results and is not search completion.

All ten optional-worker failures are QFT baseline/expanded ZX requests. A
controlled phase enters a controlled T sequence that the pinned ZX capability
explicitly rejects. The beam records that decline and retains an admitted
result; no unverified replacement is accepted. The failed campaign's summary
contains raw rows and suppresses comparative medians and speedups. The other
worker rows include certified exact MITM one/two-wire candidates, certified
rational-pi and affine-pi approximate candidates, and `NoCandidate(115)` for the
dyadic 0.3-radian target at depth four and epsilon 0.2. Outcomes repeated across
all five samples. The worker beam completion reasons and logical work are recorded per sample;
this corpus does not demonstrate exhaustive worker search.

## Prepared execution and reuse

The table compares warm execution time to unchanged execution on the **same**
deployment. Each cell is `existing combined / terminal / beam`; values below one
are lower measured time. The terminal policy uses expected executions one.

| Corpus | CPU SV | OpenMP SV | GPU SV |
| --- | ---: | ---: | ---: |
| qft6 | 2.671 / 2.776 / 2.698 | 1.005 / 1.012 / 1.008 | 0.575 / 0.579 / 0.577 |
| pauli_evolution6 | 0.633 / 0.646 / 1.002 | 0.264 / 0.267 / 0.994 | 0.190 / 0.229 / 1.005 |
| qsvt_oracle6 | 0.997 / 1.003 / 1.000 | 0.996 / 0.845 / 0.850 | 0.992 / 0.992 / 0.994 |
| clifford_t6 | 1.478 / 1.584 / 1.004 | 0.657 / 0.706 / 1.005 | 0.377 / 0.405 / 1.001 |
| signed_controls6 | 0.029 / 0.543 / 0.271 | 0.012 / 0.250 / 0.120 | 0.037 / 0.169 / 0.083 |
| branch_heavy6 | 0.362 / 0.623 / 0.625 | 0.382 / 0.498 / 0.495 | 0.381 / 0.396 / 0.392 |

| Corpus | CPU density | OpenMP density | GPU density |
| --- | ---: | ---: | ---: |
| qft6 | 3.308 / 3.323 / 3.326 | 1.187 / 1.090 / 1.096 | 0.846 / 0.844 / 0.847 |
| pauli_evolution6 | 0.743 / 0.762 / 0.988 | 0.375 / 0.377 / 0.986 | 0.343 / 0.373 / 0.996 |
| qsvt_oracle6 | 1.001 / 1.002 / 0.998 | 0.995 / 0.993 / 0.981 | 1.001 / 0.997 / 0.996 |
| clifford_t6 | 1.756 / 1.879 / 1.004 | 0.883 / 0.952 / 1.014 | 0.635 / 0.679 / 1.012 |
| signed_controls6 | 0.036 / 0.662 / 0.330 | 0.016 / 0.311 / 0.153 | 0.049 / 0.293 / 0.141 |
| branch_heavy6 | 0.355 / 0.708 / 0.709 | 0.369 / 0.608 / 0.604 | 0.365 / 0.457 / 0.458 |

QFT is a concrete Native V1 cost-model limitation: terminal fusion changes CPU
state-vector warm time from 106.348 to 295.169 µs and CPU density time from 170.039
to 565.046 µs, although the dimensionless score admits the plan. GPU state-vector
QFT improves from 157.641 to 91.206 µs but median preparation rises from 12.260 to
712.144 µs. Its measured setup-inclusive reuse break-even is 11 executions.
This prescribed model is a deterministic selection objective, not a calibrated
wall-time guarantee.

The existing combined pass remains materially better than the bounded beam on
signed controls: CPU SV warm times are 96.458 µs versus 888.604 µs, and GPU SV
110.071 µs versus 247.187 µs. Both improve over unchanged execution, but the new
beam is not a replacement for choosing and measuring existing passes. Pauli and
Clifford+T beams often exhaust work and keep near-original execution; terminal
fusion alone is more useful on those GPU/OpenMP samples.

For the admitted signed-control beam, CPU SV search/preparation/warm time is
1,342.767/117.651/888.604 µs and GPU SV is
1,362.618/1,863.649/247.187 µs. The measured setup-inclusive break-even is one
execution in both configurations. Branch-heavy beam break-even is 7 CPU SV
executions and 17 GPU SV executions. Near-unity ratios on unchanged QSVT/MPI
plans reflect measurement variation and are not evidence of optimizer gain.

The full machine-readable summary includes all stages, preparation, search,
warm execution, allocation/storage observations, and reuse-100 results. Reuse-100
changes the cost objective's declared execution count, not the ten-execution
measurement batch. Break-even is
`ceil(max(0, incremental search + preparation) / positive per-run saving)`;
a null value means no positive saving was observed. A numerical break-even for
an unchanged or unscorable plan is not an optimization recommendation.

MPI terminal changes were conservatively `Unscorable` whenever their ordered
unknown communication component changed. QSVT terminal rows were `Complete`
without useful rewriting. MPI beam statuses include `WorkLimit`, `RoundLimit`
and `Unscorable`. The measured near-unity MPI timings do not establish a
communication optimization or scaling improvement.

## Remaining boundaries

- Structured worker beam search is explicitly skipped; structured optimization
  combines QuantumFlow, static exact cleanup and transactional terminal fusion.
- The exact mathcore extension is affine only. Its unchanged legacy CAS remains
  approximate and was not claimed to be repaired.
- Local approximation certificates do not become global bounds without supported
  composition; they exclude native floating-point execution error.
- ZX controlled-T support, global phase teleportation, unrestricted symbolic CAS,
  globally optimal synthesis, and complete search coverage are not claimed.
- The default aggregate limits visibly constrain these beam corpora. Native V1
  calibration and candidate-order/budget tuning should be measured separately;
  these measurements used the specified score and limits without fitting
  them to the benchmark.
