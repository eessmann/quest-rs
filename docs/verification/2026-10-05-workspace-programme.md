# Sparse operators, shared algebra and dual CFD histories: implementation evidence

This record covers the implementation worktree based on
`dd9a869ebe9952cf85a6411a180ed7b688f568fb`. The approved
[programme](../superpowers/plans/2026-10-05-sparse-mathcore-cfd.md) remains open.
Implemented APIs, numerical tests, admitted circuit executions and unresolved
scientific/capacity acceptance are distinct below. Native QuEST source is unchanged.

## Implemented and reviewed foundations

| Area | Concrete implementation | Evidence and limits |
| --- | --- | --- |
| Owning encodings | Generic descriptors and transform schedules; compact projectors; separate represented-operator and whole-unitary identities; controls/adjoints and compatibility adapters | Whole-register reference tests, including failure/padding sectors; an extracted block alone does not validate U |
| Sharded sparse input | Stable-ordinal duplicate reduction, bounded external runs, deterministic synchronous matching colors and distributed permutation completion | Local MPI 1/2/4/8/split, adversarial input and rank-independent semantic checks; high-degree work remains admitted explicitly |
| Persistence and execution | Immutable serial-HDF5 buckets, JSON manifests, file SHA-256 and canonical semantic digest; owned snapshots, bounded portable replay, local-owner CPU/MPI matching | [Distributed contract](../research/distributed-matching.md); producer-to-load restart and whole-unitary native/portable regressions |
| Native admission | Replicated dense/diagonal matrix limits, distributed target-width constraint, indexing/count and scratch admission; unitary controls without dense expansion | General linear operators retain their separate semantics; unsupported new accelerator paths reject |
| MathCore | Maintained pinned fork as a normal dependency, shared exact/scoped/static/dynamic/polynomial algebra and backend contracts; real consumers throughout the workspace | [Ownership, semantics and provenance](../research/shared-algebra.md); superseded affine/expression implementations removed, independent checkers retained |
| Encoding portfolio | Uniform/per-matching bounds, weighted LCU/tensors/Kronecker sums, eligible SCC constructions, explicit sparse-access QROM | [Independent same-matrix comparison](../../crates/quest-qsvt/docs/portfolio-comparison.md) and [constructed cross-size counts](../../crates/quest-qsvt/docs/portfolio-resource-curves.md); no native or equal-accuracy advantage claim |
| Polynomial dynamics and Carleman | Complete physical coefficients, ordered reference and normalized symmetric hierarchy, external degree-zero source, stateless recipes and explicit truncation evidence | [Derivation and error contract](../../crates/quest-cfd/docs/carleman-history.md); conserved means remain present and reject the strict dissipative theorem when its hypotheses fail |
| Global histories | Forced/time-dependent causal DG1/DG2, direct non-Hermitian inverse, coherent local/distributed amplitude trees and streamed MPI history solve | [Generated Carleman and full-coordinate KvN consumers](../../crates/quest-cfd/docs/distributed-history.md); no full history matrix, RHS or gate stream required on the distributed route |
| Physical foundation | Full BDM2/P1 box references, complete polynomial-time boundary lifting, generated box constraints, cell-whitened distributed Householder charts, supplied-force physical pressure recovery and generated autonomous physical force/drift | [Physical-space contract](../../crates/quest-cfd/docs/physical-space.md); distributed autonomous gauge/momentum tests at 1/2/4/8 ranks and split communicators; [bounded general polynomial boundary reference](2026-10-05-polynomial-boundary-lifting.md); ambiguous numerical rank rejects |
| Nonlinear demonstrations | Full eight-coordinate viscous Burgers and doubled-field 24-coordinate Fu–Shu KdV; independent Airy, energy, physical/time/order tests | [Dual-history receipts](2026-10-05-dual-history.md); the KdV auxiliary field evolves and is never removed |

Probability-weighted distributed KvN readout and conditional sampling estimates
have passed independent review. The [observable contract](../../crates/quest-cfd/docs/probability-observations.md)
separates conditional ensemble energy, postselection, caller-supported probability
bounds and systematic bias. It includes the executed nonlinear MPI history;
sampling costs remain estimates, not measured quantum shots. The later
[composed temporal observation](../../crates/quest-cfd/docs/distributed-temporal-observation.md)
cleans inverse workspace, executes coherent DG interpolation and reduces a typed
physical observable. Independent MPI 1/2/4/8/split checks passed, including
original-normalized joint probability, physical scaling, complex interference
and rejection before mutation. MPI2 injected errors and panics after projection
terminate the job within the test deadline. This remains simulated postselection,
not a repeated sampled measurement campaign. Generated distributed
physical-force assembly has also passed independent review and local MPI tests.
Its [focused evidence](2026-10-05-generated-physical-force.md) now connects to
[collectively scheduled full-coordinate box KvN history construction](2026-10-05-generated-box-kvn-history.md). The local
sparse readers consume owned immutable spools without invoking MPI callbacks.
Three-dimensional cavity references now
include reviewed transverse-plane and spanwise reflection diagnostics at both
physical orders; these are finite classical samples, not symmetry or convergence
certificates.

