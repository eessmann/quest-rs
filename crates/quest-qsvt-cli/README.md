# QSP and QSVT applications

`quest-qsvt-cli` provides typed `clap` commands and a library entrypoint,
`Cli::run() -> Result<serde_json::Value>`. Only `main` installs `color-eyre`.
Successful commands print a JSON report. Admission, IO, construction and native
failures return an error and a nonzero exit status. Catalogue checks retain a
result for every selected family; any failure makes the process exit nonzero.

The default features are `native`, `hdf5`, and `certification`. Set `QUEST_ROOT`
to the installed QuEST package and `HDF5_DIR` to a serial HDF5 installation.
The final executable embeds their runtime search paths through `quest-build`.
A build with `--no-default-features` supports production synthesis and catalogue
inspection without native QuEST, HDF5, or arbitrary precision. `offline-synthesis` is an explicit
additional feature; it is never selected after a production failure.

## Synthesis and catalogue

```sh
cargo run -p quest-qsvt-cli -- synthesize \
  --input polynomial.json --output phases.json --mode real-parity-wx --certify
cargo run -p quest-qsvt-cli -- synthesize \
  --input laurent.json --output controls.json --mode unit-circle-response
cargo run -p quest-qsvt-cli --features offline-synthesis -- offline-synthesize \
  --input polynomial.json --output phases.json
cargo run -p quest-qsvt-cli -- catalog list
cargo run -p quest-qsvt-cli -- catalog check --kappa 5 --epsilon 0.1 --certify
```

Real-parity Wx input uses the IO crate's explicit polynomial bases, for example
`{"basis":"Chebyshev","coefficients":[0.0,0.25]}`. Sequence-only phase output retains the
`pyqsp-wx-symmetric` convention. Unit-circle construction requires an explicit
nonnegative Laurent polynomial, for example
`{"basis":"Laurent","coefficients":[[0.1,0.2],[0.05,-0.1]]}`.

`--algorithm rhw` (default) selects structured Half-Cholesky;
`--algorithm inverse-nlft` selects the divide-and-conquer nonlinear Fourier
inverse. Both support both response conventions and both explicit precision
routes. No solver or precision fallback occurs.

`--export compiled` is the default. It stores the exact-bit source, target,
complement and complete phase/control payload, solver/version/precision, limits,
and SHA256 digest. `--certify` also stores a historical verification receipt.
`--export sequence` explicitly exports the weaker tagged phase/control format
and cannot be combined with `--certify`. A `--no-default-features` build supports
binary64 synthesis with `--export sequence`; compiled artifacts require
`certification`. For example:

```sh
cargo run -p quest-qsvt-cli --no-default-features -- synthesize \
  --input polynomial.json --output sequence.json --export sequence \
  --algorithm inverse-nlft
```

Production admission, completion and synthesis use binary64. `--certify`
independently verifies that frozen export. `--tolerance` defaults to `1e-11`;
when certification is requested it sets all five certification tolerances.
A basis-conversion error bound is reported separately; certification of the
converted target does not silently certify the earlier conversion.
`offline-synthesize` always independently certifies its final binary64 export.

There are exactly 21 frozen catalogue families from C++ revision
`7fe7f740579b03c52a8cf48be6a31268b029c19f`. Selection requires the exact stored
`--kappa` and binary64 `--epsilon` pair. No nearby family is substituted. Omit
both selectors to list or check all 21. `check` actually constructs each target
and optionally certifies it; an epsilon label is provenance, not a certificate.

## Imported encodings and overlap

```sh
cargo run -p quest-qsvt-cli -- embedded \
  --encoding block.h5 --qsp phases.json --route standard \
  --input-state input.h5 --output-state raw.h5 \
  --normalized-output-state conditioned.h5
cargo run -p quest-qsvt-cli -- overlap \
  --encoding block.h5 --qsp phases.json --route standard \
  --input-state input.h5 --reference-state reference.h5
```

