# Exact Dashu migration

Scope: `quest-language`, `quest-symbolic`, `quest-math`, and `quest-synthesis`. Existing uncommitted work was retained. No commits, C++ changes, Zotero changes, or unrelated crate mutations were made by this agent.

## Changes

- Replaced project-owned `num_bigint::BigInt` and `num_rational::Ratio` values with native `dashu_int::IBig` and `dashu_ratio::RBig`, including tests, examples, and manifests. Native `RBig` is reexported from `quest_language::rational`, `quest_language::quantum`, `quest_symbolic`, and `quest_math`; no compatibility aliases or numerical wrappers were introduced.
- Native `RBig` stores its canonical denominator as `UBig`. Raw `AngleTarget` and `BoundAngleTarget` numerator/denominator payload fields remain `IBig` so checked local admission continues to accept and normalize signed denominators. Constructors use `RBig::from_parts_signed`; native rational values are already reduced.
- Preserved checked signed floor/ceil interval division, exact ring parity and coefficient reduction, norm-equation remainder logic, canonical affine caching and source replay, exact angle budgets, and dyadic/pi cancellation before final machine export. Bit accounting uses `BitTest::bit_len` and checked `usize`/`u64` conversions at existing public resource-ledger boundaries.
- Kept the exact ties-to-even binary64 converter, including subnormal rounding, signed underflow, finite cancellation of overflowing affine terms, and precision-capped mathematical-pi refinement. Conversion uses native exact integer division and remainder, never `to_f64_fast`.
- Added `quest_math::encoding::{parse_integer, parse_rational, integer, integer_array, rational}`. Integers use canonical signed decimal strings. Rational serde uses reduced objects `{numerator: "...", denominator: "..."}` with positive denominators. `AngleTarget` has explicit canonical serialization/deserialization, normalizing local signed pairs on output and rejecting noncanonical or unreduced wire pairs on input. Cyclotomic coefficient arrays and approximation certificate bounds use these helpers.
- Kept the frontend template format at its pre-port version 2: its syntax/SSA representation contains no affected exact payload. The parent owns version-3 optimizer protocol, compiled structured artifacts, and compilation-evidence producers.
- `num-traits` remains only for primitive/f64 classical conversions in `quest-language`; exact arithmetic no longer uses its compatibility traits.

## Evidence

All Cargo commands used the project-local environment and `CARGO_BUILD_JOBS=2` under a parent-coordinated build slot.

1. Before implementation, the new `cyclotomic_payload_uses_decimal_coefficients` test failed as intended against the old native limb serializer. Log: `/tmp/dashu-exact-baseline.log`.
2. Before manual target serde, `angle_interchange_reduces_signed_pairs_to_decimal_strings` failed on raw `-6/-8` output, and `angle_interchange_rejects_noncanonical_or_unreduced_pairs` failed because `2/0` decoded. Log: `/tmp/dashu-exact-wire-red.log`.
3. `cargo check -p quest-language -p quest-symbolic -p quest-math -p quest-synthesis --all-features --all-targets`: exit 0. Log: `/tmp/dashu-exact-check.log`.
4. `cargo test -p quest-language -p quest-symbolic -p quest-math -p quest-synthesis --all-features`: exit 0; 203 passed, 0 failed, 0 ignored over 31 unit/integration/doc-test invocations. Log: `/tmp/dashu-exact-tests.log`.
5. `cargo clippy -p quest-language -p quest-symbolic -p quest-math -p quest-synthesis --all-features --all-targets`: exit 0, no warnings or errors in the final invocation. Log: `/tmp/dashu-exact-clippy.log`.
6. `cargo fmt` was limited to the four assigned packages. No legacy arbitrary-precision types, aliases, `num_bigint`, `num_rational`, or `to_f64_fast` remain in their code.

The green suite includes ties-to-even/subnormal/pi-refinement tests, overflowing-term cancellation, source obligations, affine budgets, signed floor/ceil endpoint fixtures, canonical decimal wire tests, exact ring and synthesis identities, prime norm/unit associates, lattice completeness on small grids, twelve-digit synthesis, and independent certificates.

## Handoff and limits

The parent must validate consumer integration and all requested workspace configurations. After the 203-test run, Clippy required two semantically equivalent source changes: explicit `Mul::mul` for the interval period and `clone_from` for the radial projection. The parent's final suite should cover that final tree. Linux/MPI/accelerator evidence and performance benchmarking are not established by these macOS gates.

One relevant native API distinction: `IBig % i32` returns `i32`, so primitive parity remainders are compared directly with zero; signed division between arbitrary integers still uses an `IBig` remainder and explicit negative floor correction. Another distinction is that Dashu's `Signed::is_positive` includes zero; the synthesis radial-excess test explicitly compares with `RBig::ZERO` to preserve strict positivity.

## Final review cleanup

The parent reported a subsequent full-workspace gate of 1005 passing tests plus passing doctests, followed by an independent read-only mathematical review. Its strict `-D warnings` gate identified the synthesis entry point exceeding the 100-line function limit by two lines. The construction of the exact Hadamard enclosure is now a separate `hadamard_enclosure` helper, retaining the same matrix, precision, limits, error conversion, and call order without a blanket lint allowance.

An audit removed impossible `RBig::denominator().is_zero()` branches from language angle admission and rational/pi/binary64 export, symbolic coefficient admission, source binding, and imported rational admission. Native `RBig` guarantees a reduced positive denominator. All raw signed-input denominator checks, wire-pair checks before construction, interval positive-divisor checks, and norm-equation divisor checks remain. The former private symbolic `normalize` helpers were renamed `admit_rational` and `admit_coefficient` to describe their remaining coefficient-budget/work admission role.

These final refactorings did not run tests or builds in this agent because the parent reserved the build slot and will rerun the affected gates. The parent must use those fresh results for final-tree validation.