The later [generated polynomial-time source](../../crates/quest-cfd/docs/generated-time-boundaries.md)
retains prescribed traces, body forces and the derivative of the full boundary
lifting. Its distributed drift, pressure reconstruction and DG1/DG2 history
construction have passed local 1/2/4/8-rank and split-communicator checks.
Independent review reproduced mixing earlier drift results from different times
or states across ranks; pressure recovery now agrees on both exact query time
and complete state-shard identity before collective queries. A nonzero history
inverse was prepared; these time-source tests do not claim native inverse replay.

[Bounded pressure and traction observations](2026-10-06-bounded-pressure-observables.md)
now include BDM2/P1 and fixed-time polynomial boundary problems. They retain the
pressure gauge, facet trace convention, full lifting/acceleration and explicit
stress convention. Independent review also corrected missing retained-capacity
accounting in the Simplex capture adapter. These are bounded prepared classical
observables; general scalable coherent value oracles remain separate work.

The [configuration-flux diagnostic](2026-10-06-configuration-flux.md) computes
instantaneous inward and outward exterior trace rates using every physical
coordinate and the prepared polynomial drift. Independent analytic tests cover
constant, compressive, time-dependent and nonlinear cross-coordinate flow,
duplicate internal facets and bounded resource admission. Periodic evolution is
unchanged; these rates are not an integrated escape or convergence certificate.

The later [initial weak-generator diagnostic](2026-10-06-configuration-weak.md)
contracts complex configuration amplitudes and compares all five physical rates
plus integrated kinetic energy with the original full constrained system. Its
twelve independently rerun tests and retained DG2 work rejection support only
that bounded initial diagnostic. Its subsequent
[seven-row fixed experiment](2026-10-06-configuration-weak-campaign.md) completed
three requests and retained three work rejections and one sampling rejection.
The separately [retained DG2/two-cell follow-up](2026-10-06-configuration-weak-energy-shell-v2.md)
removes constant occupied energy but has a larger defect under changed
quadrature weights. Independent saved-output review verified provenance,
sampling and comparisons; substantial sampling bias and weak-rate defects
remain. These stages follow checkpoint 07 and have no
new broad workspace or physical-convergence acceptance attached.

The [prepared weighted matching composition](2026-10-06-distributed-weighted-matching.md)
now consumes the shared pure LCU schedule on CPU/MPI. Its independent tests
compare whole-register forward and standalone adjoint action on 1/2/4/8 ranks
and split communicators, including controls, padding, failure flags and a
nonzero coefficient whose preparation mass underflows. The wrapper admits
all child applications before PREP and separates constructor comparison traffic
from routing payload. Already prepared children retain their earlier costs;
this is not a total-pipeline or generic distributed tensor implementation.
The later [prepared LCU transform](2026-10-06-prepared-lcu-transform.md) connects
that owning source to the shared QSVT schedule, with independent whole-register
CPU/MPI, projected polynomial and direct inverse checks. Its reviewed admission
fixes include common overflow rejection and communicator-dependent loop costs;
unknown physical error premises remain unknown.

The subsequent [persisted preparation bridge](2026-10-06-persisted-matching-preparation.md)
collectively transfers an exclusive loaded snapshot into that prepared matching
runtime. It includes actual permutation-validation capacity admission and a
prior-child failure regression. Its focused stage follows checkpoint 07. The subsequent
[loader-capacity correction](2026-10-06-persisted-matching-loading.md) admits
actual Rust buffer capacities before downstream IO/routing, with independent
failure, IO and restart checks. Its stated payload model excludes opaque native
metadata and process-memory costs.

The later [cumulative routing telemetry](2026-10-06-prepared-routing-telemetry.md)
adds completed-application receipts across matching children and QSVT source
queries. Independent 1/2/4/8-rank, split-communicator, failure and arithmetic
checks passed. Measured matching payload remains separate from modeled costs
and excluded native/MPI traffic. This focused stage also follows checkpoint 07.

