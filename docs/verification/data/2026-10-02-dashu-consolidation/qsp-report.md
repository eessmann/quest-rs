# QSP native Dashu migration

The native migration, full ordinary QSP suite and strict all-target Clippy are complete. Explicit large-scale release fixtures are delegated to the root integrated gates.

## Scope and implementation

- Changed only `crates/quest-qsp` plus this requested report; preserved existing unrelated/pre-port work. No commits.
- Replaced Rug values with a native `FBig<HalfEven, 2>` type alias. No compatibility float wrapper, backend dispatch, algorithm fallback, or tolerance changes.
- Explicit typed native `Context<HalfEven>`, `Context<Down>`, and `Context<Up>` perform arithmetic through fallible context APIs. Native domain, exponent range and Ziv retry failures remain typed `PrecisionError::Arithmetic(FpError)` values. Nonfinite results remain rejected.
- QSP keeps its independent complex rectangle, interval FFT, complete matrix product, response, unitarity, projector conversion and Fourier lower-witness algorithms. Only exact dyadic binary64 interchange is shared with quest-numerics.
- Every synthesis/verifier attempt owns a `ConstCache`; trigonometry and logarithms/exponentials borrow it. Constant cache word storage is inspected and admitted at each completed attempt. Precision retries reconstruct attempt state from original source data.
- Preserved original algorithm identity, inverse NLFT default, explicit precision retries and complete terminal/full phase responses.
- Removed stale word-alignment checks, including historical receipt and frozen synthesis precision artifact checks. Precision policies admit individual bits. Existing wire payloads and QSP artifact version remain unchanged.
- Direct native math helpers are scalar QSP boundaries, not value wrappers: they apply typed Context calls, preserve native FBig representation and forward typed errors. The one u32-to-usize widening has a focused lint expectation with a compile-time target-width invariant; every native-to-wire precision conversion uses checked try_from.
- Native stored guard digits are preserved, not rerounded at endpoint admission. Endpoint storage validates actual digits against precision plus one; the conservative scalar storage model includes that guard bit. Adaptive transcendental scratch and allocator overhead remain outside modeled application byte limits.
- Ported independent seeded binary64 dyadic fixtures to Dashu IBig/FBig while retaining signed-zero, gradual-underflow, limb-sticky, midpoint-neighbor, overflow and directed-rounding coverage.
- Use native `Context::sin_cos` for each directed exact-angle interval, nearest complex exponential, FFT root, and real-parity phase control. Both correctly rounded outputs retain the same represented dyadics and typed error boundary. Scalar nearest sine/cosine helpers remain test-only references. Standalone directed primitives remain in the independent interval algorithms where inputs differ.
- Hoisted the unchanged nearest pi value outside the FFT root loop. The length-one transform still creates no twiddles or pi cache; root reuse and modeled work charges remain unchanged. No logarithm algorithm change was introduced.

## Test evidence

- Before native migration, added `offline_solver_65_bit_precision_and_original_support_are_retained` and ran it in the project-local devenv with `CARGO_BUILD_JOBS=2`.
- Initial compilation exposed the pre-existing stale references to deleted `word_bits` in `offline/mod.rs:163-164`; removed only those stale alignment checks.
- Re-run demonstrated RED: artifact load failed with `Invalid("historical attempt")` at `tests/artifact.rs:162` for the 65-bit certified offline result. This establishes the artifact admission regression.
- `CARGO_BUILD_JOBS=2 devenv shell -- cargo test -p quest-qsp --all-features --lib --test artifact --test float_conversion`: passed 31 unit tests (one degree-8192 scale fixture ignored), all 3 artifact tests and all 6 exact interchange tests. The 65-bit regression is GREEN for both explicit factorization algorithms.
- Native library compiled with all features. First test compilation exposed only direct API mismatches (ConstCache is exported at crate root; zero comparison and test matcher borrowing); these were corrected.
- Owned Rust sources formatted; `git diff --check -- crates/quest-qsp` passed.
- `CARGO_BUILD_JOBS=2 devenv shell -- cargo test -p quest-qsp --all-features`: GREEN, 107 passed and 3 explicitly ignored scale fixtures. Re-ran after lint/precision-retry cleanup with the same result. This includes default-NLFT identity, explicit RHW, both phase conventions, final controls, independent full-response/product/projector checks, direct-versus-FFT overlap, bit-granular offline precision, bounded retry policy, shared Remez/QR consumer tests, caller-owned Rayon pools, and all doctests.
- `CARGO_BUILD_JOBS=2 devenv shell -- cargo clippy -p quest-qsp --all-features --lib -- -D warnings`: GREEN.
- Final `CARGO_BUILD_JOBS=2 devenv shell -- cargo clippy -p quest-qsp --all-targets --all-features --locked -- -D warnings`: GREEN after narrowly correcting test-only lint findings (method reference, googletest expectation instead of assertion in Result test, justified exact native dyadic fixture arithmetic expectations).
- Final changed-test validation `CARGO_BUILD_JOBS=2 devenv shell -- cargo test -p quest-qsp --all-features --locked --lib --test offline`: 31 library and 17 offline tests passed, two scale fixtures explicitly ignored. Dependencies retain the existing nightly generic_const_exprs/next-solver warning. The QSP library emits no warnings.
- The ignored fixtures are `certification::tests::degree_8192_analytic_product_is_certified_by_interval_fft`, `offline_original_degree_8105_catalog_exports_and_certifies`, and `degree_8105_catalog_preserves_canonical_and_generalized_bits`. Per root direction, explicit release execution is part of the root integrated gates.
- Feature-disabled/certification-only builds and workspace-wide supported-configuration gates remain root-owned.
- Added four characterization regressions before their corresponding paired-call replacements: directed exact-angle intervals, nearest complex exponential, FFT roots/cache reuse/length-one resources, and phase-control entries. Exact native values and signed zeros match the original separate correctly rounded calls at 65, 128 and 256 bits; interval fixtures also include adjacent quadrant values, subnormal inputs, large finite phases and maximum binary64. Complex exponential retains the typed infinite-input failure. The phase-control work fixture initially omitted the existing precision-limb charge and was corrected before production changes.
- Final optimization gate `CARGO_BUILD_JOBS=2 devenv shell -- cargo clippy -p quest-qsp --all-targets --all-features --locked -- -D warnings`: GREEN.
- Final optimization gate `CARGO_BUILD_JOBS=2 devenv shell -- cargo test -p quest-qsp --all-features --locked`: GREEN, **111 passed, 0 failed, 3 explicitly ignored** including doctests. Complete output is captured in `/tmp/quest-qsp-final-tests.log`; totals were summed from all 18 suite result records. This supersedes the earlier ordinary suite count. No performance improvement is claimed until the root repeats the matched-accuracy benchmark.

## Limits / concerns

- No new Linux, MPI, accelerator or external homogeneous-performance evidence is claimed.
- Backend allocation failures and adaptive scratch are not recoverable process-wide allocation limits; the crate maintains conservative application storage and work admission.
- Final matched-accuracy benchmark, ignored release scale fixtures and workspace-wide validation are owned by the root coordinator.
