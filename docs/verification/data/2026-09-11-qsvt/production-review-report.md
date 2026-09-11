# Independent binary64 production review

2026-09-11. Reviewed `quest-qsp/src/{stages,kernel,admission,sequence}.rs`, `quest-numerics/src/{fft,policy,interval}.rs`, associated production/parallel tests, and QSVT convention conversion. The initial task was read-only; the parent then authorized the concrete phase-conversion correction below. No commits were created.

## Finding: extreme imported phases silently lost convention offsets — corrected

The public phase builder admits any finite phase. Previously `PhaseSequence<WxLaurent>::canonical` added binary64 pi/2 directly to its first phase, and `CanonicalWxImag::projector_phases` directly subtracted pi/2 or pi/4. At phase 1e20, all those operations return the original bits. The omitted quarter turn changes the rotation by operator distance sqrt(2). For example, the expected canonical constant response after the positive quarter turn is cos(1e20)=0.7639704044417283, whereas the unchanged phase gives sin(1e20)=-0.6452512852657808. Unrestricted imported phases reach `quest-qsvt` standard routes, so this was a silent correctness defect rather than merely a poor diagnostic. Production atan-derived phases are bounded and did not trigger this extreme case.

The correction composes the actual sine/cosine rotation with the required offset. Positive/negative pi/2 use exact swaps/signs; negative pi/4 uses the bounded sum/difference rotation. Direct scalar offset arithmetic is retained when its evaluated rotation agrees with the composed reference within 4*EPSILON, preserving ordinary phase bits. Otherwise a bounded atan2 result represents the composed rotation. No modulo operation against rounded TAU is used, and only shifted phases are changed. Unshifted Wx-symmetric sharing and unshifted Laurent payload bits are preserved.

`PhaseSequence` accumulates a numerical convention-conversion diagnostic, and `ConvertedProjectorPhases` exposes read-only values plus the combined diagnostic. Existing convenience conversion methods remain available. QSVT consumes the typed result and records its accumulated diagnostic in the existing `TransformEvidence::phase_conversion_roundoff_estimate` field. The former estimate proportional to the magnitude of the original angle could become infinity at f64::MAX; the new estimate stays finite for the tested extreme inputs. It consists of observed rotation residuals plus binary64 allowances, **not an outward bound or a transcendental certificate**. Conditional theorem analysis keeps it in NumericalObservations and does not consume it as certified error. Source-polynomial certification grants no privilege to the constructed shifted payload.

New regressions compare complete 2x2 QSP matrix products and complete complex 2x2 QSVT projected blocks, with phases +1e20, -1e20, +f64::MAX, and -f64::MAX. Degree-zero canonical/Laurent and degree-one symmetric routes cover scalar-phase retention and imaginary off-diagonal matrix entries. Moderate conversion fixtures retain the old phase bits. Diagnostics remain finite and bare transform theorem bounds remain absent.

## Other reviewed invariants

No further concrete correctness defect was found in the bounded review:

- The inverse recursion completes its prefix/first-half solve, computes the midpoint update, then solves the dependent second half. Only independent forward reconstruction children and pointwise/FFT pairs are parallelized.
- Generalized freezing explicitly right-multiplies the terminal control by K=[[0,-1],[1,0]], preserving scalar phase. Evaluation and residual reconstruction use the same control ordering with the signal factor between controls. Canonical freezing recomputes actual exported phase trigonometry after symmetry construction; its public response is imaginary U00 in the stated Wx convention.
- Circle contractivity admission uses outward interval FFT samples and a global derivative bound times pi/N for the nearest-sample gap. It fails explicitly when a positive requested margin cannot be established.
- Caller-owned Rayon paths keep each point's arithmetic order, separate concurrent FFT scratch, join before error propagation, and retain left-before-right/indexed failure precedence. Independent children receive partitioned work and planner/storage allowances; these may conservatively reject tight parallel budgets and are documented accordingly.
- Retained target/source/complement and recursive vector payload allowances are subtracted before admitting cached workspaces. Concurrent plan estimates are partitioned after accounting for parent cached plans. Completion retries accumulate FFT/convolution work; inverse work is subtracted before response reconstruction. RustFFT planner bytes remain an explicit estimate, not an allocator-enforced cap.
- Warm convolution clears stale padding, validates both input slices before transforms, checks nonfinite intermediate/output values, and retains exact linear support. The caller-pool warm-allocation regression passed.
- The reviewed production stage/kernel/admission modules contain no call to certification, precision, or offline synthesis. Failures return their original errors; the static precision-boundary and budget-failure tests passed.

This is a bounded code and regression review, not a formal proof of RustFFT, maryada, platform libm, or worst-case allocator behavior. It does not claim SIMD-platform testing beyond the selected scalar/caller-pool configuration.

## Validation

- Three new focused phase-conversion tests passed, including full complex QSVT blocks.
- `cargo nextest run -p quest-qsp -p quest-qsvt -p quest-numerics --features rayon --locked --offline`: **68/68 passed**, run `b2631292-fd0a-4d36-9ead-e065da2cd8c0`, 0.423 seconds. The separate opt-in degree-8105 parallel scale fixture was not rerun here; prior scale and 43-case Astro catalog evidence remain separately recorded.
- `cargo test -p quest-qsp -p quest-qsvt --doc --features rayon --locked --offline`: **10 compile-fail doctests passed**.
- Strict focused phase-conversion Clippy and final all-target QSP/QSVT Rayon Clippy both passed. Owned-file formatting and diff checks passed.

Changed production files are `quest-qsp/src/{sequence,lib}.rs` and `quest-qsvt/src/{routes,transform}.rs`; new tests are `quest-qsp/tests/phase_conversion.rs` and `quest-qsvt/tests/phase_conversion.rs`. Original synthesis kernels and FFT algorithms were not changed by this correction.
