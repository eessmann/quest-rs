# Removal and retention ledger

Baseline: `001a2b656a5a80a60659a408f87f57670309a09b`. This ledger records the
architectural decisions of the October 2 pass. The prior
[mathematical audit](../2026-10-01-mathematical-audit/README.md) remains the
paper/capability reference. The C++ checkout and Zotero library were not modified.

This is an HPC scientific solver. Custom arithmetic implementations are expected
to obey their documented mathematical laws; this pass does not introduce a
hostile-backend threat model. Admission checks serve numerical correctness,
failure recovery, storage accounting, or native interoperability. Immutable
metadata is admitted once and reused in evaluation loops.

| Layer | Removed or replaced | Retained purpose and obligations |
| --- | --- | --- |
| `quest-numerics` | Binary64-only contractor API; duplicated derivative implementations; string-selected MP unary dispatch | Backend capabilities, first/second/vector AD, directed enclosures and deterministic root coverage; finite/domain/exponent/work/storage checks remain runtime obligations. FFT plans use runtime lengths and CPU selection; SIMD and caller-owned parallel pools are real execution choices. |
| `quest-polynomial` | Interpreted `Expr`, dynamic defaults, `typed_function!`, `to_dynamic`, `Real` delegation, paired callbacks, separate native/offline Remez and forwarding static builder | Concrete `Function<E>`, sealed structure, exact captured inputs and one open assumption-bearing extension; generic coefficients/bases/shapes and owning Remez; independent coefficient evaluation and separate uniform/minimax proofs. Complex binary64 boundary remains for QSP and scientific data. |
| `quest-qsp` | Approximation-specific MP context and the old offline Remez engine | Inverse NLFT default, explicitly selected RHW, runtime FFT kernels, bounded offline precision and independent frozen-sequence verifier. QSP complex working numbers implement synthesis-specific complex kernels; they are not a second real-function/Remez evaluator. Verifier MP arithmetic intentionally remains independent of candidate and polynomial arithmetic. Existing artifacts retain recorded algorithms. |
| `quest-qsvt` | Imports through compiler facade | Typed seven-route mathematics, projectors and continuation spaces; runtime rectangular dimensions, normalization, nullspaces, complex phase and separate approximation/synthesis/execution errors. No physical-space claim follows from a scalar polynomial certificate alone. |
| `quest-qsvt-io` | No representation change requiring a version bump | Scientific JSON/HDF5/C++ interchange, external dimensions and persisted sequence algorithms; checked admission is an external data boundary. |
| `quest-qsvt-cli` | Facade imports | Runtime catalog/application dispatch, files, reports and platform choices. JSON output remains a user-facing serialization boundary, not erased compiler proof evidence. |
| `quest-circuit` | Entire forwarding crate, moved tests/benchmarks | `quest-compile` is the sole canonical compiler API and macro export owner. No replacement compatibility crate. |
| `quest-compile` | Forwarding model/payload/program/provenance/rational modules; native synthesis relay through process client; erased errors; JSON-valued evidence; redundant parity spelling and `OptimizationError` alias | Concrete compiler errors/evidence, generic generators, semantic owner re-exports and meaningful staged APIs. Pass traits distinguish admissible transformations; exact replay and original-target recertification remain independent. Result aliases name actual error/ownership contracts rather than alternate implementations. |
| `quest-language` | Synthetic finite AST reconstruction and dummy floating captures; boxed compiler/process errors | Actual file-driven syntax, checked semantic admission, SSA, effects, foreign-handle ownership and VM runtime checks. Finite insertion is checked typed data. Typed classical `Expr<T>` is necessary for runtime programs and is distinct from the removed numerical interpreter. |
| `quest-macros` | Facade-name dependency resolution and placeholder captures | Checked token templates, exact source angles, typed scalar/oracle capture banks, source-order capture-once semantics and renamed-dependency resolution to canonical owners. |
| `quest-qasm` | Facade imports in adapters/tests | Text parsing and export are the external program interchange boundary. Runtime AST data is intrinsic to this purpose. |
| `quest-symbolic` | Duplicate private rational aliases | One canonical rational type per crate, immutable exact source graph and independent affine replay. Symbolic π and source obligations remain exact; numerical simplification cannot discard finite-conversion or parameter obligations. |
| `quest-math` | No independent verifier arithmetic merged | Exact-ring, determinant, denominator and full-phase verification remains independent of candidate search. Its rational alias selects the workspace bigint representation without depending on the compiler. |
| `quest-synthesis` | No new circuit algorithm or weakened acceptance | Bounded lattice/norm candidate search, explicit unresolved candidates and independent exact replay. Approximation soundness does not imply complete or optimal search. |
| `quest-optimizer-protocol` | No unchanged format version increment | Typed process interchange, resource ceilings and target identities; unsupported versions rejected. |
| `quest-optimizer-client` | In-process native synthesis relay/dependency | Linux process isolation, transport, bounded resource handling and independent checking of worker numerical results; platform limitation is explicit. |
| `quest-optimizer-worker` | Facade imports | Optional external engine process and typed protocol. Worker output still needs original-target independent recertification. |
| `quest-rs` runtime | Three separate native matrix preparation maps and duplicated sizing arithmetic | One dispatch-derived preparation pool per owner; separate reservations/lifetimes across owners, source identity, ordered signed controls, register/environment checks, RAII and ABI-safe native ownership. |
| `quest-sys` | Unused generated names output | Reviewed CXX signatures, exception conversion, native lifecycle, precision/version checks and MPI ABI witness. Foreign MPI C shims and one-to-one bridge wrappers are interoperability requirements. |
| `quest-build` | Historical explicit selector spellings and ancestor normalization | `QUEST_ROOT` exact prefix and conventional CMake discovery; imported target identity/link order, matching headers/compiler/sysroot, loader paths and deployment admission. Internal CMake `QuEST_DIR` pins an already admitted package. |
| `xtask` | Generator bootstrap from output/coverage and unused generated names | Reviewed adapter registry is authority; regeneration freshness and real native consumers verify interoperability/deployment. Erased command-line errors and parsed Cargo metadata are tooling boundaries, not mathematical evidence. |
| Documentation/fixtures | Current facade examples, stale API guidance, and obsolete `benchmarks/architecture/run-functions.sh` targeting removed dynamic examples | New portable numerical/project fixtures compare explicit checkout revisions. Dated verification source/receipts remain frozen to reproduce named historical commits. Their old imports and script paths refer to those historical revisions, not supported current compatibility APIs. |