HDF5 layouts are those of `quest-qsvt-io`: `/block_encoding/{U,PiL,PiR}` and
`/state/vector`, with their required metadata. The original row/column counts
select the ordered leading logical isometry columns independently of padding.
Numerical oracle and isometry admission happens before native preparation.

The route is always explicit: `standard`, `direct`, `hermitianized-full`,
`hermitianized-even`, `hermitianized-odd`, `multiplication-even`, or
`multiplication-odd`. Standard accepts tagged symmetric or Laurent Wx phases;
the other routes accept paper-native `psi`/`phi` controls or frozen control
matrices. Polynomial files require `--synthesize-input` (binary64), or an earlier explicit
`synthesize` command. `--certify-input` separately certifies an explicitly
synthesized frozen export. Neither flag enables offline fallback. Direct
Hermitian admission and route-specific projector checks remain library checks.

Compiled inputs are recognized separately from source/sequence JSON, validated,
and independently recertified at `--input-tolerance` (default `1e-11`). Saved
receipts are not trusted as acceptance evidence. Wx artifacts undergo fresh
certification of the actual rounded projector phases/readout, retained by the
native transform. Generalized artifacts retain their certificate in a typed
route response. The explicitly chosen route transfers each absolute-order source
coefficients to `T_k(x)` for direct/Hermitianized routes or `T_k(y)`, `y=x²`, for
multiplication routes; it does not substitute a Laurent variable. The original
Laurent offset, span and coefficient bits remain attached to the route meaning.
The report records `certified_projector_payload` and `response_evidence`.
Raw imported phase/control sequences remain explicitly weaker evidence.


`embedded` preserves the supplied input mass and writes subnormalized logical
amplitudes. Optional normalized output consumes the runtime conditioning stage;
zero retained mass cannot be conditioned. The report separates initial, input,
bridge and retained mass. `overlap` requires normalized logical input and
reference vectors, performs the native real and imaginary Hadamard tests, and
reports complex overlap, absolute event masses, and normalized overlap when the
transformed norm is nonzero. Imported control files carry no target certificate.

## Physical linear solve

```sh
cargo run -p quest-qsvt-cli -- solve \
  --matrix matrix.h5 --rhs rhs.h5 --qsp reciprocal-phases.json \
  --route standard --reciprocal-scale 0.1 \
  --output-state physical-x.h5 --normalized-output-state normalized-x.h5 \
  --residual-tolerance 1e-5
```

Solve accepts a nonempty square matrix with numerical full rank. It computes
singular values with explicitly sequential faer SVD and bounded scratch, and
requires `sigma_min > sigma_max * dimension * f64::EPSILON`. It constructs the
block dilation of `A.adjoint() / sigma_max`, normalizes the right-hand side,
and applies the supplied reciprocal singular-value response. Standard requires
odd degree; the two generalized odd routes are also accepted.

`--reciprocal-scale s` is an explicit premise that the imported route response
approximates `s/x` over the singular-value domain. The CLI does not infer this
from a filename, certificate of an unrelated target, or catalogue label. Given
the subnormalized logical output `raw`, it recovers

```text
physical_x = raw * ||b|| / sigma_max / s
```

The primary HDF5 output contains `physical_x`, with its physical norm. Optional
normalized output is separate. The report independently evaluates both
`||A physical_x - b||` and that residual divided by `||b||`. If a requested
residual tolerance fails, the computed outputs and report remain available but
the process exits nonzero. Without that option, the residual is diagnostic.
Solve executes locally; distributed solve or full-state output is not advertised.

## Catalogue workers and distributed execution

