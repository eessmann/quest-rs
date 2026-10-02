# Independent numerical-foundation review

Reviewed `arithmetic.rs`, exact interchange, `interval.rs`, `ad.rs`, `roots.rs`, `shapes.rs`, and `arithmetic/budget.rs`. This review is independent of my compiler and QR implementation ownership. No compiler/QR self-review was substituted for it.

## Findings fixed after parent authorization

1. **Scalar root-cover split admission (P1)**: `cover` previously committed covered branches before checking total box count; overflow then retained all children. A one-box budget could produce two unresolved branches, or two covered branches and `complete=true`. Admission now includes proposed covered, excluded, unresolved, and child boxes atomically before committing; shortage retains the original parent unresolved. Regression covers tight and loose tolerance.
2. **Rational operand exponent policy (P2)**: fixed-width `ExactConstant::Rational` did not validate constructed numerator/denominator before division, unlike string `Ratio`. Zero numerator or large equal operands bypassed exponent admission. Both operands now validate before division, tested for point and enclosing MP backends.
3. **Vector contractor retained storage (P1)**: admission counted one matrix despite simultaneous callback Jacobians and preconditioned matrix; callback-returned higher-precision limbs were not admitted. Unused center Jacobian and box value now drop immediately; conservative admission includes preconditioner, retained source Jacobian, output matrix, scalar I/O/scratch, and simultaneous branch buffers. Retained callback outputs are re-admitted using actual scalar storage before new workspaces allocate. Budget shortages return the original box without existence/uniqueness claims. Tests cover pre-callback matrix admission and unexpectedly wide MP callback values.

## Mathematical and architectural assessment

- Extended division retains both sign branches and returns the full domain when numerator and denominator both contain zero. The sign reversal and ray endpoints use enclosing arithmetic.
- Scalar Newton uniqueness comes from derivative exclusion; existence additionally requires a witnessed exact center root or strict interval inclusion. Krawczyk uses the independent contraction/inclusion gate. Vector Hansen--Sengupta keeps branch-preserving Gauss--Seidel distinct from Krawczyk existence/uniqueness evidence.
- These conclusions remain conditional on the stated continuously differentiable callback enclosure premise. Arithmetic admission does not establish the callback premise.
- Sealed `CertifyingBackend` admits interval64 and directed MP interval arithmetic (and the transparent budget wrapper), not point backends. AD arithmetic follows structural product/chain/reciprocal rules without erasing backend choice.
- MP directed endpoints are checked before order/comparison, stored dyadics clear inherited inexact flags, and trig extrema detection conservatively widens uncertain large argument reductions. No new false-enclosure or false-root-existence path was found under these contracts. This is a code review conclusion, not a formal proof of upstream libraries.
- MPFR tests provide independent sampled corroboration, not the mathematical admission premise or a proof of universal correctness.

## Additional findings fixed after parent authorization

4. **Identity AD input admission (P2)**: identity expressions could return invalid seeds without invoking arithmetic. Added `Backend::validate` with explicit audited backend implementations, charged delegation in `BudgetedBackend`, recursive First/Jet/Gradient component checks, and admission in seed constructors and Jacobian inputs/outputs. The default custom-backend hook documents its scalar-validity responsibility. Admission preserves the stored value without lower-precision arithmetic or rounding; a wide MP seed regression checks this directly. Private MP validation methods were renamed `validate_value` to avoid shadowing the public trait hook.
5. **Failure paths losing partial covers (P1)**: metadata and fallible allocation checks outside the arithmetic step previously returned a bare error and discarded accumulated coverage. They now retain the error in the report and preserve current/pending/prior-unresolved boxes. Recovery reuses the admitted pending buffer, so later allocation failures do not need another allocation to preserve the boxes. Initial admission/main-workspace failures retain the input domain. Only failure to allocate the minimal one-box recovery slot itself returns `Err`.

## Evidence

- `numeric-review-red.log`: both scalar split admission and rational-operand tests failed before fixes.
- `numeric-vector-red.log`: both simultaneous-matrix and returned-MP-storage tests failed before vector fixes.
- `numeric-review-green.log`: **35 passed**, comprising arithmetic 17, roots 15, MPFR oracle 3.
- `numeric-review-clippy.log`: strict Clippy passed for numerics library and changed arithmetic/root test binaries.
- Commands used `devenv shell`, `CARGO_TARGET_DIR=target/mpfr-oracle`, `CARGO_BUILD_JOBS=2`. No workspace-wide gate or commit.

Separate minor ownership fix: removed the redundant `one.clone()` from QR solution diagnostics, as reported by the integrated polynomial lint gate.

Additional evidence after findings 4–5:
- `numeric-admission-red.log`: invalid identity AD seed test failed before the validation hook.
- `numeric-failure-red.log`: metadata and initial backend-budget failure tests failed before recovery changes.
- `numeric-admission-green.log`: **38 passed**, arithmetic 18, roots 17, MPFR 3.
- `numeric-admission-clippy.log`: strict library and changed-test Clippy passed after the validation and recovery changes.
- `numeric-admission-preservation.log`: additional exact wide-seed/composite-validation/metering regression.

QR test lint cleanup uses fixed-size array chunks and a focused assertion-in-Result allowance for bounded analytic regression fixtures. No production lint suppression was added.

## Final bulk work admission review

Source-only independent review of `Backend::charge`, `BudgetedBackend`, First/Jet/Gradient forwarding, and the f64 QR call found no actionable issue. The default backend method repeats `visit` and stops at its first error, retaining custom backend side effects and MP operation-limit behavior. Audited F64/Interval64 backends can use constant-time no-op forwarding because their visit is unmetered; nested BudgetedBackend wrappers each perform checked aggregate admission before forwarding. Every wrapper remains bounded, and overflow does not mutate its own counter. A previously admitted outer reservation is intentionally retained when an inner backend subsequently refuses, matching the documented conservative modeled-work contract. AD wrappers forward directly without adding derivative-order multipliers to a caller-declared opaque work amount.

QR retains checked n-cubed work construction and local maximum admission, then charges the complete amount before entering faer. The change removes an unnecessary O(n^3) loop of accounting callbacks without changing the mathematical factorization or loosening its operation allowance. The added regression source covers nested limits, conservative reservation, usize overflow, default MP partial failure, and zero charge. Root owns execution of final gates; this review ran no builds or timing and makes no fresh test-result claim.
