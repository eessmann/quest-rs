# Architecture and synthesis overhaul

Implementation in `codex/circuit-synthesis`, isolated from the primary checkout,
based on `7148ccaa47e9e39506d62ba1e441d657040b83f8`. The primary Cargo/devenv edits
and the C++ working tree were not implementation inputs. This record separates
mathematical contracts, executable evidence and platform coverage. All six
implementation stages and the final local validation are complete. Linux and GPU
runtime evidence was outside this snapshot. The [2026-09-30 Linux follow-up](2026-09-30-linux-architecture-synthesis.md)
records subsequent platform execution, corrections and refreshed measurements;
GPU execution remains unvalidated for this overhaul.

## Review boundaries

The six stages can be reviewed by ownership and contract:

| Stage | Main review surface | Acceptance evidence |
| --- | --- | --- |
| 1. Review and contracts | [Approved implementation contract](../plans/2026-09-29-architecture-synthesis.md), this record and pinned reference manifests | Checkout identities, initial baseline limitation, mathematical and platform boundaries |
| 2. Semantic core/compiler | `quest-language`, `quest-compile`, extracted `quest-symbolic` | Independent SSA checks, core-only builds, compile-fail capabilities and artifact rejection |
| 3. Unified lifecycle | `quest-circuit`, macros, typed builder, specialization, native preparation | Cross-frontend tests, once-only captures, full-phase native comparisons, static reuse, traps and feedback |
| 4. Synthesis replacement | `quest-synthesis`, independent `quest-math` verifier | Exact reconstruction, ancilla/determinant boundaries, pinned newsynth comparisons, deterministic limits and corrupted proofs |
| 5. QSP/QSVT | `quest-qsp`, polynomial conversion, QSVT route types and CLI/IO | Structured versus dense RHW, inverse NLFT, both C++ branches, actual projector export and large release fixtures |
| 6. Consumers and acceptance | Native/CLI consumers, book, build helper and performance receipts | Workspace/doctests/lint, native feature matrix, binding freshness and measured reuse |

## Architecture and public behavior

`quest-language` owns checked semantics, exact/dynamic angle distinctions,
quantum regions and immutable matrix/oracle payloads. `quest-compile` owns
rewrites, specialization, region compilation, artifact validation and reports.
`quest-circuit` is the frontend facade. Macros depend on the core/compiler and
publish checked templates with deferred captures. `quest` owns environments,
native allocation, preparation, dispatch and RAII.

The user lifecycle is `construct -> verify -> compile -> prepare -> run`, through
`ProgramBuilder`, staged `Program`, `Environment::prepare`, `PreparedProgram` and
`RunOutput`. `QuantumRegionBuilder` constructs finite reusable regions which
enter that same lifecycle; it has no separate public native executor. Macros,
typed builders and text import converge on checked SSA. Rust captures run once
in source order. Prepared static gate records retain resolved parameters,
operands, signed controls and full phase; genuinely dynamic control and feedback
remain interpreted. Static entry-region dispatch is conservative: formal call
targets and control-dependent paths are not all cached. Immutable input
specialization binds typed scalar/array values once and retains source binding
history; explicit partial specialization leaves omitted inputs dynamic. Native
arbitrary-angle simulation is the default.

`legacy_circuit!`, its parser/expander, obsolete lifecycle aliases and the
vendored mathcore/rsgridsynth implementations are removed. The used exact affine
implementation is project-owned under `quest-symbolic`, with its license and
provenance retained. Exact rewriting preserves binding obligations and possible
traps. Numerical matrix admission never grants exact-unitary privileges.

Source export and compiled export are explicit operations. Compiled artifacts
retain optimized SSA or frozen QSP payloads, source identity, conventions,
algorithm/precision, bindings, limits and evidence. Loading checks version,
integrity and mathematical/semantic admission; it does not substitute unoptimized
source for the saved executable. Plain QASM source export reports an error for
exact Rust captures or native payloads it cannot faithfully represent; compiled
artifacts retain those values. QSP consumers independently recertify loaded
payloads. A digest alone is not a mathematical certificate. Circuit artifacts retain
independently checked historical local synthesis certificates, identified as
such; these do not establish source equivalence for an arbitrarily changed
final executable. Certified QSP artifacts instead recertify their actual frozen
payload on load. Artifact decoding admits encoded size and conservative aggregate
reconstruction/proof storage before allocation and rejects unknown fields.

## Mathematical implementation and references

