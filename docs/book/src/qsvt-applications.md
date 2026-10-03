# QSVT applications

`quest-qsvt-cli` provides typed `clap` commands and a library entrypoint,
`Cli::run() -> Result<serde_json::Value>`. Only `main` installs `color-eyre`.
Successful commands print a JSON report. Admission, IO, construction and native
failures return an error and a nonzero exit status. Catalogue checks retain a
result for every selected family; any failure makes the process exit nonzero.

The default features are `native` and `certification`. Serial HDF5 is required for all builds. Set `QUEST_ROOT`
to the installed QuEST package and `HDF5_DIR` to a serial HDF5 installation.
The final executable embeds their runtime search paths through `quest-build`.
A build with `--no-default-features` supports production synthesis and catalogue
inspection without native QuEST or arbitrary precision. It still requires serial HDF5. `offline-synthesis` is an explicit
additional feature; it is never selected after a production failure.

## Feature matrix

| Build features | Available application capabilities |
| --- | --- |
| `--no-default-features` | Binary64 synthesis, catalogue list/check, traces |
| `certification` | Independent arbitrary precision verification of frozen exports |
| `native` | Local embedded transforms, Hadamard overlap, physical solve |
| `rayon` | Caller-owned catalogue and binary64 synthesis worker pool |
| `mpi` (includes `native`) | Root-driven distributed embedded and overlap, without state gathering |
| defaults | `native`, `certification` |
| `offline-synthesis` | Explicit arbitrary precision construction followed by export certification |

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

`--algorithm inverse-nlft` (default) selects the divide-and-conquer nonlinear
Fourier inverse; `--algorithm rhw` explicitly selects structured Half-Cholesky.
Catalogue checks, exports and solves accept the same algorithm flag. Both support both response conventions and both explicit precision
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

There are exactly 21 frozen catalogue families from the official
[PennyLane inverse dataset](https://pennylane.ai/datasets/inverse). The unchanged
HDF5 snapshot is embedded and decoded once per catalogue command. Loading
requires a writable temporary directory; all HDF5 handles and temporary files
are released before numerical work. Reports include the dataset source URL,
SHA-256 and retrieval metadata in `source`, replacing the C++ `source_revision`. Selection requires the exact stored
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

`--route auto` is the default: tagged Wx phases select `standard`; generalized
controls select `hermitianized-full`, retaining both logical sides. For
polynomial synthesis, nonnegative Laurent sources select generalized controls;
other bases select Wx synthesis and must satisfy its real/parity contract.
Physical solves choose `standard` for Wx and `hermitianized-odd` for generalized
controls. A failed admission never retries another route or solver. Explicit
route choices are: `standard`, `direct`, `hermitianized-full`,
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

Solve accepts nonempty square or rectangular matrices with numerical rank
`min(rows, cols)`. For rectangular systems the response approximates the
Moore-Penrose pseudoinverse; a nonzero least-squares residual is retained. It computes
singular values with explicitly sequential faer SVD and bounded scratch, and
requires `sigma_min > sigma_max * max(rows, cols) * f64::EPSILON`.
SVD and rank comparison operate on scaled entries, so intermediate overflow or
underflow does not misclassify large or tiny full-rank matrices. It constructs the
block dilation of `A.adjoint() / sigma_max`, normalizes the right-hand side,
and applies the supplied reciprocal singular-value response. Standard requires
odd degree; the two generalized odd routes are also accepted.

`--reciprocal-scale s` is an explicit premise that the induced singular-value
response approximates `s/x` over the singular-value domain. For multiplication-odd, the Gram polynomial
`P(y)` induces `x P(x²)`, so reciprocity requires `P(y) ≈ s/y`. The CLI does not infer this
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
Solve executes locally. Physical scaling uses an exponent decomposition to
avoid intermediate overflow/underflow in the two divisions. Residuals are
evaluated in normalized coordinates and both relative and absolute norms must
be finite before output is published.

## Matrix and physical-register workflows

`embedded` and `overlap` accept `--matrix matrix.h5 --alpha alpha` instead of
`--encoding`. The explicit positive physical normalization must make the matrix
a contraction; dense Julia dilation retains the original rectangular logical
dimensions and admits its PSD roots and whole-oracle unitarity.

`matrix-preset --preset diagonal|hermitian|general --rows R --cols C --seed S
--scale A --output matrix.h5` exports reproducible complex matrices in the
standard `/matrix/dense` format. Diagonal presets use positive entries in
`[A/2,A)` and require a square shape, as do Hermitian presets. General presets
allow rectangular shapes. The documented generator is SplitMix64 with the top
53 bits mapped to `[0,1)`; `scale` describes entries and is not a certificate of
spectral normalization. Choose `--alpha` from the desired physical matrix.

For local `embedded`, `--physical-input` reads the complete physical register,
including response and auxiliary qubits, in little-endian basis order. Its
length must equal `2^transform_qubits`. The runtime projects the supplied state
onto the admitted input subspace and reports discarded mass. The optional
`--output-state` writes decoded logical amplitudes; provide it or a physical
output path. Optional
`--physical-output-state` separately writes the complete postselected physical
register before conditioning, including zero amplitudes outside the selected
output subspace. Physical register I/O requires local execution.

`catalog synthesize --kappa K --epsilon E --output payload.json` exports an
exact stored family using the normal `--algorithm`, `--export` and `--certify`
options. `catalog solve --kappa K --epsilon E --matrix matrix.h5 --rhs rhs.h5
--output-state x.h5` constructs that family and applies its stored reciprocal
scale. The normalized singular values must lie in `[1/K,1]` before synthesis.
`--residual-tolerance` still independently checks the physical answer: an
epsilon provenance label is never used as a residual certificate. Certification
checks synthesis against the stored polynomial, not the reciprocal approximation.

## Catalogue workers and distributed execution

Enable `rayon` and pass `--workers N` or `--workers auto` to use a caller-owned
thread pool. `auto` uses the executing root's available local parallelism;
the default remains one worker. For independent catalogue family checks,
results and errors retain catalogue order,
regardless of task completion order. Stage times are sums of worker wall times;
the total is the elapsed application wall time. Each family retains its own
resource budget, so concurrent working storage grows with the worker count.
For `synthesize` and explicit `--synthesize-input`, the same caller-owned pool
is borrowed by binary64 completion and synthesis. Dependent inverse halves stay
ordered; frozen output never retains the pool. Certification remains serial
within a target. Offline construction, SVD and native execution are not moved
onto workers. Native or MPI calls stay on the initializing caller thread.

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
ABI/library. MPI thread support is admitted before any worker starts; only the
simulation root creates a pool, and every rank agrees on pool admission before
continuing. All ranks must invoke a matching workflow. Rank zero reads the
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

Local execution separates lowering/admission from native preparation. The
collective facade currently exposes combined admission/preparation, reported as
`collective_admission_and_preparation`.

Every successful report includes application stage wall times and their total,
including runtime/pool admission, scientific output, optional trace writing and
owned pool/MPI teardown. Final JSON printing is outside that total. Pool-only
and collective teardown are recorded separately. Certification timing surrounds the entire call, including
all precision retries; individual attempts retain precision, work and elapsed
time. Offline reports retain every construction and certification attempt.

Add `--trace trace.json` for Chrome/Perfetto spans. The outer
`application_dispatch` span starts after runtime/worker admission and ends before
their teardown; it is distinct from the full JSON `total` duration. Failed
dispatch stages are recorded and the trace is written on those error paths.
A rejected preflight creates no trace. Traces are
bounded and report dropped events. Timing measures wall time on the calling
process, not synchronized distributed time or independent GPU event time.