## Mathematical and ownership boundaries checked during independent review

- Candidate point arithmetic and solver diagnostics cannot certify their own
  output. Audited enclosures admit the stored candidate coefficients and support once
  before proof. The immutable support cache is reused in hot evaluations; custom
  point backends have the usual documented arithmetic and exact-ordering laws.
- Binary64 exports are frozen before proof and independently compared against
  retained coefficients. Repeated custom conversions cannot change certified data.
- Exact-domain outer endpoints cover the uniform proof; inner endpoints constrain
  alternation witnesses to the original domain.
- A complete critical-point cover is required for a uniform certificate. Root
  existence, at-most-one, uniqueness, and unresolved coverage remain distinct.
- Scalar and vector input admission occurs before zero/cancellation shortcuts.
  Recovery preserves the parent when an atomic contractor step cannot finish.
- Retry policies are bounded before cloning, retain original targets, and account
  retained capacities/limbs. Reportable allocation recovery is limited by whether
  storage for the minimal failure artifact itself can be obtained.
- Unstable const features encode real shape/structure invariants. They do not
  replace runtime convergence, enclosure, finite-value or resource checks.

## Representation changes

Compiled publications, historical compilation evidence, and frontend templates
are version 2. Producers and consumers were migrated together and reject old
versions. Worker protocol, scientific interchange, QSP artifacts and exact-angle
semantics were not gratuitously versioned. No legacy decoder was introduced.
