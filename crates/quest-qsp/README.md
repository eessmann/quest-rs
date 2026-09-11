# quest-qsp

Canonical and generalized quantum signal processing (QSP) in pure Rust. The
production pipeline uses binary64 FFT Weiss completion and a divide-and-conquer
inverse nonlinear Fourier transform (NLFT). It produces immutable phases or
two-dimensional controls without requiring an installed native `QuEST` library.

## Canonical quickstart

Supply a real Chebyshev polynomial with exactly one parity. This example
synthesizes the odd target `p(x) = 0.6*x`:

```rust
use quest_polynomial::{Chebyshev, Limits, Polynomial};
use quest_qsp::{Complex64, SynthesisBuilder};

let target = Polynomial::new(
    Chebyshev,
    vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
    Limits::default(),
)?;
let admitted = SynthesisBuilder::new().canonical(&target)?.admit()?;
let completed = admitted.complete()?;
let candidate = completed.synthesize()?;

assert!((candidate.response(0.3)? - 0.18).abs() < 1e-11);
let phases = candidate.phase_sequence();
assert_eq!(phases.convention(), "pyqsp-wx-symmetric");
assert_eq!(phases.degree(), 1);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The canonical convention is
`U(x) = exp(i*phi_0*Z) Wx(x) ... Wx(x) exp(i*phi_d*Z)`, where
`Wx(x) = [[x, i*sqrt(1-x*x)], [i*sqrt(1-x*x), x]]` for `x` in `[-1, 1]`.
The target is **the imaginary part of `U00`**. `response(x)` evaluates this
sequence, while `phases()` and `phase_sequence()` expose the frozen angles.

Canonical admission rejects mixed parity and imaginary coefficients, including
small nonzero values. It does not project parity, chop coefficients, rescale the
target, or silently discard subnormal coefficients during basis conversion.

## Generalized quickstart

The generalized builder accepts complex Laurent coefficients with nonnegative
support. A positive first exponent is padded explicitly with zero coefficients;
negative exponents require a separate mathematical transformation by the caller.

```rust
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, SynthesisBuilder};

// b(z) = (0.1 + 0.2i) + (-0.3 + 0.1i)*z.
let target = Polynomial::new(
    Laurent::new(0),
    vec![Complex64::new(0.1, 0.2), Complex64::new(-0.3, 0.1)],
    Limits::default(),
)?;
let candidate = SynthesisBuilder::new()
    .generalized(&target)?
    .admit()?
    .complete()?
    .synthesize()?;

let [[response, _], _] = candidate.evaluate(Complex64::new(1.0, 0.0))?;
assert!((response - Complex64::new(-0.2, 0.3)).norm() < 1e-11);
assert_eq!(candidate.control_sequence().degree(), 1);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Here the exported product is `C0 D(z) C1 ... D(z) Cd`, with
`D(z) = diag(z, 1)` and target `b(z)` in its upper-left entry. The last control
already contains the right factor `K = [[0, -1], [1, 0]]`; do not append `K`
again. Unitarity concerns `|z| = 1`; `evaluate(z)` also accepts other finite
complex values for polynomial evaluation. For a canonical candidate,
`evaluate(z)` uses this transformed Laurent representation; use `response(x)`
for the real Wx response.

## Consuming stages and numerical meaning

| Stage | API | Result and contract |
| --- | --- | --- |
| Configure | `SynthesisBuilder::new().canonical(...)` or `.generalized(...)` | Owns copied target data in a mode-specific ready state. |
| Admit | `.admit()` | `AdmittedTarget` with an outward unit-circle contractivity bound and positive margin. |
| Complete | `.complete()` | `CompletedPolynomial` with a complement, completion grid and binary64 residual. |
| Freeze | `.synthesize()` | `FrozenCandidate<Canonical>` or `FrozenCandidate<Generalized>` with immutable binary64 exports. |
| Certify, optionally | `CertificationBuilder::new().candidate(...).policy(...)?.certify()` | `Certified` with independently established bounds on the frozen export. |

Each transition consumes its input. Methods needed by later stages are absent
until earlier states have been obtained. In particular, a frozen candidate is
not an independent certificate. Completion and reconstruction residuals from
production synthesis are numerical diagnostics.

Set `Policy` with `SynthesisBuilder::policy` to configure response tolerance,
strict contractivity margin, FFT backend, maximum grid and numerical `Limits`.
Defaults use tolerance `1e-11`, margin `1e-12` and `FftBackend::Scalar`.
Contractivity is established using outward coefficient bounds or bounded
unit-circle subdivision, and must be strictly below `1 - contractivity_margin`.
An exactly unit-modulus target does not meet this requirement.

Target selection checks storage using the current policy. Setting a tighter
policy afterward rechecks retained storage at admission; set a larger policy
before target selection if the input exceeds the default allocation limits.
Work accounting covers each consuming stage, including completion retries and
inverse/reconstruction work. Memory estimates include retained data and
concurrent scratch. Opaque FFT planner storage is modeled; the byte limit is
not a process-wide allocator quota.

## Feature and precision boundaries

No optional features are enabled by default.