The Rust generator uses exact Giles–Selinger reduction, deterministic elementary
Gray-path lowering, Matsumoto–Amano normalization and Ross–Selinger candidate/norm
equations. Its exact rational LLL/Minkowski grid enumeration differs from
newsynth's grid-operator algorithm; no optimal T-count or identical search
complexity is claimed. Candidate generation and independent certificates remain
separate. Resource failures never establish mathematical impossibility.

`AllowOneClean` is the default and permits at most one new, reusable zero wire.
Verification checks `C J = J U`, including clean return and wire order.
`NoAncilla` enforces determinant restrictions. Full scalar phase is retained,
including controlled rotations; `Rz(pi/4)` is not equated to T.

Pinned [newsynth 0.4.1.0 executable fixtures](../../crates/quest-synthesis/tests/fixtures/README.md)
were built with GHC 9.10.3 and random 1.1. Only Cabal dependency bounds were
relaxed; its algorithm source was unchanged and is not incorporated into Rust.
All four fresh words pass independent full-phase certification; corrupted words
are rejected.

QSP now defaults to the structured rank-two RHW/Half-Cholesky recurrence.
`InverseNlftDivideConquer` remains supported for both real-parity Wx and complex
unit-circle generalized responses, in binary64 and explicit offline precision.
A separately implemented bounded dense RHW reference checks the recurrence.
Algorithm, response convention, precision, FFT backend and execution policy are
independent; no silent fallback occurs.

Immutable QSVT route types distinguish Hermitian `x` from Gram `y=x²`, preserve
monomial/Chebyshev conversion and even/odd reduction, and bind certificates to
exact target bits and source spans. Converted Wx projector angles and readout
are independently certified as their actual binary64 payload. Standard analysis
can consume that attached response bound; encoding and completion theorem
assumptions remain explicit. Generalized QSP certificates do not establish a
blanket generalized-QSVT robustness theorem or native floating-point guarantee.

[C++ branch adapters and capability manifests](../../benchmarks/reference/branches/README.md)
use immutable archives:

- main `a932e7e081ac3766cad19ad6f8f4b920c8fa7fcf`;
- develop `4fc35983138d07a990862a4d83ad16f2b737c98f`.

The comparison reconstructs all four complex operator entries without phase
alignment. Both Rust algorithms agree within about `1.7e-15` on supported
fixtures. Main's rejected constant cases are recorded as unsupported. C++
parity projection is tolerance-based; Rust real/parity admission is exact.
Development's certificate precedes final lowering; Rust retains stronger
frozen-export verification. Older `7fe7f740` fixtures remain historical baselines.

## Correctness review fixes

Independent review found and fixed aggregate resource-admission gaps in dense
polynomial basis conversion, converted-projector verification, exact proof
replay, normal-form reconstruction and approximation acceptance. Regressions
first failed and then passed. Array initializers now admit the aggregate expanded
size of repeated shared expressions and rank wrappers before materialization;
indexed places admit the combined base and indices before cloning. Synthesis
verification work exhaustion stays distinct from rejected mathematical evidence. Polynomial conversion now checks dense rows plus
linear buffers and cumulative logical work before allocation. Projector
verification admits all retained/transient vectors before cloning or trig work.

Native migration also exposed duplicate accounting of shared matrix payloads.
Allocation-identity tracking now counts each immutable source allocation once;
native caches account separately for their representations. The diagonal test
allows the common IR/provenance overhead while explicitly requiring preparation
below 64 KiB and a total budget below a dense native cache. Shared clones,
different signed-control profiles, density execution and partial-error state are
covered independently.

Explicit unit-circle coefficient transfer retains nonzero support offsets into
QSVT; padded storage is checked before allocation. Zero-polynomial route binding
checks the original empty span, not the solver's
padded constant zero. Tests reject binding a stored-zero target to evidence for
an empty source. Signed zero capture identity is compared by IEEE bits.

Native output admission includes map entries, names and nested array storage,
including initial VM outputs and all retained sampling results. Collective
preparation compares classical outputs and execution counts as well as quantum
operations. It reserves aggregate extraction/serialization scratch and rejects
native dense gates that exceed per-rank amplitude capacity before native calls.
The MPI wrapper helper preserves `mpicc`'s invocation basename while validating
and watching its resolved target; resolving the symlink before execution had
broken OpenMPI's wrapper dispatch.

## Completed validation

