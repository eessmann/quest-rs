# Independent certification and explicit offline work

The examples use the [shared numerical prelude](numerical-polynomials.md).

Dashu certification and offline operations are cold stages. Enable `quest-qsp/certification` to verify an immutable binary64 export; enable `quest-qsp/offline-synthesis` to request a separate arbitrary-precision computation. Neither feature changes the ordinary binary64 synthesis algorithm or creates an automatic fallback.

## Certify exported bytes

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:independent_certification}}
```

The default verifier starts at 256 bits, uses an outward interval FFT product tree and retries within the configured precision/resource budget. A direct-convolution verifier remains available through `ConvolutionMethod::Direct` as an independent reference path. Default response, completion, conversion, reconstruction and unitarity tolerances are `1e-11`.

The backend uses pinned pure-Rust Dashu components: `dashu-float`/`dashu-int` 0.6.2 and `dashu-base`/`dashu-ratio` 0.6.1. Native float imports preserve binary64 values exactly, including subnormals and signed zero; directed exports handle underflow, overflow and halfway cases. FFT twiddles use exact integer octant reduction and directed pi/sine/cosine operations. Numerical acceptance relies on these primitive directed-rounding contracts, with independent integer fixtures and direct-convolution checks. The verifier retains its own interval algorithms, separate from the approximation backend. Precision and directed rounding belong to local typed contexts; each attempt owns its constant cache.

Generalized matrix entries are admitted as exact binary64 dyadics. Real-parity Wx verification reconstructs trigonometric rotations from the exact exported binary64 phases with directed Dashu sine/cosine, rather than treating diagnostic matrix entries as the phase authority. It reconstructs all four matrix-polynomial entries, independently checks source conversion and completion, and bounds the actual unitary defect.

The verifier accepts using Dashu bounds before converting summaries to binary64. Upper summaries round upward. `Certified` owns the original frozen candidate and the detailed immutable report. Higher verification precision never changes candidate controls or phases.

`CertificationError::NotEstablished` means the computed sufficient bound did not establish the requested tolerance. `Violation` requires a demonstrated lower witness. These outcomes have different meanings: a loose enclosure is not proof that a candidate is bad, while raising verifier precision cannot repair an actual error already present in exported bytes. Reports retain the original Dashu bounds.

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

The offline builder starts from original polynomial coefficients at 128 bits by default and doubles precision up to the configured maximum, initially 4096. Precision is bit-granular between 64 and 1,048,576 bits. Nearest rounding computes candidates, while directed arithmetic establishes contractivity and the separate certificate. Exact dyadic imports and exports preserve the checked binary64 interchange contract. The builder implements explicit RHW/Half-Cholesky and inverse-NLFT paths, with its own FFT, Weiss completion and scaled controls. A successful solve exports binary64 controls or phases once and certifies that export independently. A retry reconstructs all numerical state from the original source.

Reports separate Dashu computation from certification time and retain every attempt. Detailed scalar values are native `FBig<HalfEven, 2>`. Domain, exponent, correct-rounding and application budget failures remain typed. Cancellation can retain one guard bit without a second rounding. Byte budgets account significand words, retained constant caches and application workspaces; they do not measure allocator capacity or opaque internal transcendental work. Final export failure preserves original input and the last target/complement/controls/phases together with certification evidence. The ordinary production reconstruction diagnostic is unavailable (`None`) on an offline candidate; read the independently established finite reconstruction bound from the certified report.

## Explicit offline approximation

```rust
{{#include ../../../crates/quest-qsp/examples/qsp_tutorials.rs:offline_approximation}}
```

The shared `RemezRequest` owns the original statically typed function. With `MpBackend`, `MpIntervalBackend` and `MpHouseholder`, it generates candidates and proves the explicitly selected uniform-error or minimax-gap contract using complete critical-point coverage. Arithmetic precision does not change the meaning of `Accuracy`. The example selects a uniform bound of `0.006` and freezes a binary64 polynomial before certification; extra proof precision cannot conceal export rounding error.

Failures retain the owning request, candidate, attempted precision and partial coverage. The solver does not silently project parity; choose and admit a suitable polynomial explicitly before real-parity Wx synthesis. See [the shared approximation model](numerical-polynomials.md) for minimax gap, exact-domain and export contracts.