The [matching record fingerprint correction](2026-10-06-sparse-record-fingerprints.md)
replaces systematic sign cancellation in commutative record hashes, adds explicit
digest work/scratch admission and versions persisted resources as schema 2.
Old manifests reject explicitly. The separate
[portfolio correction](2026-10-06-portfolio-source-fingerprints.md) covers four
weighted/structured families and whole-input capacity admission. Independent
focused tests retain their source scopes; 64-bit fingerprints remain accidental
integrity checks rather than authentication or operator-equality certificates.

The [persisted weighted inverse campaign](2026-10-06-persisted-weighted-transform.md)
connects eight-rank publication, one phase compilation and same-file replay on
1/2/4/8 ranks and split groups. All forward/adjoint/forward requests passed the
fixed residual criterion. Earlier startup, schema and readout failures remain
separate immutable evidence. The readout regression exercises the actual paired
exchange; matching telemetry and process memory retain their stated exclusions.
This bounded operator example does not establish physical-history convergence
or input capacity beyond one node.

The [mixed affine foundation](2026-10-06-mixed-traction.md) adds explicit
Dirichlet and mechanical-traction facets, complete open-domain constraints,
absolute pressure and canonical time-polynomial lifting. Its
[P2 cylinder consumer](2026-10-06-cylinder-p2.md) retains all 54 coordinates on
the fixed polygonal source. The independently assembled exact rank and the
bounded supplied-state pressure/energy/force snapshot have separate receipts.
Neither supplies a developed shedding trajectory or a continuum error bound.
The subsequent [bounded P2 evolution](2026-10-06-cylinder-p2-evolution.md)
completed the fixed two/four/eight-step study in all 54 coordinates, preserving
its failed state/force temporal accuracy criterion. Its independently checked
504-file default-package build closure and scoped tests are newer than
checkpoint 06; that historical full-workspace result does not cover these edits.

The [constructed CFD resource workflow](../../crates/quest-cfd/docs/constructed-resources.md)
now reports actual H-adjoint matching layouts, coherent RHS preparation, optional
reciprocal schedules, controlled primitive counts and caller-conditioned sampling
costs. Twelve complete-chart KvN/Carleman cases cover temporal DG1/DG2. A count
limit and a reciprocal residual rejection remain explicit partial outcomes.
Independent review reproduced and corrected an attempted-memory reporting defect
on the latter path: partial receipts now retain the admitted temporary workspace
envelope and elapsed construction time even when the constructor rejects. These
envelopes are conservative logical limits, not measured allocator or RSS peaks.

Post-checkpoint review corrected a numeric descriptor fingerprint defect: paired
binary64 sign changes canceled in the former whole-word XOR/multiply hash. A
weighted LCU and its negative could therefore share both numeric descriptor IDs.
Byte-wise little-endian hashing now distinguishes the reproduced cases; tests
also reject binding the other operator to an existing schedule. The matching
history/adjoint source check, owning replay and weighted composition suites pass
19 tests. Temporal encoding passed five CPU/MPI tests, including 1/2/4/8 ranks
and split communicators, after the correction. These fingerprints remain bounded
accidental-integrity checks; persisted SHA-256 digests retain their separate role.
This correction and the observable metadata additions are included in checkpoint
02 below. Checkpoint 01 retains its original historical evidence.

## Executed experiments

- The short Burgers order-two inverse ran on local CPU QuEST with 13 qubits,
  degree 465 and measured relative history residual approximately `4.37e-6`.
  It used coherent RHS preparation. Its optional phase certificate was explicitly
  skipped; its `T=0.001` window is shorter than the frozen physical demonstration.
