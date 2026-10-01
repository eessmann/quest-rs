# Task 2: mathematical utility parity

Implemented original-coefficient, bounded binary64 utility APIs in:

- `quest-polynomial/src/analysis.rs`: `Polynomial<Laurent>::to_chebyshev_symmetric`.
- `quest-polynomial/src/norm.rs`: `NormDomain`, `NormOptions`, `NormOutcome`, immutable `NormEvidence`, `NormStatus`, and generic `Polynomial<B>::certify_norm`.
- `quest-numerics/src/contractors.rs`: scalar extended Newton, Krawczyk, scalar Hansen–Sengupta and dynamic square vector Hansen–Sengupta; immutable input-bound results and explicit dimension/branch/byte/work limits.
- `quest-qsvt/src/pauli.rs`: complete dense Pauli decomposition/reconstruction and tensor labels under explicit exponential work/storage admission.
- Additive exports in the three crate roots and analytic integration tests.

## Mathematical contracts and supported scope

The Laurent conversion uses `z^k+z^-k=2*T_k((z+z^-1)/2)`. Absent stored coefficients are exact zeros, and symmetry is exact inversion symmetry `a_k=a_-k`, not conjugation symmetry. Complex coefficients are supported. The immutable `Conversion` retains the source and outward l1 rounding bound. Multiplication by two is exact when representable; overflowing output is rejected before returning evidence. C++ `polynomial/core.hpp` lines 951 onward admits approximate symmetry at its machine threshold and requires zero in allocated storage. Rust deliberately requires exact symmetry, admits effective symmetric support surrounded by stored zero padding, and never silently projects an asymmetric source.

Norm certificates cover every supported polynomial basis, original basis parameters, complex coefficients on real closed segments, and full closed complex discs with arbitrary finite center and nonnegative radius. Generalized interval basis recurrence encloses the original polynomial. Each cell has an interval upper bound; interval point evaluations provide lower bounds only. Refinement never substitutes a sampled maximum for an upper certificate. For pole-free discs, the maximum-modulus principle justifies bounding the whole disc by its boundary. Genuine Laurent poles use effective negative support: zero storage entries do not create a pole. A pole inside or on the disc/segment yields `UnboundedPole`; unresolved directed pole membership yields `InconclusivePoleLocation`. The unit-disc norm of a Laurent function with a pole is not its finite unit-circle boundary norm. Initial norm bounds and cell refinement must remain finitely representable; arithmetic failure is an explicit error. `Budget`/`Resolution` return sound finite bounds without claiming the requested tolerance. Parameter endpoints, radius zero, empty/zero polynomials, original support limits, and nonfinite options are handled explicitly. Scaled modulus computation avoids overflow when the actual large real norm is representable.

Contractors require caller callbacks to enclose one continuously differentiable scalar function and its derivative, or one square vector function and its Jacobian, on every supplied interval/box. Callback work, allocation and semantic consistency are caller premises, clearly documented in the module. Results retain input and algorithm provenance. Extended division intersects finite half-line branches with the finite input domain, without a tiny surrogate divisor; denominators containing zero can produce two disjoint images. Every root stays in the returned union under the callback premise. Empty unions certify exclusion; unchanged images are inconclusive. Scalar uniqueness requires a regular derivative and either strict Newton inclusion or an exact point root (Krawczyk additionally verifies a contraction factor below one). Vector Hansen–Sengupta is one sequential Gauss–Seidel sweep of the preconditioned interval system. It retains every branch and falls back to the original box with `InconclusiveBudget` if branch admission would be exceeded. `CertifiedUnique` for vectors additionally verifies independent strict Krawczyk inclusion and an outward infinity-norm contraction bound below one. Singular input never acquires uniqueness evidence simply from contraction. Rust uses dynamic square dimensions instead of C++'s compile-time dimensions; no implicit iterative solver or global pool is introduced.

