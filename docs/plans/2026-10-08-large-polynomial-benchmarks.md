# Large-polynomial limits and scientifically matched NLFT benchmarks

Status: approved for implementation on 2026-10-08. This records the user's
approved plan and its implementation boundaries. Existing consolidation edits
and historical receipts must be preserved. No commit, merge or push is requested.

## Global constraints

- Safe first-party Rust; supported public QuEST/QSVT APIs only; no upstream
  numerical algorithm patches, private-interface bypasses or compatibility hacks.
- Keep hdf5-metno. Use existing locked devenv for native dependencies.
- All execution is local. Native configure/build/execution uses
  `systemd-run --user --scope -p MemoryHigh=24G -p MemoryMax=28G`.
- Serialize heavy commands with `/tmp/quest-quality-build.lock`; use disk-backed
  temporary storage and preserve old result directories.
- C++ versus Python stays in the main paper; Rust is an appendix. Headline
  comparisons are matched forward and inverse transforms. Completion and public
  solvers have separate cost/guarantee reporting because C++ certifies completion.
- Latest stable Python at campaign freeze, currently nlft-qsp v2.1.0 commit
  `4f071fbd8ef781516dadee9e91b3507c7b83937e`; pin interpreter and dependencies.
- Degree 1,000,000 means 1,000,001 coefficient/reflection slots.
- Main observed accuracy gate 1e-10, strict breakdown 1e-12, neither grows with
  degree. Keep existing source-fixture tolerances unchanged in a separate lane.
- Generic paths in checked-in documents and receipts; no personal machine paths.

## Task 1: Unified resources and workspace migration

Introduce ShapeLimits (coefficients, padded FFT length, completion grid),
ResourceLimits (peak modeled bytes, cumulative work), independent AccuracyPolicy,
OperationResources (shared accounting), and structured ResourceReport. Migrate
numerics, polynomial, QSP, MathCore integration and workspace callers. Safe RAII
reservations follow owned buffer/workspace lifetimes; cached allocations stay
charged and shared storage is counted once. Work is monotonic across retries,
inverse and reconstruction. Parallel reservations are deterministic before
dispatch. Use checked arithmetic and batch work charges, not hot scalar locks.

Reusable inverse/forward workspaces own plans/scratch; convenience calls use the
same implementation with temporary workspaces. Opaque RustFFT plan estimates stay
charged while retained and remain estimates, not allocator guarantees. Preserve
conservative defaults. Explicit million profile: coefficients 1_000_001, FFT
2^21, peak 8 GiB, cumulative work 2^37. Completion at this degree starts at 2^22
and is a separate capacity gate. Do not promise that larger limits fix quadratic
algorithms, conditioning or certification.

Tests: release/error paths, overflow, cumulative retries/stages, deterministic
parallel reservations, FFT padding boundaries, allocation reuse; document API
migration. Execute all 17 native exact inverse fixtures and independent forward
fixtures with unchanged source accuracy gates, measured memory and resource
reports. Admission alone is not validated capacity.

## Task 2: Corpus, independent oracle and Python adapter

SoftwareX benchmarks/nlft is authoritative for corpus, manifests, schemas,
validation, controller, statistics and figure exports. Thin adapters consume one
contract; Rust adapter remains in quest-rs. Freeze JSON metadata and HDF5
binary64 arrays with exact hashes, supports, lengths, source degrees, per-case
seeds and transformation provenance. Physical a has support [-d,0], b [0,d].
Rust conjugate-complement storage reverses and conjugates physical a.

Forward receives independent shared reflections; inverse receives a shared
prepared scattering pair. Never prepare forward via the implementation under
test's inverse. Define normalization, phase, conjugation and output ownership.
Preserve 62 historical cases times two legacy boundaries in a migration ledger;
these are historical execution contracts, not 124 invalid mathematical inputs.
No silent trimming/scaling/omission or offset loss. Keep application approximation
error separate. Replace quadratic sampled Horner with supported FFT evaluation;
sampling is not a contractivity proof.

Independent oracle: balanced transfer-product reference, direct transfer at 32
reproducible held-out circle points, small degree <=64 cross-check at 100 decimal
digits. Validate actual returned outputs. Inverse measures reconstruction
backward residual. Align supports; report pair coefficient Linf, mixed
absolute/relative L2 and observed response error. Distinguish oracle disagreement
or inconclusive results from implementation accuracy failure. Bounded higher
precision checks may resolve uncertainty, never relax thresholds. Cache a verdict
only with identical output bytes and complete validator/input identity.

Tests: exact HDF5 readback; sign/order/conjugation/offset/phase/degree corruption;
nonfinite/malformed outputs; high-precision small cases and independent fixtures.

## Task 3: Controller, measurement and statistics

Optimized single-physical-core baseline with affinity and explicit thread/backend
provenance; retain supported SIMD. Measure operation entry to owned result return,
including internal allocation/planning/production checks. Exclude input loading,
construction, external validation, serialization and caller result destruction.
Validate every timed output after stopping the clock and release it; a failed
sample invalidates process comparison without erasing observations. Fresh plans
do not mean cold hardware caches. Reusable-workspace comparisons require matching
supported interfaces, otherwise record unsupported.

Rust uses actual cargo nextest bench with Criterion custom timing/raw samples,
not runner wall time. Pilot freezes warmup/sample settings before publication.
Collect 20 randomized independent process blocks per headline case, complete
pairs within each session; distinguish within-process samples. Per-case paired
ratios and 95% block bootstrap confidence intervals, accounting for sessions.
No pooling unrelated families/degrees/contracts into an aggregate headline.

Controller owns immutable versioned experiment identities, append-only attempts,
phase deadlines, process-tree cleanup, 12-hour resumable sessions and atomic
completion receipts. Unsupported/admission/accuracy/oracle/timeout/OOM/interruption/
infrastructure are distinct; missing values are null, not zero. Publication
rejects incompatible contracts, missing identities and selected retries.

Tests: identity mismatches, failed samples, interrupted sessions, duplicate
attempts, phase-specific timeouts/censoring and incomplete campaigns; reproduce
all reported values from raw receipts.

## Task 4: Native and Rust adapters, integration and review

Implement thin public forward/inverse adapters against the shared artifact and
measurement contract. Do not resurrect old NLFTSolver CLI semantics or bypass
certified completion. Use HDF5 already present in each language. Rust employs
the new explicit resources and actual nextest bench. Independently review resource
accounting and methodology before publication measurements. Run affected Rust
tests, full workspace acceptance, formatting and strict Clippy.

## Task 5: Pilot, frozen campaign and paper

Run staged capacity/oracle pilots; freeze protocol and latest stable dependency
identities; collect publication observations in local <=12-hour resumable
sessions. Retain failed/unsupported outcomes, source gates and raw provenance.
Replace unsupported old claims and regenerate figures from verified receipts,
showing accuracy and coverage beside runtime. Rust remains in the appendix.
Distinguish measured performance, empirical numerical accuracy, certified
guarantees and resource capacity. Do not publish unsupported success claims.
