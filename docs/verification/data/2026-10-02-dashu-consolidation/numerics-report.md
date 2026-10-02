# Numerics and polynomial native Dashu migration

Status: native implementation and owned-crate validation are complete. Workspace-wide validation and independent review are coordinated by the root agent.

## Ownership and preservation

This task owns `crates/quest-numerics` and polynomial integration only. The pre-existing uncommitted static arithmetic, AD, root-cover, owning Remez and certificate architecture is preserved. The root agent owns workspace dependency pins and lockfile. No commits or other checkout edits were made. The C++ checkout and Zotero were not touched.

## Native arithmetic

`arithmetic::Binary` is a public alias for native `dashu_float::FBig<HalfEven, 2>`; there is no compatibility scalar wrapper. `MpBackend` owns statically typed nearest/down/up contexts and one `ConstCache`. Backend operations use fallible Context APIs, mapping domain, infinite input, exponent overflow/underflow and certification retry failures into `ArithmeticError`. Directed outputs change only their type-level rounding mode through `with_rounding::<HalfEven>()`, preserving the exact represented value and permitted add/sub guard digit.

Precision remains bit-granular (including 65 bits). Admission uses binary exponent plus actual significand bit length, with zero handled before inspecting its special exponent. Unreduced exact ratio operands are admitted before cancellation. Decimal import parses an exact integer mantissa and power-of-ten denominator/numerator, then performs one directed binary division. There is no decimal float intermediate.

Storage accounting uses `IBig::as_sign_words()` without cloning the significand. Up to two words live in the native header; wider significands account all their native words as heap storage. The working-output estimate includes one extra guard bit. This is a model of live scalar storage, not allocator retained capacity, temporary allocation or opaque transcendental iteration costs.

Interval hull, intersection and singleton/endpoint transfer preserve exact native values across precision changes. Products and quotients evaluate each endpoint pairing in both directed contexts. Transcendental extrema detection retains the static integer-bound argument reduction, widening uncertain large phase reductions conservatively. Generic AD, root contractors, polynomial recurrence, exchange and certificates continue to consume the associated native scalar type directly; polynomial production changes were unnecessary.

## Exact interchange

Public signatures remain:

```rust
exact_from_f64(value: f64, precision: u32) -> ArithmeticResult<Binary>
to_f64(value: &Binary, direction: BinaryRounding) -> ArithmeticResult<f64>
```

`BinaryRounding` retains Down/Nearest/Up. Import decodes the binary64 sign, exponent and integer significand directly, preserving negative zero and all subnormals. Export rounds the exact integer dyadic onto the normal/subnormal binary64 grid using quotient, remainder and tie parity. It handles both signs, gradual underflow, subnormal ties, binade carries and finite overflow midpoint rounding. It does not call Dashu's native binary64 exporter, which the registry-source audit identified as potentially overflowing prematurely for values just above maximum finite.

## Independent verification

`tests/exact_oracle.rs` now reconstructs native endpoints into canonical Dashu `RBig`, with zero returned before any exponent shift. Its independently derived rational Taylor/atanh/arctangent series and remainder inequalities are unchanged; only exact integer/rational storage changed. It shares no production rounded or transcendental algorithm.

The root agent supplied frozen CPython `decimal`/libmpdec reference bounds in `tests/data/decimal-reference.json`. The consumer independently parses each decimal string into exact `RBig` and checks native interval results at 128 and 256 bits. The fixture records generator and runtime provenance, and the root agent owns the reproducible generator.

## Baseline and test-driven evidence

Commands use the project-local `devenv shell` and `CARGO_BUILD_JOBS=2`; build slots are serialized with the root agent.

- Pre-port `cargo test -p quest-numerics --test arithmetic --test exact_oracle`: existing 25 arithmetic tests all passed, including four previously unverified regressions. A newly added actual-significand storage regression failed as intended: Rug modeled 1.0 at 1024 bits as 160 bytes versus 40 bytes at 64 bits. The oracle was not reached because Cargo stopped on that deliberate red test.
- Pre-port `cargo test -p quest-numerics --test exact_oracle`: 2 passed after immediately making old Rug zero extraction safe. Runtime 15.16 s is informal debug timing, not a benchmark.
- First native focused compile exposed missing `ErrorBounds` generic constraints and the crate-root `ConstCache` export; both were repaired.
- Native focused `cargo test -p quest-numerics --test arithmetic --test exact_oracle`: 26 arithmetic tests and 2 exact-oracle tests passed. The oracle took 0.45 s in this informal debug run.
- First full `cargo test -p quest-numerics -p quest-polynomial` stopped at `vector_returned_mp_limb_storage_is_admitted`, whose old fixture merely requested 65536-bit precision on 1.0. The fixture now stores a real 65536-bit significand, `1 + 2^-65535`; the scalar pre-callback budget fixture similarly stores `1 + 2^-16383`. These preserve the intended byte-admission behavior under actual native storage accounting.
- Focused `cargo test -p quest-numerics --test roots_generic -p quest-polynomial`: all 17 root tests passed. The test filter means this command did not execute polynomial tests.
- Full `cargo test -p quest-polynomial`: all 68 integration tests and 7 compile-fail doctests passed, including multiprecision recurrences, scalar admission, both linear kernels and static Remez certificate regressions.
- Targeted project-nightly rustfmt completed on only the owned edited Rust sources/tests.

Additional native tests passed: the MAX + 2^969 overflow-boundary regression with signed directed expectations; a 129-bit retained guard at selected 128-bit precision including endpoint transfer and storage admission; 16384 deterministic finite binary64 bit-pattern roundtrips in all three directions; and exponent admission that distinguishes raw native scale from value magnitude. The independent oracle also validates 2048 signed exact dyadics spanning subnormal and normal grids against rational distances to their adjacent binary64 neighbors and tie parity.

The first guard fixture incorrectly expected `1 - 2^-129` to retain 129 significant bits at selected precision 128; native Context rounds that value to one. It was replaced by the genuinely dense 128-bit operand `(2^128 - 1) * 2^-128` minus `2^-129`, whose exact native result `(2^129 - 3) * 2^-129` demonstrably carries the permitted 129th digit. This was a fixture correction, not a production algorithm change.

Final `CARGO_BUILD_JOBS=2 devenv shell cargo test -p quest-numerics --all-features` passed **72 integration tests and 4 doctests**, after the last production lint repairs. Breakdown: arithmetic 30, independent/external oracle 4, kernels 13, observer 6, allocation 1, roots 17, parallel 1; doctests 1 executable and 3 compile-fail. All frozen libmpdec fixtures passed at both requested precisions.

Final `CARGO_BUILD_JOBS=2 devenv shell cargo clippy -p quest-numerics -p quest-polynomial --all-targets --all-features` passed. Lint repairs made the error mapper const, pass tiny Copy contexts by value, use checked epsilon negation, remove redundant constant clones and unnecessary precision conversions, and document narrow exact-arithmetic exceptions for admitted native negation/multiplication and bounded binary64 grid bookkeeping. The only remaining messages are existing nightly `generic_const_exprs` / next-generation solver compatibility warnings in polynomial sources/tests. Targeted rustfmt ran after repairs.

## Remaining project-level work

No owned-crate implementation or validation remains. Workspace-wide builds, matched-accuracy benchmarks and independent review are owned by the root agent. Linux/MPI/accelerator evidence is unavailable locally and remains pending. Informal debug timings above must not be treated as matched-accuracy benchmark results.
