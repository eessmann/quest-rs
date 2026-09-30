# Independent certification and explicit offline work

The examples use the [shared numerical prelude](numerical-polynomials.md).

Astro Float operations are cold features. Enable `quest-qsp/certification` to verify an immutable binary64 export; enable `quest-qsp/offline-synthesis` to request a separate arbitrary-precision computation. Neither feature changes the ordinary binary64 synthesis algorithm or creates an automatic fallback.

## Certify exported bytes

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:independent_certification}}
```

The default verifier starts at 256 bits, uses an outward interval FFT product tree and retries within the configured precision/resource budget. A direct-convolution verifier remains available through `ConvolutionMethod::Direct` as an independent reference path. Default response, completion, conversion, reconstruction and unitarity tolerances are `1e-11`.

The backend is pinned to `astro-float 0.9.6`, with its `std` feature and no GMP dependency. The adapter decodes binary64 bits itself because the upstream `from_f64` routine mishandles subnormals. It also implements directed dyadic-to-binary64 conversion, including underflow, overflow and halfway cases. FFT twiddles use exact integer octant reduction and directed pi/sine/cosine operations; the backend expression macro is not used for enclosure arithmetic. Numerical acceptance relies on these primitive directed-rounding contracts, with independent integer fixtures and direct-convolution checks.

Generalized matrix entries are admitted as exact binary64 dyadics. Real-parity Wx verification reconstructs trigonometric rotations from the exact exported binary64 phases with directed Astro Float sine/cosine, rather than treating diagnostic matrix entries as the phase authority. It reconstructs all four matrix-polynomial entries, independently checks source conversion and completion, and bounds the actual unitary defect.

The verifier accepts using Astro Float bounds before converting summaries to binary64. Upper summaries round upward. `Certified` owns the original frozen candidate and the detailed immutable report. Higher verification precision never changes candidate controls or phases.

`CertificationError::NotEstablished` means the computed sufficient bound did not establish the requested tolerance. `Violation` requires a demonstrated lower witness. These outcomes have different meanings: a loose enclosure is not proof that a candidate is bad, while raising verifier precision cannot repair an actual error already present in exported bytes. Reports retain the original Astro Float bounds.

## Actual QSVT projector phases

A Wx phase certificate does not automatically cover rounded projector shifts.
`Certified<RealParityWx>::certify_projector_phases(policy)` reconstructs and
certifies the actual converted reflection-signal product and readout rotation
on `[-1,1]`. `TransformBuilder::certified_standard` consumes those immutable
values and retains their exact evidence; imported phases have weaker status.

`StandardPremises::certified_actual_phase_response` uses only the evidence
already attached to that transform and includes its response bound. Exact
projected-unitary and completion hypotheses remain explicit assumptions.
[Infinite-QSP theory](https://arxiv.org/abs/2310.12683v2) does not establish a
blanket binary64 implementation guarantee.

## Explicit offline synthesis

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:offline_synthesis}}
```

The offline builder starts from original polynomial coefficients at 128 bits by default and doubles precision up to the configured maximum, initially 4096. Precision limits must be whole backend words, so reported precision is the precision actually used. Each attempt owns its Astro Float constant cache; nearest rounding computes candidates, while directed arithmetic establishes contractivity and the separate certificate. Binary64 imports use exact bit decoding, including subnormals, and exports use the shared checked dyadic converter. It implements explicit Astro Float RHW/Half-Cholesky and inverse-NLFT paths, with its own FFT, Weiss completion and scaled controls. A successful solve exports binary64 controls or phases once and certifies that export independently. A retry reconstructs all numerical state from the original source.

Reports separate Astro Float computation from certification time and retain every attempt. Detailed scalar values are `astro_float::BigFloat`; failed approximations expose retained candidates through `arbitrary_coefficients()`. Backend errors and nonfinite arbitrary-precision results become typed failures. Final export failure preserves original input and the last target/complement/controls/phases together with certification evidence. The ordinary production reconstruction diagnostic is unavailable (`None`) on an offline candidate; read the independently established finite reconstruction bound from the certified report.

## Explicit offline approximation

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:offline_approximation}}
```

The Astro Float Remez entry point reads the same original function expression used by the polynomial crate. It computes numerical alternation candidates with pivoted Householder QR, freezes a binary64 Chebyshev polynomial, and independently proves its uniform error over the admitted real subinterval of [-1,1]. Here **`error_tolerance` bounds the total uniform approximation error**. This differs from the binary64 Remez builder's minimax-gap tolerance.

The offline exchange gap is empirical: numerically located extrema alone do not prove a minimax gap. The published uniform-error bound instead uses full-domain interval derivative and mean-value analysis of the final polynomial. A failed enclosure remains a typed `ApproximationNotEstablished` result with original-expression provenance. The solver does not silently project parity; choose and admit a suitable polynomial explicitly before real-parity Wx synthesis.
