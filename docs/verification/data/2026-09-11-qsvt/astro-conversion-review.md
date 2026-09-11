# Astro binary64 interchange review

Independent source review and integer-fixture integration tests found one defect:
`exact_from_f64` admitted precision 53, while Astro's `from_u64` requires at least
64 bits regardless of the integer's significant width. Reproducer:
`f64::from_bits(0x4a265315ba7fb19d)`, precision 53, returned
`PrecisionError::Backend(InvalidArgument)`. Root corrected allocation to
`precision.max(64)`; the regression now passes. No other conversion defect found.

The new `crates/quest-qsp/tests/astro_conversion.rs` is gated on `certification`.
It uses independently packed little-endian integer dyadics, without decimal
conversion or either interchange function to construct reference values.
Coverage includes 2,048 seeded finite-bit candidates at eight requested
precisions, random and explicit signed midpoint neighbors, normal/subnormal and
overflow boundaries, ties to even, distant sticky bits across multiple words,
storage padding, both backend exponent extremes, signed zero, typed error
distinctions, and explicit treatment of stored-value rounding provenance.

Validation after the fix: all six integration tests pass (0.10 s); focused
Clippy with `-D warnings` and rustfmt check pass. Commands used
`CARGO_BUILD_JOBS=2`, `--offline -p quest-qsp --no-default-features --features
certification --test astro_conversion` with `cargo test` and `cargo clippy`.

Only the installed x86_64 target was executed. Source review confirms scaling
uses the full mantissa allocation in native Word units; fixtures also pack
generically for 32-bit Word, but no 32-bit execution is claimed. No production
code or manifest was edited by the reviewer.

## Certification source follow-up

The bounded read-only review found no numerical enclosure defect in the migrated
twiddle and interval paths. Exact integer reduction preserves axes for N=1/2/4;
octant reflection reduces the angle to an interval checked inside [0,1]. Directed
pi, increasing sine, decreasing cosine, reflected swaps and quadrant signs give
the required outward bounds. FFT conjugation and inverse normalization preserve
the chosen sign convention. Certification compares multiprecision bounds with
the exact imported tolerance, rather than accepting rounded binary64 summaries.

One unproven robustness caveat was sent to root: infallible interval integer/zero
constructors do not check an Astro allocation-error NaN immediately, and a later
zero multiplication shortcut could hide it. Public policy admission rules out
invalid working precision; no real allocation-failure reproducer was produced.
This is not reported as a demonstrated numerical acceptance defect. Backend
adaptive scratch is explicitly excluded from the modeled resource cap in the
current policy documentation. No certification source was edited or additional
catalog validation claimed by this reviewer.
