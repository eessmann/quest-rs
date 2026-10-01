# Mathematical audit and capability parity, 2026-10-01

This is the implementation evidence ledger for
[the approved plan](../plans/2026-10-01-mathematical-audit.md). Results apply to
the reviewed working-tree diff and the source hashes in the evidence directory;
no commit or remote integration is implied.

## Baselines and source access

- Rust baseline: `4f873db4a01bddfbe2f98a317fa2edccde9ff53f`.
- C++ comparison: `568725f2bd488a03a4f98cdf92de924f17b2834a`, inspected read-only.
- Environment: project `devenv shell`; rustc `1.100.0-nightly`, revision
  `6bb1652a020e80cef79332741d89e996d71933c9`, LLVM 23.1.1,
  `aarch64-apple-darwin`. Compiler behavior is part of this receipt.
- Zotero Desktop local library was read without importing or modifying records.
  Local PDFs reviewed: Ni et al. (2025, item `PLDYJKFD`), Motlagh–Wiebe
  (2024, `E8NHUUVK`), Sünderhauf (2023, `XN38DW5T`), Martyn et al. (2021,
  `PPICVK64`), Shende–Bullock–Markov (2006, `YGYMZG6F`), Clader et al.
  (2022, `PL3CC7YU`), and Sünderhauf–Campbell–Camps (2024, `GRIQTMQ9`).
  FNFT's short software paper was also inspected; it is not the discrete
  SU(2) inverse-NLFT algorithm used here.
- Giles–Selinger exact synthesis, Ross–Selinger approximation, and
  Giles–Selinger normal forms were primary-source supplements, not represented
  as local Zotero attachments. The Hansen interval-Newton attachment was HTML,
  not a usable paper PDF. No claim of reviewing its complete original text is
  made. The Patel–Markov–Hayes paper was identified, not fully reviewed.

## Mathematical contracts and evidence

