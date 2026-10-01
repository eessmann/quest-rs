# Architecture stage receipts

Local macOS measurements of the new `Program` lifecycle and direct Rust gate
synthesis. The raw [2026-09-30 receipts](2026-09-30-portability/) preserve each process run,
OS peak-memory report, compiler/hardware details, build log, executable hashes,
source-example hashes and lockfile hash. Existing baseline fixtures were not
changed. The [earlier receipts](2026-09-29/) and [2026-09-29 acceptance receipts](2026-09-29-final/)
are retained. The tables below use the 2026-09-30 rerun after the Linux search,
resource-accounting and compiler-selection corrections; the measurements themselves
were made on macOS. The source manifest binds them to the reviewed correction.

Local checkout paths in archived logs use generic placeholders. Numerical
measurements and recorded hashes retain their original values.

## Reproduction

From the repository root:

```sh
devenv shell -- bash benchmarks/architecture/run-local.sh benchmarks/architecture/NEW-RECEIPT-DIRECTORY
```

The script builds these executables with Cargo's `release` profile (`lto =
"thin"`) and runs `/usr/bin/time -l` on the executables themselves, after the
build has completed:

```sh
cargo build --release -p quest-rs --example architecture_runtime -p quest-synthesis --example architecture_synthesis
./target/release/examples/architecture_runtime 4   # also 8 and 12
./target/release/examples/architecture_synthesis 3 # also 6, 9 and 12
```

Recorded host: Apple M1 Pro, 10 CPU cores, 16 GiB RAM, macOS 27.0 build 26A428,
aarch64-darwin. Rust 1.100.0-nightly (`6bb1652a0`, 2026-09-22), LLVM 23.1.1.
Native QuEST 4.3.0 came from the local Nix development environment. The build log
records its store path and Clang 21.1.8 toolchain. This is a working-tree receipt;
the recorded Git HEAD predates these uncommitted implementation changes.

Each case runs in three separate processes, sequentially. The table gives the
median of the three observations. These are modest local scaling measurements,
not isolated-machine statistics, a comparison against the old implementation,
or evidence for Linux/GPU/distributed performance. Other development work was
active on the host. Process ordering is fixed rather than randomized.

## Compilation, preparation and native execution

The workload has `2*q` layers, each containing H, native arbitrary-angle
Rz(0.17), and a nearest-neighbor CNOT for each wire. The gate count equals the
prepared static-dispatch count in every receipt. Discrete synthesis is not
performed by this workload.

Compilation measures `Program::parse(...).verify()?.lower()?.plan()?` once.
Preparation measures `Environment::prepare(plan.clone())`. Warm execution
reuses that prepared program for 100 runs. The comparison prepares, runs and
drops a program on every one of 100 runs, including the cheap plan clone in
the measured interval. Both paths reset the register to zero before each timed
interval; reset and final amplitude export are excluded. Environment and
register construction are also excluded. Timings below are per run where
applicable; raw CSV files retain total nanoseconds.

| Qubits | Gates | Compile ms | Prepare ms | Reused preparation: run ms | Prepare + run + drop ms | OS peak RSS MiB |
|---:|---:|---:|---:|---:|---:|---:|
| 4 | 96 | 0.657 | 0.068 | 0.451 | 0.494 | 6.578 |
| 8 | 384 | 2.028 | 0.167 | 1.868 | 2.032 | 8.812 |
| 12 | 864 | 4.596 | 0.362 | 7.423 | 7.839 | 12.703 |

The maximum amplitude difference between the two execution paths was exactly
zero in all nine processes. The measured difference is attributable to this
specific preparation comparison; no asymptotic or general speedup is inferred.

| Qubits | Modeled plan bytes | Environment reservation for prepared program | Modeled register bytes |
|---:|---:|---:|---:|
| 4 | 373,478 | 407,270 | 1,024 |
| 8 | 1,481,510 | 1,616,422 | 16,384 |
| 12 | 3,181,160 | 3,484,520 | 262,144 |

The prepared reservation already includes retained plan storage; do not add the
plan column to it again. Each run additionally reserves the default 67,108,864
byte interpreter storage cap, released on return. That cap is an admission
ceiling, not an actual allocation or measured resident set. OS peak RSS covers
the complete process, including compilation and both execution strategies;
`*.time.txt` also records macOS peak memory footprint separately.

## Rust synthesis and independent certification

All cases synthesize the full-phase exact target Rz(pi/7), with seed 0,
256 request-owned working bits and default synthesis limits. Epsilon is the
exact binary64 value whose bits are retained in each CSV. The first timing
includes grid/norm search, exact synthesis, and both mandatory acceptance
verifiers. A fresh independent rotation-certificate replay is timed separately;
it is additional work, not subtracted from the accepted synthesis measurement.

| Epsilon | Elementary gates | T/T-dagger count | Synthesis + acceptance ms | Independent replay ms | Logical work | OS peak RSS MiB |
|---:|---:|---:|---:|---:|---:|---:|
| 1e-3 | 81 | 32 | 164.366 | 0.217 | 1,196,587 | 3.391 |
| 1e-6 | 162 | 64 | 339.554 | 0.369 | 1,217,753 | 3.469 |
| 1e-9 | 235 | 92 | 448.595 | 0.519 | 1,228,574 | 3.469 |
| 1e-12 | 297 | 122 | 580.618 | 0.665 | 1,251,191 | 3.547 |

Gate counts, work and grid exponents were identical across the three process
runs per case. Work includes conservative pre-admitted units for interval
Taylor/precision attempts and independent matrix reconstruction, so it is not
an instruction counter. No claim of optimal T-count or comparison against
newsynth execution speed is made.

The conservative matrix/output sub-reservations are 815,104; 898,048; 972,800;
and 1,036,288 bytes respectively. They are explicitly partial models, not total
heap peaks: lattice scratch and exact-ring temporaries have separate admission
checks within the 67,108,864-byte request limit. The live grid reserves its
8,388,608-byte conservative scratch model before admitting candidate work. The OS RSS column is the measured
whole-process peak. The raw receipts distinguish these quantities by name.
