# Generic polynomial storage and recurrence consolidation

Ownership: `src/{basis,polynomial,analysis,norm}.rs`, `tests/generic_storage.rs`, and migrated existing polynomial tests/examples. Function/Remez orchestration belongs to root, arithmetic/AD belongs to numeric foundation owner.

## Implemented contracts

- `Polynomial<B,C=Complex64,D=DynamicShape>` retains its exact owned scalar coefficients and checked sealed shape. `DynamicShape(pub usize)` and unit `StaticShape<const COUNT:usize>` include the existing empty zero case. `with_shape` checks the retained count without replacing coefficient ownership.
- `from_scalars(basis,coefficients,shape,&mut backend,limits)` validates finite/backend-valid coefficients, effective signed support, stored capacity, actual backend scalar heap storage, and checked work limits. MP coefficients are retained without precision conversion; imported mantissas larger than the selected backend precision still count their actual storage.
- Point, enclosure, and structural Jet evaluation share the same generic recurrence. Enclosures lift stored coefficients using singleton endpoints. `evaluate_with`, `evaluate_enclosure`, `jet_with`, and `jet_enclosure` return crate `Result` with backend errors converted into the public arithmetic error boundary.
- Basis recurrence coefficients are computed in selected backend arithmetic from exact integers and retained binary64 parameters. Existing f64/interval basis helpers delegate to this authority. No rounded f64 rational coefficient is lifted into interval/MP arithmetic.
- Effective support trims irrelevant zero storage before evaluation. Removable Laurent negative powers are skipped; genuine negative support retains the reciprocal/pole check. Constant and zero shortcuts still validate inputs and cannot trigger irrelevant extreme Jacobi recurrence overflow.
- Deliberate complex methods, derivatives, cold basis conversion, parity and norm evidence remain complex specializations. Static source shape is retained by evidence; transformations changing the coefficient count return dynamic shapes.

## Focused verification

The integrated crate check initially exposed the expected missing shape API while root Function/Remez integration was in progress; subsequent integrated checks still depend on the root-owned Remez modules. A standalone ignored fixture imports the four current source modules directly and excludes root-owned Function/Remez. It is a scoped storage/recurrence gate, not evidence of the entire new numerical engine.

`devenv shell -- env CARGO_BUILD_JOBS=2 cargo test --manifest-path .superpowers/sdd/2026-10-02-static-numerical-core/polynomial-storage-smoke/Cargo.toml --target-dir target --test generic_storage --test bases --test norms` passed 21 tests: 8 new shape/finite/support/backend/storage/AD tests, 6 existing basis tests, 7 existing norm tests. Log: `polynomial-storage-smoke.log`.

The same direct-source library passes scoped Clippy with warnings, pedantic, nursery, unchecked arithmetic, casts, and indexing denied. Log: `polynomial-storage-clippy.log`. Owned files were formatted individually. Linux/accelerator validation is not claimed.

## Caller migration

Existing tests and examples now use canonical `function!`, `GenericFunction` backend evaluation, and `RemezRequest`. The obsolete `typed_function` test suite is renamed `functions`; independent analytic derivative, domain, static count, assumed extension, resource, and retained request regressions remain. Dynamic representation comparisons and historical dynamic syntax saturation cases are removed with the interpreted implementation. The two representation measurement examples are consolidated into one `function` example that measures the canonical implementation.

Integrated no-run and runtime gates now pass against the root-owned canonical Function/Remez engine: 46 tests (analysis13, bases6, functions8, generic_storage11, norms7, warm_evaluation1). The warm scalar/interval/derivative evaluation test retains zero allocations. Logs: `polynomial-callers-build.log`, `polynomial-callers-tests.log`.

Three further storage regressions cover signed-shift work/support overflow, static complex conversion/norm source evidence, and finite linear Jacobi evaluation without unused higher recurrences. Clenshaw begins at the leading retained coefficient and requests only used recurrence terms, preserving the large-parameter linear value and derivatives.

The owned caller/example Clippy gate now passes (`polynomial-owned-callers-clippy.log`) against the canonical library. Earlier all-target attempts recorded root-owned library/test lint dependencies; parent subsequently reports the full all-target polynomial gate passing. The latest strict direct-source library gate also passes after the leading-term recurrence refinement (`polynomial-storage-clippy.log`).

## Final scientific support and retained-storage contract

The review identified that cached support is a mathematical invariant of the constructor's selected arithmetic. The final design documents PointBackend comparison/arithmetic laws and retains the admitted cache for point, derivative, enclosure and complex evaluation, avoiding repeated singleton scans in root/proof callbacks. Remez orchestration performs one independent coefficient/support admission at candidate publication before using that cache for proof. There is no complex provenance runtime flag or per-evaluation defensive scan; invalid custom arithmetic is outside the documented backend contract. The parent owns that publication admission and its focused regression.

`retained_heap_bytes(&backend)` now exposes the shared coefficient backing allocation: Vec capacity, Vec/Arc allocation headers, and actual scalar-owned heap words from Backend::storage_bytes. Polynomial/basis/shape inline storage is excluded; an Arc clone shares this allocation and must not be charged again by callers that deduplicate ownership. Admission and this accessor use the same accounting authority. A focused capacity/header regression was added. The final integrated workspace and subsequent focused receipts supersede the temporary support-recompute experiment's `support-green.log`; that experiment is not the final production design.

Final feature-enabled integrated evidence: `workspace-features-nextest.log` records 987 passed and 8 skipped. This includes the retained-capacity/header regression and the final constructor-cache/Remez-publication design. The workspace Clippy pass initially exposed one owned documentation-markdown issue; that comment was corrected, with the final complete receipt delegated to the parent. No Linux/MPI/accelerator inference is made from this Darwin run.

The final workspace all-target Clippy receipt (`workspace-clippy.log`, complete dev profile) now passes with the selected feature matrix, including the owned storage, caller, example and runtime changes.