Enable `rayon` and pass `--workers N` or `--workers auto` to use a caller-owned thread pool for
independent catalogue family checks. Results and errors retain catalogue order,
regardless of task completion order. Stage times are sums of worker wall times;
the total is the elapsed application wall time. Each family retains its own
resource budget, so concurrent working storage grows with the worker count.
For `synthesize` and explicit `--synthesize-input`, the same caller-owned pool
is borrowed by binary64 completion and synthesis. Dependent inverse halves stay
ordered; frozen output never retains the pool. Certification remains serial
within a target. Offline construction, SVD and native execution are not moved
onto workers. Native or MPI calls stay on the initializing caller thread.
The default is one worker. `auto` resolves the OS-reported available local
parallelism, and reports the resolved count. Distributed applications initialize
MPI and admit `MPI_THREAD_MULTIPLE` before creating any workers; only rank zero
resolves `auto` and owns the pool. Pool construction failures are agreed on all
ranks before input or native execution. The pool drops before the communicator
and MPI runtime. Application totals include MPI admission and pool creation,
also reported separately as `mpi_environment` and `worker_pool`.

```sh
cargo run -p quest-qsvt-cli --features rayon -- \
  --workers 2 catalog check --certify
cargo run -p quest-qsvt-cli --features rayon -- \
  --workers 2 synthesize --input polynomial.json --output phases.json
cargo build -p quest-qsvt-cli --features mpi
mpiexec -n 4 target/debug/quest-qsvt-cli embedded --distributed \
  --encoding block.h5 --qsp polynomial.json --route standard \
  --synthesize-input --certify-input --input-state input.h5
mpiexec -n 4 target/debug/quest-qsvt-cli overlap --distributed \
  --encoding block.h5 --qsp phases.json --route standard \
  --input-state input.h5 --reference-state reference.h5
```

MPI additionally requires an installed MPI/SUBCOMM-enabled QuEST package and an
explicit `MPICC` matching that package; the native build checks the loaded MPI
ABI/library. All ranks must invoke a matching workflow. Rank zero reads the
files and performs any requested binary64 synthesis and independent
certification exactly once. A bounded wire carries the frozen controls/phases,
complex matrix words, logical vectors, metadata and evidence. Every rank agrees
on IO/admission/allocation status before broadcast or collective native entry.
Nonroot ranks do not read the input files or run synthesis.

Distributed embedded execution appends initialized-zero high qubits to retain
native dense-operation capacity per rank; every projection explicitly fixes
those idle qubits to zero. Distributed overlap obtains the corresponding idle
capacity from its collective Hadamard preparation. Both workflows report
absolute masses; overlap also reports its complex value. Only rank zero writes
the JSON report and optional trace. Distributed state gathering, state-file
output and solve are rejected rather than approximated by local execution.

## Timing and traces

Successful execution reports `mass.native_dispatches` independently of semantic
queries and forward/adjoint source applications. Counts are native API calls per
rank, including decomposed gates, negative-control phase toggles, matrices,
projections, probability readout and Hadamard scratch operations. They are
computed from the fixed admitted schedule and published after a successful run.
They exclude setup, run admission, MPI agreement, caller initialization,
snapshots and later conditioning. Errors do not report a complete count. Native
API calls are distinct from backend kernels and MPI messages.

Local execution separates lowering/admission from native preparation. The
collective facade currently exposes combined admission/preparation, reported as
`collective_admission_and_preparation`.

Every report includes application stage wall times and their total, through
scientific output writing, optional trace serialization/writing, and owner
teardown. Local pool teardown is `worker_pool_teardown`; distributed pool,
communicator and MPI teardown is `mpi_and_worker_teardown`. Final JSON printing
is outside that total. Certification timing surrounds the entire call, including
all precision retries; individual attempts retain precision, work and elapsed
time. Offline reports retain every construction and certification attempt.

Add `--trace trace.json` for Chrome/Perfetto spans. The outer
`application_dispatch` span starts after runtime and worker admission and ends
before owner teardown; it does not claim to measure the full application total.
Failures during admitted dispatch and all internal retries are traced. Failed
runtime/worker preflight produces no trace file. Traces are
bounded and report dropped events. Timing measures wall time on the calling
process, not synchronized distributed time or independent GPU event time.

This Rust port draws on `quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`. Its MIT notice is retained in `LICENSE-quest-qsvt`.