Dense Pauli coefficients are `c_p=Tr(A P_p)/2^n` for any finite square matrix of power-of-two dimension. All coefficients are retained. Base-4 digits are I,X,Y,Z, with qubit zero least significant; display labels reverse qubit order, so index 13 is ZX. Scalar 1x1 matrices have zero qubits. Phase/sign masks give constant work per computational basis state after per-string setup. Normal unscaled summation preserves subnormal trace coefficients; a bounded normalized restart on partial-sum overflow handles huge representable averages. Coefficients/reconstruction are rounded binary64 numerical results, not exact interval trace certificates. Rust is sequential rather than OpenMP and omits the C++ named-output hard-coded `1e-10` truncation. It has no arbitrary-unitary gate synthesis claim.

## Verification and actual results

All commands ran in the project environment (`devenv shell -- ...`), using its configured nightly toolchain. No C++ source was changed.

- Red contractor run: `cargo test -p quest-numerics --test contractors` failed with missing public contractor imports before implementation.
- Red norm run: `cargo test -p quest-polynomial --test norms` first encountered another task's transient `typed.rs` delimiter error, then failed with missing norm imports/conversion method before implementation.
- Red Pauli run: `cargo test -p quest-qsvt --test pauli` first encountered the same transient dependency error, then failed with missing Pauli imports before implementation.
- Additional red regression `cargo test -p quest-qsvt --test pauli decomposition_preserves_subnormal` failed because pre-normalized summation returned zero instead of the smallest subnormal. The implementation was changed to preserve unscaled sums and normalize only on overflow.
- Additional red regression `cargo test -p quest-polynomial --test norms representable_large` failed with unbounded squaring for a real constant `f64::MAX`. The modulus enclosure was changed to scale before squaring.
- `cargo test -p quest-numerics -p quest-polynomial -p quest-qsvt --test contractors --test norms --test pauli`: passed 17 tests (6 contractors, 7 norms/conversion, 4 Pauli). Cases independently derive linear isolation/exclusion, quadratic ±1 split, coupled 2x2 linear-system root and singular cases, explicit branch budget fallback; `1-x²` interior maximum, `x+i` segment/disc norms, reciprocal off-origin disc and boundary poles, zeros/empty/point cases, huge constants; non-Hermitian trace signs, ZX ordering, full dense roundtrip, invalid shapes/nonfinite/budgets, smallest-subnormal and maximum-finite identity.

Task-specific Clippy and final formatting status will be appended below. Parent integration owns the final broader workspace gate; no commit created.

- `cargo clippy -p quest-numerics -p quest-polynomial -p quest-qsvt --lib --test contractors --test norms --test pauli --message-format short`: passed. The shared polynomial crate emits the other task's known nightly `generic_const_exprs`/next-solver warning; no utility Clippy errors or warnings remained.
- `rustfmt --edition 2024 --config skip_children=true` on all owned sources/tests and additive crate roots: passed. Child-module traversal was disabled to avoid formatting another implementer's in-progress files.

## Independent review repair: vector byte admission

The independent generic owner correctly identified that the original byte
model omitted nested Vec metadata and geometric growth. A new boundary test
failed first (239-byte admission incorrectly succeeded). The repair models
simultaneously live matrix cells, center/right-hand-side arrays, two exact
branch generations, retained input/fallback copies, two division scratch cells,
and nested row/list Vec headers. Owned allocations now use fallible exact
reservation, and final absolute-coordinate translation reuses the admitted
branch buffers. Fixed local stack values, allocator bookkeeping and callback
allocations remain explicitly outside this heap-buffer admission model.

For one dimension and one admitted branch the conservative peak model is nine
interval cells (144 bytes) plus four nested Vec headers (96 bytes), 240 bytes.
The regression independently checks 239 rejects and 240 succeeds.

- `devenv shell -- cargo test -p quest-numerics --test contractors`: passed all
  seven tests after the repair.
- `devenv shell -- cargo clippy -p quest-numerics --lib --test contractors --message-format short`:
  passed after correcting the new documentation paragraph length.
- Project rustfmt on the repaired contractor source/test passed.