| Area | Source and implementation correspondence | Limits of the evidence |
|---|---|---|
| Inverse SU(2) NLFT | [Ni et al., Algorithm 1 and Theorem 5.6](https://arxiv.org/abs/2505.12615): dependent prefix recursion, midpoint update and second-half shifts; `quest-qsp/kernel.rs` and `offline/kernels.rs` | Stability requires strict contractivity and the outer complement. Adaptive Weiss sampling is an implementation choice. The paper's rounding hypotheses do not automatically prove a nearest-rounding binary64 implementation. |
| Complement and controls | Analytic Schwarz/Weiss completion, positive-real constant gauge; both solvers reconstruct the actual exported controls, including terminal K | Boundary completion alone does not establish outerness. The added linear fixture has an independently solved outer root and a reflected factor with identical autocorrelation. Higher-degree outerness remains a mathematical premise of the algorithm, not a new blanket certificate. |
| Generalized QSP | [Motlagh–Wiebe](https://doi.org/10.1103/PRXQuantum.5.020368), polynomial unitary products and generalized rotations | Arbitrary admitted U(2) controls are checked as full products. Their query counts do not certify gate-level implementation cost. Negative Laurent support requires inverse-signal scheduling absent from both projects. |
| Generalized QSVT | [Sünderhauf](https://arxiv.org/abs/2312.00723), Hermitianization and Chebyshev compression; typed argument and component contracts in `quest-qsvt` | The Hermitianized polynomial for complex coefficients must be evaluated before selecting its blocks. For p(x)=ix the lower-left block is iB†, not (iB)†; the added rectangular complex regression fixes this distinction explicitly. |
| Standard QSVT | [Martyn et al.](https://doi.org/10.1103/PRXQuantum.2.040203) and [Tang–Tian](https://arxiv.org/abs/2302.14324), alternating projected-unitary products | Actual exported Wx and converted projector phases are separate evidence. Standard robustness statements retain their exact encoding/completion assumptions and cannot be applied indiscriminately to generalized or native execution. |
| Remez | One retained mathematical expression, scalar/interval AD and certified root coverage; C++ generic callable concepts compared with Rust's sealed expression architecture | Static dispatch cannot prove consistency of user callbacks. Binary64 Remez's minimax-gap certificate differs from offline Remez's uniform exported-error bound; offline exchange gaps remain empirical. |
| Exact Clifford+T | [Giles–Selinger](https://arxiv.org/abs/1212.0506), denominator reduction and determinant restrictions; independent `quest-math` proof replay | Clean-ancilla evidence is CJ=JU on clean inputs, not arbitrary occupied ancilla action. Full scalar phase is retained. |
| Approximate Clifford+T | [Ross–Selinger](https://arxiv.org/abs/1403.2975), candidate and norm equations; bounded rational LLL search and exact final norm tests | Sound certified candidates do not establish global T-count optimality, factoring-oracle complexity or search completeness. Unresolved factorization is not nonexistence. Sphere containment is justified below. |
| Normal forms and workers | [Giles–Selinger normal forms](https://arxiv.org/abs/1312.6584); full-phase reconstruction and compiler recertification against the original request | Channel equivalence alone loses phase. Worker-provided metadata or certificates never replace original-target recertification. |
| Circuit/data loading resources | [Shende et al.](https://doi.org/10.1109/TCAD.2005.855930), [Clader et al.](https://doi.org/10.1109/TQE.2022.3231194), and [structured block encodings](https://doi.org/10.22331/q-2024-01-11-1226) from Zotero | QSD is a distinct continuous-gate synthesis algorithm absent from both compared projects. Dense simulator dilation and oracle counts do not establish QRAM, T-count, depth or hardware data-loading bounds. |

## Seven route contracts

Write B=A/alpha and H=[[0,B],[B†,0]]. Ordered left and right spaces are part
of the type/route contract, including rectangular matrices and nullspaces.

| Route | Mathematical result |
|---|---|
| Standard | Parity-dependent singular-value transformation; odd maps right to left, even acts on the right space. |
| Direct Hermitian | p(B), requiring the whole supplied oracle to be Hermitian with the admitted matching projectors. |
| Hermitianized full | p(H), preserving the complex coefficients in both off-diagonal blocks. |
| Hermitianized even | The right/right block of the even component, including p(0) on nullspaces. |
| Hermitianized odd | The left/right block of the odd component. |
| Multiplication even | q(B†B); the argument is x², and is not obtained by merely reinterpreting Chebyshev coefficients. |
| Multiplication odd | B q(B†B), retaining the forward encoding even when q has degree zero. |

An encoded Hermitian A does not establish a Hermitian oracle U. PREP/UNPREP
identities must hold on the whole operator, not just their zero-state column.
Basis conversion, argument reduction, projection and continuation remain
separate checked operations.

## Lattice sphere containment

The rotation search uses 0<epsilon<=1, a unit target z, and feasible ring points
u satisfying |u|<=1 and |u•|<=1. It filters the cap
Re(conj(z)u)>=1-epsilon²/4. This implies
|Im(conj(z)u)|<=epsilon/sqrt(2). In the ideal weighted coordinates used by
`Grid::new`, the radial displacement from 8/epsilon²-1 is at most 1, the tangent
coordinate has magnitude at most sqrt(8), and the combined bullet coordinates
have norm at most 2. The squared distance is therefore at most 13, below the
implemented squared radius 36.

For the actual midpoint construction, let s=2^bits. Admission requires
1/s<=epsilon²/4096 and target coordinate-width sum/s<=epsilon²/4096. The exact
sqrt(2) enclosure has width one grid unit; dividing by two gives a
sqrt(1/2) enclosure of width one grid unit. The cyclotomic representation used
for that root preserves this width. Thus the root midpoint error is at most
h=epsilon²/4096. Feasibility implies a²+b²+c²+d²<=1 after scaling, so replacing
the algebraic root perturbs either complex embedding by at most 2h. The target
midpoint error is at most h. Each target dot/cross product changes by at most
3h+2h²<=4h. Consequently the weighted radial magnitude is below 2, tangent
magnitude below 3, and bullet-pair norm below 3. Their squared norm is below
22<36. The sphere contains every feasible exact cap point under the admitted
precision conditions, including midpoint errors.

LLL performs exact integer row operations and swaps, retaining a unimodular
change of basis. The coefficient-norm pruning is necessary by the average of
the two embedding norm squares. Radial pruning uses the upper bound
(1+target_width/s)(1+2*root_width/s) and exact rational Cauchy–Schwarz projections.
The small-grid tests separately compare enumeration to direct enumeration of
the sphere intersection and compare exact algebraic epsilon-cap points to the
original sphere. These tests support, but do not replace, the inequalities.

## Confirmed corrections

- Offline synthesis formerly performed six computation attempts for a verifier
  work limit of one. The regression observed all six before the fix. Fixed
  verifier `Budget`/`Policy` errors now terminate with the first report and
  export retained. Numerical failures can still use the caller's explicit
  bounded precision policy, because recomputation may change the export.
- Native compiler provenance formerly reported `ross-selinger-rust-v1` while
  the generator reported `ross-selinger-lll-prime-norm-v1`. A single public
  algorithm identity now supplies generator, compiler and worker metadata.
- Rank admission now scales before SVD and compares the relative threshold,
  avoiding intermediate overflow at extreme physical scales. Physical residuals
  use the actual rounded output in normalized coordinates.
- Independent review identified shared-expression DAG work before admission,
  omitted vector-contractor metadata/capacity storage, and dropped native targets
  on failure. Cached precharges and cumulative work, exact-capacity contractor
  buffers, and additive `run_reported()` failure ownership resolve these findings.
  The original reviewers re-inspected the fixes and approved their scopes.
- Allocation diagnostics revealed recurring Rayon external-injection queue
  allocations and lazy macOS worker condition-variable allocations under load.
  A multithreaded zero-allocation promise was therefore unjustified even inside
  an entered pool. The corrected test asserts zero allocation for 1,600 calls
  in a single-worker scope, and checks four-worker numerical response/output
  buffer reuse while reporting scheduler allocations separately. Backtraces
  identify allocation origins, not exact total counts: tracing temporarily
  disables recursive tracking. No production numerical algorithm changed.

## Supported capability matrix

The comparison concerns supported mathematics and workflows, not an identical
command spelling or a translation of the C++ object hierarchy.

| C++ capability | Rust implementation / consolidation | Compatibility decision |
|---|---|---|
| Monomial, Chebyshev, Laurent, Hermite, Laguerre, Jacobi | Existing typed bases, checked support and parameterized recurrence | Preserve all six bases and original parameters. |
| Generic certifying Remez function | `Function<E>`, `Expression`, `Backend`, `typed_function!`, generic Remez states/results | Dynamic `Function<Expr>` remains the default type; custom callables carry conditional evidence. |
| Scalar/interval/AD evaluation | Shared backend operations and `JetBackend`, including non-Copy MP arithmetic | Exact dyadic constants retained. No silent evaluation at lower precision. |
| Compile-time structure | Const expression construction, metadata, `StaticDegree<N>` and checked dimensions | Runtime degrees retained; no compile-time convergence claim. |
| Symmetric Laurent conversion | `to_chebyshev_symmetric` with original-source conversion evidence | Require exact coefficient symmetry; do not project near-symmetric input. |
| Supremum norms | `certify_norm`, finite outward real-segment/disc bounds and explicit stopping status | A disc includes its interior; negative Laurent powers with a pole are not treated as a circle-only norm. |
| Interval contractors | Extended Newton, Krawczyk, scalar/vector Hansen–Sengupta | Explicit callback enclosure premises, root-preserving branches and bounded work/storage. |
| Pauli expansion | Dense decomposition and reconstruction with an explicit little-endian convention | No threshold chopping of small coefficients. |
| RHW and inverse NLFT | Both production and explicit MP factorization paths | Inverse NLFT becomes the fresh-request default; RHW and persisted selection remain available. |
| Target admission / completion | Strict contractivity, retained source, Weiss completion and eager typed ratio | Rust's stricter parity/domain admission is intentional. Completion evidence does not claim general outer-root certification. |
| Independent verification | Outward MP direct convolution and FFT product reconstruction | Verify all four entries and actual exported phases; do not share producer arithmetic. |
| Dense/projected encodings | Complex rectangular dilation, projectors/isometries, explicit normalization and seven routes | Logical dimensions, padding and ordered spaces remain distinct. |
| Inverse polynomial catalog | All 21 stored families, selected-algorithm synthesis/export and solve selection | Approximation domain and physical residual checks remain separate from QSP export evidence. |
| Applications | Matrix-backed embedded/overlap execution, deterministic auto routes, seeded matrix presets, physical register I/O and solve | Preserve existing file workflows and logical state interfaces. |
| Execution / interchange | Checked JSON and HDF5, immutable artifacts, QuEST runtime and distributed wire | File metadata is re-admitted; CPU evidence does not validate MPI/GPU implementations. |
| Parallel numerical kernels | Explicit caller-owned Rayon pools and scalar/SIMD FFT selection | No silent backend fallback. Deterministic 1/2/4-worker checks are separate from speed measurements. |
| Circuit synthesis | Exact reduction, normal forms, approximate rotations, original-target recertification | Rust-specific certified synthesis is reviewed against its own cited papers, not equated with QSD. |

New algorithms absent from both projects—signed-Laurent signal scheduling,
general QSD lowering, and multiprecision interval Remez certification—are outside
this comparison's implementation commitment. Existing binary64 interval floors
remain explicit even when candidate generation uses more precision.

## Migration and numerical boundaries

The old `Function`, `Expr`, `function!` and runtime-degree builders remain
available. `Function::new(expr)` keeps contextual `.into()` inference. Use
`typed_function!` or `Function::from_expression(typed_node)` for a concrete
expression, and `to_dynamic()` for lossless compatibility/provenance. The
[numerical-polynomials chapter](../book/src/numerical-polynomials.md) documents
backend ownership, conditional callable results and static degrees. The
[applications chapter](../book/src/qsvt-applications.md) documents catalog,
route, matrix and register commands.

Fresh requests use inverse NLFT at every synthesis entry point. Existing artifacts
keep their recorded algorithm; explicit `rhw` remains supported. Automatic route
selection follows the admitted input convention. It does not retry another route
after failed admission. Offline computation uses only caller-selected bounded
precision attempts. Work/policy exhaustion does not change algorithms or relax
accuracy requirements.

The inverse catalog provenance was refreshed to the compared C++ revision;
all 21 coefficient payloads are byte-identical to the original `7fe7f740` import.
Its stored epsilon is approximation metadata, separate from reconstruction
certification and measured physical residual. A caller's requested physical
residual can correctly fail even when synthesis certification succeeds.

Norm results use outward binary64 arithmetic and explicitly distinguish a
proved finite bound, achieved tolerance, exhausted refinement and poles.
Contractor conclusions are conditional on the documented callback enclosure
and regularity premises. Dense Pauli coefficients are rounded numerical traces,
with all terms retained; they are not interval-certified exact coefficients.
Large/small cancellation can still lose relative accuracy. None of these
interfaces grants proof merely because the dimensions or expressions are static.

Native conditional callables are supported. Trusted offline Remez admits sealed
expressions; arbitrary custom callables can use the generic backend evaluator
but have no unconditional offline certificate. Static degrees currently wrap
native Remez; offline degree remains runtime. Independent certification retains
its own arithmetic, and the exact-angle subsystem retains exact symbolic
semantics.

## Validation status

The final local integrated gates used the project environment and the feature
selection recorded in the [reproduction instructions](data/2026-10-01-mathematical-audit/README.md).
Logs, source hashes, task receipts and independent review resolutions are stored
beside those instructions. Tests establish the checked contracts and examples;
they do not discharge the broader mathematical assumptions listed above.

| Gate | Observed result |
|---|---|
| Workspace Nextest, including scalar/SIMD, Rayon, offline synthesis and native QSVT | 885 passed, 6 skipped; final run 63.913 s. |
| Workspace doctests and compiler contracts | 56 passed, 1 ignored; sealed admission, const traits, static dimensions, builder stages and evidence distinctions covered. |
| Strict workspace all-target Clippy | Passed with `-D warnings`. |
| Formatting and whitespace | `cargo fmt --all -- --check` and `git diff --check` passed. |
| Rust documentation and mdBook | Both built successfully. |
| Release large-degree tests, explicitly including ignored regressions | All 3 passed: degree 8192 interval-FFT certification, original degree 8105 offline certification, and degree 8105 sequential/1/2/4-worker bit comparisons. |
| Installed native consumers | Direct, facade, wrapped and renamed consumers passed CPU/OpenMP numerical and deployment checks; statevector and density-matrix direct paths covered. |
| Catalog comparison | All 21 families through degree 8105 match the requested C++ source; all 21 binary64 inverse-NLFT constructions passed unchanged default tolerance. |
| Binding source regeneration | Generated Rust/C++ source, names and adapter registry unchanged. Full coverage-manifest freshness did not pass; see the platform limitation below. |

The degree 8192 analytic product's certified response and reconstruction upper
bounds were zero, completion was approximately 4.44e-17 and unitarity 6.28e-17.
The degree 8105 offline export certified on its first 128-bit attempt, with
response upper bound 4.19907426849013860e-17 and reconstruction upper bound
6.97491398967269712e-17. These are synthesis bounds for those inputs, not
reciprocal-function approximation or end-to-end physical residual bounds.

Cross-owner independent reviews covered generic functions/Remez, numerical
utilities and circuit/boundary changes; the parent independently reviewed
application changes. The generic admission/failure-ownership and contractor
storage findings were repaired and re-reviewed. Final integrated gates ran
after those repairs. Earlier failing logs are retained with their context.
An [independent final ledger review](data/2026-10-01-mathematical-audit/reviews/ledger-review.md)
checked the mathematical claim boundaries, result counts and both source
manifests and found no unresolved blocking issue within its stated scope.

Both `const_trait_impl` and `generic_const_exprs` remain nightly features; the
latter is explicitly incomplete. This compiler emits a notice that generic const
expressions cannot use its default next-generation trait solver, and switches
the affected crate to the coherence solver. The exact compiler revision above
and focused compile-pass/fail tests are therefore part of the contract. Stable
compatibility is not asserted.

### Generic-function measurements

Three release trials measured the same `log(1+x²)` expression. The table reports
median time per call; construction used 10,000 iterations, value/jet 1,000,000,
and interval jet 10,000. Raw counts and environment are retained in
[the measurement directory](data/2026-10-01-mathematical-audit/functions/).

| Measurement | Dynamic expression | Typed expression |
|---|---:|---:|
| Construction | 105.41 ns; 5 allocations | 0.28 ns; 0 allocations |
| Scalar value | 16.44 ns | 2.97 ns |
| Scalar second-order jet | 23.00 ns | 6.25 ns |
| Interval second-order jet | 823.79 ns | 816.80 ns |
| Release executable size | 489,824 bytes | 467,808 bytes |
| Warm leaf rebuild wall time | 2.06 s | 2.06 s |

Every measured evaluation phase allocated zero times. Typed construction is
optimized away almost entirely in this benchmark; its subnanosecond timing is
not a general allocation or construction cost estimate. Build measurements
prewarm dependencies and rebuild only the example leaf. Executable sizes include
the wrapper and allocation counter. These results do not establish a whole-Remez
speedup or clean workspace compilation cost; many distinct expression types can
increase monomorphized code size. Four-worker FFT scheduler allocations are
reported separately and have no zero-allocation guarantee.

### Remaining platform evidence

Linux, MPI and accelerator runtime validation was unavailable in this local
Darwin pass and remains pending. No historical platform receipt is substituted.
The binding freshness command found only a coverage-manifest difference: local
CPU headers omit the conditional MPI declaration and use `QUEST_ROOT` discovery,
whereas the checked-in manifest records MPI and `CMAKE_PREFIX_PATH`. Regeneration
left source artifacts identical; the manifest was restored to preserve its MPI
record. Thus full manifest freshness remains an explicit validation gap.

The reference C++ checkout and Zotero library were not changed. This evidence
was captured before committing and integrating the existing Rust worktree.
