# Dashu arithmetic migration

Project-owned arbitrary-precision arithmetic now uses one pure-Rust family:
`dashu-base` 0.6.1, `dashu-int` 0.6.2, `dashu-ratio` 0.6.1 and
`dashu-float` 0.6.2. The Cargo lock records the resolved published sources.
There is no production or test Astro-float, Rug, GMP/MPFR or Malachite backend.

## Rust API changes

Exact public values use native `IBig`, `UBig` and canonical `RBig`. The former
`BigRational`/`Rational` aliases are removed. Use `RBig::from(integer)`,
`RBig::from_parts(numerator, unsigned_denominator)` or
`RBig::from_parts_signed(numerator, signed_denominator)` after admitting a
nonzero denominator. Accessors are `numerator()` and `denominator()`; the latter
returns `&UBig`. Canonical rationals cannot represent a zero denominator.
Raw target pairs retain signed integers for checked input normalization.

Multiprecision point values and interval endpoints use native
`FBig<HalfEven, 2>` (`Binary` names this fixed mathematical representation).
`MpBackend`, `MpIntervalBackend`, static expressions, AD and generic Remez keep
their existing roles. Precision is bit-granular. Cancellation can retain one
guard digit; applying a second nearest rounding would change the arithmetic
contract. Stored values are admitted using their actual significand and value
exponent, and search storage uses actual word counts.

Use checked `exact_from_f64` and `to_f64` at binary64 boundaries. The exporter
rounds the exact dyadic itself, including subnormals and overflow-midpoint ties.
Decimal-source constants are parsed as exact integer ratios before directed
binary division. Neither boundary passes through a preliminary machine float.
The exact-angle subsystem retains its separate rational/pi conversion semantics.

## Interchange

Optimizer messages, compiled artifacts and compilation-evidence records use
version 3. Exact integers are canonical decimal strings; rational pairs are
reduced and have positive denominators. Producers normalize local signed target
pairs; decoders reject noncanonical pairs. Rebuild older artifacts; no legacy
decoder is provided. Frontend templates remain version 2 and QSP formats retain
their versions because their representations did not change. QSP artifact
admission now agrees with bit-granular compute policies, including 65 bits.

## Retained boundaries and removals

| Layer | Decision and concrete purpose |
| --- | --- |
| Old arithmetic adapters and direct `num-bigint`/`num-rational` edges | Removed; native Dashu types supply all project-owned arbitrary precision. |
| Exact dyadic binary64 conversion | Retained as a checked mathematical boundary, with midpoint, signed-zero and subnormal regressions. |
| Independent QSP interval/product algorithms | Retained to verify exported responses independently of candidate generation; they share the selected primitive arithmetic family. |
| Exact rational oracle | Retained for independent inclusion arguments; it shares Dashu integers, so it is not an independent arithmetic library. Frozen libmpdec fixtures add external arithmetic witnesses. |
| Attempt-owned constant caches | Retained for transcendental reuse; retained words are accounted, opaque internal work is not a hard allocator budget. |
| Paired QSP trigonometric evaluation | Uses Dashu's correctly rounded `sin_cos` to share argument reduction; directed endpoints and independent verification retain their contracts. |
| Native Cargo input watches | Retain source, external package and environment invalidation; omit the build script's own generated output tree to prevent unnecessary rebuilds. Generated sources remain CMake inputs. |
| Native `Binary` name and native `RBig` reexports | Identify the fixed representation and public exact types without wrapping values or preserving old APIs. |
| Optional QuiZX/OpenQASM private `num` dependencies | Retained upstream pure-Rust implementation details behind checked scientific interchange; no forks or project-owned arbitrary-precision adapters. |
| Runtime precision, resource limits and convergence | Retained because they depend on numerical inputs and bounded attempts. |
| Historical measurements and archived source | Retained with original provenance outside active dependency selection; abandoned Rug migration instructions removed. |

Inverse NLFT remains the default; RHW remains explicitly selectable. Failures do
not select another algorithm or relax tolerances. The C++ reference and Zotero
library were not changed. Validation and performance evidence are recorded in
[the Dashu evidence directory](data/2026-10-02-dashu-consolidation/).