The [final workspace receipt](data/2026-09-29-architecture-synthesis/default-validation.json)
records commands, exact exit statuses, test counts and verified log hashes.
[Focused receipts](data/2026-09-29-architecture-synthesis/focused-validation.json)
retain the native/worker feature matrix, minimal native lint, bindings, consumers,
book, formatting, all-feature QSP and explicit large release runs. Earlier checks
are labeled by scope; the final workspace run covers the frozen implementation
including ranked arrays, aggregate initializer admission and region API names.

Local host: aarch64-darwin, Apple M1 Pro, QuEST 4.3.0 from the project Nix
installation, Clang 21.1.8, Rust 1.100.0-nightly (`6bb1652a0`, 2026-09-22).
The initial isolated baseline was interrupted by workspace manifest setup; it
is not presented as a passing baseline. Prior primary-tree results remain
historical review evidence.

| Check | Observed result |
| --- | --- |
| Final `cargo build --workspace --locked` | Passed |
| Final `cargo nextest run --workspace --locked` | 787 passed, one explicitly ignored fixture, 146 test binaries |
| Final `cargo test --doc --workspace --locked` | 49 passed, one ignored, 23 result groups |
| Final workspace all-target strict Clippy and QSP all-feature/all-target strict Clippy | Passed with `-D warnings` |
| Final formatting and `git diff --check` | Passed |
| Initial full `cargo test --workspace --locked` | Passed 776 distinct top-level tests and 49 doctests before the final builder audit; 814 aggregate results include child-process repetitions. The final frozen-tree results above supersede this snapshot |
| Native/optional worker feature nextest | 97 passed, five Linux-only process cases skipped; QSVT, workers, serde, codespan reporting, ndarray, synthesis, ZX and MITM enabled |
| Minimal native `cargo clippy -p quest-rs --all-targets --locked -- -D warnings` | Passed without QSVT/MPI feature unification |
| `cargo test -p quest-polynomial -p quest-symbolic -p quest-qsvt --features quest-qsvt/certification` | Passed, including doctests, immutable conversion, route binding and actual projector-phase certification |
| Selected `quest-rs --features qsvt` runtime, structured-runtime, oracle, matrix-budget, QSVT, diagonal and environment tests | Passed on native macOS |
| Native ownership compile-fail tests | Passed after reviewing expected borrow/Send diagnostic changes |
| QSP default/all-feature suites and strict Clippy | Passed; final all-feature lint and default workspace/CLI integration also passed |
| `cargo test -p quest-qsp --all-features --release -- --ignored --nocapture` | All three ignored fixtures passed: degree-8192 outward FFT, degree-8105 offline RHW and binary64 parallel reproducibility/certification |
| Degree-8105 offline RHW | One 128-bit attempt, grid 65,536; synthesis 100.14 s, certification 62.13 s; response upper bound about `4.20e-17` |
| Pinned newsynth and both C++ branches | Executable fixtures and full-complex comparisons passed within each manifest's capabilities |
| `xtask generate-quest-bindings --check` | Passed; generated bindings unchanged |
| `xtask check-native-consumers --backends cpu,omp` | Passed direct, facade, wrapped and renamed consumers; state-vector/density checks, deployment and installed Mach-O dependency closure |
| mdBook 0.5.4 build | Passed using the installed Nix executable; isolated base devenv does not include mdBook |

[Performance receipts](../../benchmarks/architecture/README.md) measure compilation,
preparation, repeated execution, synthesis, independent certificate replay,
logical work, gate counts and OS peak RSS separately. Reused preparation was
about 5–13% lower execution-plus-preparation time for the recorded native workloads,
with zero amplitude difference. These are measured local workloads, not an asymptotic claim or a
comparison against the deleted implementation.

Linux and GPU runtime results are not established by these macOS checks. The
default native installation has CPU/OpenMP and no MPI/SUBCOMM. A separate temporary
MPI/SUBCOMM QuEST build passed ABI comparison, strict lint and all seven local
collective tests ([reproduction and logs](architecture-mpi/2026-09-29/README.md)). OpenMPI's default hardware-topology discovery crashed even for
`mpiexec -n 2 /usr/bin/true`; the successful correctness run used explicitly
recorded synthetic topology and disabled binding. It supplies no CPU-placement,
MPI performance or Linux evidence.
Linux-only process adapters retain explicit platform errors on macOS. Direct
Rust synthesis is platform-independent code exercised here on macOS; Linux
runtime evidence must be recorded separately.