- The later [full Burgers T=0.1 inverse](2026-10-05-dual-history.md#full-frozen-burgers-window)
  executed on 14 simulated qubits with degree 7555 and relative history residual
  `1.62e-7`. A bounded outward-interval proof supplies the stored history's
  singular-value lower bound. Reconstructed physical coordinates differ from an
  independent direct history solution by `2.45e-10` and from the identical
  full-DG RK4 trajectory by `1.35e-7`. The optional phase certificate was omitted;
  physical, hierarchy and total quantum error acceptance remains separate.
- Generated full-five-coordinate nonlinear KvN histories executed at one and two
  local MPI ranks. Recipe, encoding, adjoint, RHS and inverse checks retain their
  own evidence; this is not a resolved configuration/physical calculation.
- The generated physical box force was also connected to that history route and
  executed at one and two ranks. A shifted regularized state has verified nonzero
  quadratic generator action. The degree-129 inverse on 15 simulated qubits
  recovered the complete 486-dimensional history with relative residual about
  `1.16e-3`, below its declared `1e-2` target. The nonlinear response over this
  short window is smaller than that inverse error: this validates retention and
  circuit plumbing, not a resolved nonlinear physical response.
- A later, separately budgeted [nonlinear KvN trial](2026-10-06-resolved-nonlinear-box-higher-budget.md)
  resolves the complete coarse discrete signal at T=0.1 on one and two local
  ranks. Both retain five physical coordinates, 243 configuration nodes and a
  486-dimensional history, using 15 qubits and degree 449 with mandatory phase
  certification. Relative nonlinear changes `7.57e-4`/`1.52e-3` exceed inverse
  errors `3.28e-8`/`6.11e-8`; complete-register forward/adjoint checks also pass.
  The original 64 MiB certificate rejection remains preserved. Partition-dependent
  chart rotations mean the two runs use different aligned physical ensembles;
  this is not a cross-rank ensemble or continuum convergence claim.
- [KvN refinement diagnostics](../../crates/quest-cfd/docs/kvn-refinement.md)
  separate support, width, configuration order/mesh, extent, concentration and
  reference time checks. Some weak errors decrease, while energy/domain effects
  remain unresolved. Outer-cell occupation is not measured outward leakage.
- The [paired-history campaign](2026-10-06-paired-history.md) compares both lifts
  with the same 243-sample, complete five-coordinate classical ensemble.
  Nine Carleman histories and two RK4 references completed in the first attempt;
  three KvN diagnostic-work rejections were retained. After a reviewed accounting
  correction, two KvN histories completed in a separate three-request follow-up.
  The third still rejects its unchanged aggregate source-work cap. The coarse
  KvN energy discrepancy is about `1.8e-4` despite residuals below `6e-16`;
  these classical comparisons do not establish configuration convergence.
- The [coarse DFG window](2026-10-05-cylinder-window.md) completed 80,000 RK4
  steps and 401 observations over `[4,8]` under a real 512 MiB process cap. Its
  polygon error is large and lift nearly constant; no Strouhal candidate is
  admitted. Timeout and resource-rejected attempts remain in separate receipts.
- The [full-window box campaign](2026-10-06-box-windows.md) reaches T=10/20 for
  2D/3D Taylor–Green and T=100 for both cavities. Forty-two of 48 optimized
  attempts completed under 512 MiB process caps. Four hit the explicit 3D chart
  ceiling; two refined Re100 cavity runs overflowed. Two separately declared
  smaller-step runs then completed on the same full chart. The resulting 44
  successful snapshots and six preserved rejections are coarse classical
  refinement evidence, not published-profile convergence.
- The [local sparse capacity pipeline](2026-10-05-sparse-capacity.md) starts from
  genuinely generated shards, persists/reloads them and executes the same matching
  U at 1/2/4/8 local ranks. Actual address-space caps and RSS/VMS measurements are
  distinguished from managed storage envelopes. It is not a multi-host test.

Each experiment records its own scope and identity. Earlier receipt hashes are
not rewritten when later validation or documentation changes. Source and prebuilt
binary hashes identify observations but do not independently attest a build.

## Repository verification

The system lane uses Rust `1.101.0-nightly` (`db8f076d2`), installed QuEST 4.3.0,
MPICH 4.2.2 and serial HDF5. QuEST, `MPICC` and `mpiexec` must use the same MPI.
MPI socket-dependent tests run with local networking available; sandbox socket
denial is an environment failure, not a passing MPI test.

The unchanged starting tree passed 1,237 tests with three intentionally skipped
QSP scale tests. An intermediate implementation snapshot passed 1,412 tests and
failed one newly concurrent KvN recipe test whose fixture had an insufficient
declared work budget. Increasing that fixture's explicit budget, without reducing
the charged work, passed its focused tests. These historical totals do not close
current-tree verification.

The reviewed generated-box/KvN checkpoint passed the following integrated
checks. Its [source manifest](data/2026-10-05-workspace-programme/checkpoint-01-source.json)
contains 734 build/input files with digest
`cf86935872aab69c69d60a2025cfd0c42953822a9ba1fd29f9237e6326800063`,
unchanged before and after these checks. The
[machine-readable receipt](data/2026-10-05-workspace-programme/checkpoint-01-verification.json)
records commands, environment, scopes and local-log hashes. Further observable,
boundary and constructed-cost work starts from this checkpoint and requires
separate affected tests and integrated verification.

| Check | Checkpoint 01 result |
| --- | --- |
| All-feature workspace tests | 1,447 passed, four skipped; 184.174 s |
| Default-feature workspace tests | 1,296 passed, one skipped; 85.659 s |
| All-feature doctests | 65 passed, one ignored |
| Strict all-feature/all-target Clippy | Passed |
| Default all-target checking | Passed |
| Formatting and Git whitespace | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Offline QSVT catalog validation | Passed; unchanged catalog across the final admission fix |

The second frozen checkpoint includes polynomial boundary lifting, physical and
composed temporal observations, the fingerprint correction and constructed
resource reports. Its [source manifest](data/2026-10-05-workspace-programme/checkpoint-02-source.json)
contains 748 files with digest
`283a3c745f9b1a506f446a8f31a68d7a1e8565fb51e75e6229a7b7322f605eaa`,
unchanged before and after all final gates. The
[checkpoint 02 receipt](data/2026-10-05-workspace-programme/checkpoint-02-verification.json)
also preserves preliminary failed checks and their scope. Distributed time-data
and extended pressure-observation work begins after this snapshot and needs its
own verification.

| Check | Checkpoint 02 result |
| --- | --- |
| All-feature workspace tests | 1,484 passed, four skipped; 209.437 s |
| Default-feature workspace tests | 1,328 passed, one skipped; 114.726 s |
| All-feature doctests | 65 passed, one ignored, 24 suites |
| Strict all-feature/all-target Clippy | Passed |
| Default all-target checking | Passed |
| Formatting and Git whitespace | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Offline QSVT catalog | Earlier passing evidence retained; unchanged and not rerun |

Checkpoint 03 adds the reviewed polynomial-time producer/history, pressure and
traction observations, bounded interval history-spectrum proof, explicit
higher-order integration budgets and the separately budgeted nonlinear inverse
fixture. Its [758-file source manifest](data/2026-10-05-workspace-programme/checkpoint-03-source.json)
has digest `b02f0c21d692a5301ea203b4e5645f9886901fdbd678fae2dd6586e1ab588f3d`,
identical before and after every final gate. The
[checkpoint 03 receipt](data/2026-10-05-workspace-programme/checkpoint-03-verification.json)
preserves the earlier unrelated fixture compile failure separately.

| Check | Checkpoint 03 result |
| --- | --- |
| All-feature workspace tests | 1,508 passed, six skipped; 165.606 s |
| Default-feature workspace tests | 1,348 passed, one skipped; 65.929 s |
| All-feature doctests | 65 passed, one ignored, 24 suites |
| Strict all-feature/all-target Clippy | Passed |
| Default all-target checking | Passed |
| Formatting and Git whitespace | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Box-window parser/validator regressions | 12 passed, including a malformed child process |
| Offline QSVT catalog | Unchanged; historical passing evidence retained |

Checkpoint 04 adds the reviewed common-ensemble history fixture, its diagnostic
work correction, complete source-capacity accounting and instantaneous
configuration-flux API. Its [763-file source manifest](data/2026-10-05-workspace-programme/checkpoint-04-source.json)
has digest `78373c305e25399e622dd4ef7368b0321ba9e4bc1a6847dd118cc5c501699492`,
unchanged before and after the final checks. The
[checkpoint 04 receipt](data/2026-10-05-workspace-programme/checkpoint-04-verification.json)
records the commands, environment and log identities. Paired numerical attempts
retain their separate executable and build-source provenance.

| Check | Checkpoint 04 result |
| --- | --- |
| All-feature workspace tests | 1,524 passed, six skipped; 160.420 s |
| Default-feature workspace tests | 1,364 passed, one skipped; 65.390 s |
| All-feature doctests | 65 passed, one ignored, 24 suites |
| Strict all-feature/all-target Clippy | Passed |
| Default all-target checking | Passed |
| Formatting and Git whitespace | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Paired-history and box-window validator tests | 12 passed in each suite |
| Offline QSVT catalog | Unchanged; historical passing evidence retained |

The fifth checkpoint adds shared exact dyadic geometry and the
[closed affine DG mesh stage](2026-10-06-affine-mesh.md), with complete unequal-cell
spaces, topology checks, original pressure recovery and explicit constructor/query
admission. Its [770-file source manifest](data/2026-10-05-workspace-programme/checkpoint-05-source.json)
has digest `d78439456a0b961462f433c76054c809caa523a04d85a2bd59721013f7a739ec`,
unchanged before and after verification. The
[checkpoint 05 receipt](data/2026-10-05-workspace-programme/checkpoint-05-verification.json)
preserves the successful checks and both initial failed attempts.

| Check | Checkpoint 05 result |
| --- | --- |
| All-feature workspace tests | 1,548 passed, six skipped; 180.550 s |
| Default-feature workspace tests | 1,388 passed, one skipped; 106.633 s, under failed-open syscall tracing |
| All-feature doctests | 65 passed, one ignored, 24 suites |
| Strict all-feature/all-target Clippy | Passed |
| Default all-target checking | Passed |
| Formatting and Git whitespace | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Independent affine root regressions | 31 integration tests and one topology test passed |
| Shared exact geometry root tests | 11 passed |
| Standalone exact fixture reproduction | Eight ideal-rational rank certificates and two P2 mass tables reproduced |

The initial all-feature run had five CMake module-open failures; the initial
default run had one. The named installed files were readable and matched their
package checksums. A focused run of all 42 `quest-build` tests passed, and the
unchanged-source full reruns above passed. The default rerun traced failed
`openat`/`openat2` calls and recorded no failures for the named CMake modules.
The original intermittent mechanism remains unknown: these follow-ups do not
establish a fix, and traced timing is not a performance comparison. No source,
native library or installation patch was made in response.

Checkpoint 06 adds the mixed traction foundation, complete P2 cylinder snapshot
and native prepared weighted matching. Its
[tested 789-file snapshot](data/2026-10-05-workspace-programme/checkpoint-06-tested-source.json)
has digest `f12c59455b05b563ec996dcc16c60a75a3553488e4481dd396864ad492c4d163`.
The [receipt](data/2026-10-05-workspace-programme/checkpoint-06-verification.json)
preserves all original gate results and a separate formatting follow-up.

| Check | Checkpoint 06 result |
| --- | --- |
| All-feature workspace tests | 1,581 passed, six skipped; 261.666 s |
| Default-feature workspace tests | 1,418 passed, one skipped; 89.019 s |
| All-feature doctests | 65 passed, one ignored, 24 suites |
| Strict all-feature/all-target Clippy | Passed, including after formatting |
| Default all-target checking | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Formatting | Initial test-assertion wrapping mismatch; corrected check passed |
| Git whitespace | Passed before and after formatting |
| Affected native test after formatting | One passed |

Rustfmt expanded one unchanged assertion in `matching_runtime.rs`. No production
source changed after the broad tests. The separately
[formatted source snapshot](data/2026-10-05-workspace-programme/checkpoint-06-source.json)
has digest `c537bbb1e00e4e04ab5957baf6f31ce39ae49f525fe9d4192634bae651da5c4b`.
The receipt binds the exact before/after file hashes, whitespace-only delta and
focused follow-up checks. The full suite was not rerun for this test formatting
change. Earlier receipts and their original source identities remain unchanged.

No CMake module-open failure occurred in these two full test runs. The earlier
intermittent cause remains unresolved; this successful checkpoint does not
establish a native installation fix.

Independent review reproduced a rank-local storage-overflow deadlock using an
accounting-only reservation. Collective agreement now precedes the affected
broadcasts. The same bounded two-rank probe rejects normally on both ranks;
the regression is included in the passing checkpoint tests.

Checkpoint 07 adds the complete P2 evolution consumer and the prepared weighted
matching QSVT transform. Its [804-file source snapshot](data/2026-10-05-workspace-programme/checkpoint-07-source.json)
has digest `0e08426d4e0f302c334d74e4ebbb7e0ccded47ae83ace41ecb4aa18d3bbc78d0`.
That source remained unchanged before and after every gate in the
[verification receipt](data/2026-10-05-workspace-programme/checkpoint-07-verification.json).

| Check | Checkpoint 07 result |
| --- | --- |
| All-feature workspace tests | 1,601 passed, six skipped; 282.414 s |
| Default-feature workspace tests | 1,436 passed, one skipped; 89.403 s |
| All-feature doctests | 65 passed, one ignored, 24 suites |
| Strict all-feature/all-target Clippy | Passed with `--no-deps -D warnings` |
| Default all-target checking | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Formatting and Git whitespace | Passed |

No retry or formatting change was needed during this checkpoint. The P2
campaign's failed temporal sensitivity remains unchanged by these tests.
Native whole-register and independent inverse fixtures remain distinct from
certified physical error and multi-host capacity. Source/log hashes identify
the tested content; the cylinder's separately scoped build attestation has
its own explicit provenance and exclusions.

Checkpoint 08 adds the later loader, fingerprint, configuration-diagnostic and
persisted weighted-inverse work. Its [975-file snapshot](data/2026-10-05-workspace-programme/checkpoint-08-source.json)
explicitly includes crate Markdown used by doctests and external verification
fixtures; this is broader than checkpoint 07's source selection. Digest
`83373206f9f5239a83d2737b20e14a9e0e2d9b90d8daae4c3ff65f0b1f45b228`
remained unchanged throughout all eight gates.

**Checkpoint 08 failed.** Its [receipt and retained logs](data/2026-10-05-workspace-programme/checkpoint-08-verification.json)
record 1,644 all-feature tests passed, one failed and six skipped; 1,458 default
tests passed, five failed and one skipped. All 65 doctests passed with one
ignored. Workspace checking, strict Clippy, binding freshness, formatting and
whitespace passed. The MPI failure test satisfied its prompt nonzero termination
checks but missed launcher-delivered diagnostic text. The five default failures
were the recurring CMake module-open problem; its cause remains unresolved.
Neither later focused success nor the completed sparse inverse overrides these
failed gate outcomes.

Checkpoint 09 changes only the MPI failure test in that 975-file source scope.
The [test-only correction](data/2026-10-05-workspace-programme/checkpoint-09-fatal-witness/README.md)
uses durable native-error and peer-readiness witnesses, rejects deadline and
forced-cleanup outcomes, and checks that the fatal boundary does not return.
It also runs with stderr discarded. Production Rust/C++ execution is unchanged.

All eight gates passed against the [final frozen source](data/2026-10-05-workspace-programme/checkpoint-09-source.json),
digest `2111d129965cb6a473fc8b376a3a839f68732f51267b618f706f5c48a0028669`.
The [final local verification receipt](data/2026-10-05-workspace-programme/checkpoint-09-verification.json)
records the exact single-file delta, before/after identity and each original
and path-normalized log hash.

| Check | Checkpoint 09 result |
| --- | --- |
| All-feature workspace tests | 1,645 passed, six skipped; 311.320 s |
| Default-feature workspace tests | 1,463 passed, one skipped; 86.759 s |
| All-feature doctests | 65 passed, one ignored, 24 suites |
| Strict all-feature/all-target Clippy | Passed with `--no-deps -D warnings` |
| Default all-target checking | Passed |
| Generated binding freshness | Passed with matching native MPI |
| Formatting and Git whitespace | Passed |

The original environment and test scheduling were retained. No CMake module-open
failure occurred in these final full runs. The [diagnostic record](data/2026-10-05-workspace-programme/checkpoint-08-cmake/diagnosis.md)
retains the unresolved earlier mechanism and its focused PATH deviation; these
successful gates do not establish an installation fix. Local repository
acceptance remains separate from the scientific and external capacity gates below.

Reproduce the repository checks from its root with generic installation paths:

```sh
export QUEST_ROOT=<installed-quest-prefix>
export MPICC=<matching-mpi-prefix>/bin/mpicc
export PATH=<matching-mpi-prefix>/bin:$PATH
cargo nextest run --workspace --all-features --no-fail-fast
cargo nextest run --workspace --no-fail-fast
cargo test --workspace --all-features --doc
cargo check --workspace --all-targets
cargo clippy --workspace --all-features --all-targets --no-deps -- -D warnings
cargo fmt --all --check
cargo run --locked -p xtask -- generate-quest-bindings --check
python3 -B docs/verification/fixtures/sparse-capacity/test_run.py
python3 -B docs/verification/fixtures/cylinder-window/test_run.py
python3 -B docs/verification/fixtures/quest-cfd/test_box_windows.py
python3 -B docs/verification/fixtures/quest-cfd/test_paired_history.py
python3 -B crates/quest-cfd/tests/test_cylinder_evolution_runner.py
```

Existing nightly generic-constant-expression compiler notices are separate
from Clippy failures. Numerical indexing/arithmetic, exact bit comparisons and
bounded test-fixture lint allowances are explicit in their source scopes; the
static record inventories new allowances rather than implying blanket lint absence.

The [static audit inventory](data/2026-10-05-workspace-programme/static-audit.json)
records its source snapshot and explicit lint scopes: 176 new or amended
attributes (175 `allow`, one `expect`), all with reasons, including three inherited
affine/backend relocations. It found no personal paths or broken relative links
in its declared public-text scope. Both Python fixture suites passed (14 tests),
and saved completed capacity/cylinder receipts passed semantic revalidation.
Its 201 overlapping source hashes match the independently captured checkpoint;
later observable, boundary and cost-report work is excluded. It is not a
replacement for Rust testing or a claim about those later source changes.

The separate [checkpoint 02 static inventory](data/2026-10-05-workspace-programme/static-audit-02.json)
records 203 explicit attributes (202 `allow`, one `expect`), all with reasons,
and distinguishes source scopes from test/example scopes, including three
inline `cfg(test)` sites. Its 217 overlapping source hashes match checkpoint 02.
The earlier Python validator files and campaign receipts are unchanged; their
14 passing tests remain historical evidence rather than newly executed tests.
Publication-time privacy and relative-link scans are recorded separately from
the frozen source inventory.

The separate [checkpoint 03 static inventory](data/2026-10-05-workspace-programme/static-audit-03.json)
records 219 explicit attributes (218 `allow`, one `expect`), all with reasons,
including five inspected inline `cfg(test)` sites. Its 227 overlapping source
hashes match the frozen checkpoint 03 snapshot. Twelve new box-window validator
tests passed independently; the earlier 14 fixture tests remain historical.
The final named documentation delta has no privacy hits or broken relative
targets. Later paired-stage source and documents are outside this snapshot.

The [checkpoint 04 static inventory](data/2026-10-05-workspace-programme/static-audit-04.json)
records 225 explicit attributes (224 `allow`, one `expect`), all with reasons,
including seven inspected inline test-module sites. Its 233 overlapping source
hashes match checkpoint 04. Both paired-history and box-window validator suites
passed 12 tests. Independent revalidation also recomputed all nine follow-up
comparisons and checked the distinct campaign/build source scopes against the
frozen checkpoint. Earlier receipts retain their original identities and limits.

The [checkpoint 05 static inventory](data/2026-10-05-workspace-programme/static-audit-05.json)
records 241 explicit attributes (240 `allow`, one `expect`), all with reasons,
including eight inspected inline test-module sites. Its 240 overlapping source
hashes match checkpoint 05. The exact fixture script and its raw data are
separately inventoried outside the build manifest. The affine documentation
distinguishes ideal-rational certificates, floating runtime evidence, modeled
payloads and the unbounded host-environment discovery limitation. Earlier
checkpoint and campaign receipts retain their original bytes.

The [checkpoint 06 static inventory](data/2026-10-05-workspace-programme/static-audit-06.json)
records 264 explicit attributes (263 `allow`, one `expect`), all with reasons,
including ten inspected inline test-module sites. It retains the tested-source
capture and separately accounts for the single formatted test file. Mixed
boundary, native LCU and cylinder records keep their distinct focused identities;
historical hashes are not rewritten to imply current-source verification.

The [checkpoint 07 static inventory](data/2026-10-05-workspace-programme/static-audit-07.json)
records 287 explicit attributes (286 `allow`, one `expect`), all with reasons,
including sixteen inspected inline test-module sites. It independently checks
the P2 evolution publication and prepared-transform source/log identities,
preserving the former's failed accuracy result. Its five new Python runner
tests passed; earlier validator results remain historical evidence. The
frozen source, publication snapshots and later documentation refresh retain
separate scopes.

The [checkpoint 08 static inventory](data/2026-10-05-workspace-programme/static-audit-08.json)
retains its frozen 975-file source, 324 explicit lint attributes with reasons,
and separately checked publication/doc additions. It records failed repository
gates despite clean static checks. The subsequent test-only source change and
checkpoint 09 results do not rewrite that historical snapshot.
The [checkpoint 09 static inventory](data/2026-10-05-workspace-programme/static-audit-09.json)
checks the final source delta, current document links, published artifact hashes
and passing gate receipts separately from that failed historical checkpoint.

## Acceptance still open

The literal Re300 three-dimensional wake needs its pressure/outlet specification
resolved before its coupled boundary operator can be implemented. The
[pressure-coupled dynamic-traction proposal](../../crates/quest-cfd/docs/research/2026-10-05-dynamic-traction-comparison.md)
is a distinct comparison, not an implemented reproduction of the source case.

The generated polynomial-time lifting/forcing producer and bounded higher-order,
fixed-time pressure/traction preparation now have scoped implementations and
evidence above. Broader physical mesh/boundary
acceptance, validated total observable/sampling errors, accuracy-dependent
resource comparisons and resolved six-family observation-window/refinement
studies also remain open. Register counts and successful allocations cannot
substitute for these.

Actual multi-host input larger than each node's memory cap and real large-count
transport remain separate external acceptance gates. Local loopback ranks,
synthetic count limits and address-space caps do not close them. No quantum
hardware, accelerator execution, convergence or speedup is inferred from the
implemented CPU simulator paths. The detailed [CFD backlog](../../crates/quest-cfd/NEXT_STEPS.md)
retains these obligations.