| Feature | Effect |
| --- | --- |
| `certification` | Adds independent directed `astro-float` verification of a frozen binary64 export. |
| `offline-synthesis` | Enables `certification` and explicit arbitrary-precision synthesis and approximation builders. |
| `rayon` | Enables caller-owned pools for `complete_with` and `synthesize_with`. |
| `simd` | Compiles SIMD FFT backends; select `Policy::backend = FftBackend::Simd` explicitly. |

Production synthesis and evaluation stay in binary64 with every feature
combination. No production error invokes an offline solver or changes precision
or FFT backend. SIMD selection fails if a compiled supported backend is
unavailable. Sequential execution is the default; `rayon` stages borrow the
caller's pool and the returned candidate owns its payload without retaining that
borrow. Parallel branches require separate admitted scratch and may exhaust a
budget that fits sequential execution.

With `certification`, freeze once and verify those same values:

```rust
# #[cfg(feature = "certification")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use quest_polynomial::{Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, SynthesisBuilder};
use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};

let target = Polynomial::new(
    Laurent::new(0), vec![Complex64::new(0.3, 0.4)], Limits::default(),
)?;
let frozen = SynthesisBuilder::new()
    .generalized(&target)?.admit()?.complete()?.synthesize()?;
let certified = CertificationBuilder::new()
    .candidate(frozen)
    .policy(CertificationPolicy::default())?
    .certify()?;
assert!(certified.report().response().upper_f64() <= 1e-11);
let _unchanged_candidate = certified.candidate();
# Ok(())
# }
# #[cfg(not(feature = "certification"))]
# fn main() {}
```

The verifier bounds source conversion, completion, response, all four
reconstructed matrix entries and unitarity. Generalized matrices are imported
as exact binary64 dyadics. Canonical verification reconstructs rotations from
the exact exported phase values using directed arbitrary-precision trigonometry.
Reports retain `astro_float::BigFloat` endpoints; binary64 upper summaries round
upward. Default verification starts at 256 bits and can retry up to 1024 bits.
More verifier precision can tighten an enclosure but cannot repair export error.

With `offline-synthesis`, use
`OfflineBuilder::new().canonical(&original)?` or `.generalized(&original)?`,
then `.policy(OfflinePolicy::default())?.solve()?`. These builders start from
original binary64 coefficients, compute separate arbitrary-precision candidates,
export binary64 controls or phases, and independently certify each export.
Retries rebuild numerical state from the original source. Defaults begin at
128 bits and allow up to 4096 bits; precision limits must align to backend words.
Reports separate computation and certification time. The offline candidate's
production `reconstruction_residual()` is unavailable (`+infinity`); use
`solved.certified().report().reconstruction()` for the independent bound.

`OfflineRemezBuilder` provides a separate function-approximation path over a
positive-width subinterval of `[-1, 1]`. Its `error_tolerance` bounds the exported
polynomial's **total uniform error**. The reported numerical exchange gap is
empirical and is not a minimax certificate. Admit parity and contractivity
explicitly before using such a polynomial for QSP.

## Imported sequences and errors

Use `PhaseSequence::<WxSymmetric>::builder(values).build()` for tagged symmetric
Wx phases, `PhaseSequence::<WxLaurent>` for the Laurent tag, or
`PhaseSequence::<CanonicalWxImag>` for canonical imaginary-`U00` phases. Imports
validate nonempty finite values and, for the symmetric tag, compare mirrored
rotations with a `1e-12` tolerance. Convention conversion is explicit through
`canonical()` and `projector_phases_with_diagnostics()`. Its roundoff estimate
is a numerical diagnostic, not an outward bound or a transferable certificate.

`ControlSequence::builder().matrices(values).build()` checks actual generalized
matrices at a fixed `1e-10` unitarity tolerance. The caller supplies the complete
product convention including terminal `K`. The `angles(psi, phi)` alternative
constructs paper-native rotations and incorporates `K` for the caller. Matrix
admission neither normalizes imports nor grants exact inverse semantics.

`Error` distinguishes unsupported targets, invalid policies, resource budgets,
nonfinite arithmetic, insufficient contractivity, unestablished residuals and
singular inverse-NLFT pivots. Handle it as a non-exhaustive enum. Certification
has a separate `CertificationError`: `NotEstablished` retains an insufficient
bound, while `Violation` retains a proved lower witness against a tolerance.
Offline exhaustion and certification failures carry their own reports. A failed
sufficient bound alone does not prove the requested approximation impossible.

Run the documentation and executable tutorial from the workspace root:

```sh
cargo test -p quest-qsp --doc --locked --offline --all-features
cargo run -p quest-qsp --example qsp_tutorials --locked --offline --all-features
```

The workspace mdBook chapters on [QSP synthesis](https://github.com/eessmann/quest-rs/blob/main/docs/book/src/qsp-synthesis.md)
and [certification](https://github.com/eessmann/quest-rs/blob/main/docs/book/src/qsp-certification.md) explain the shared
polynomial prelude, caller-owned workers and stage observation. Use
[`quest-qsvt`](https://github.com/eessmann/quest-rs/blob/main/crates/quest-qsvt/README.md) to build transformations from these sequences.

This Rust port draws on `quest-qsvt` revision `7fe7f740579b03c52a8cf48be6a31268b029c19f`. Its MIT notice is retained in `LICENSE-quest-qsvt`.
