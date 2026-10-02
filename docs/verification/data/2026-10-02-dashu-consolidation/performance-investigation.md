# Source-grounded performance investigation

This read-only investigation examines the pinned Dashu implementation and the pre-optimization benchmark receipts. No benchmarks, builds, profiling, or production/fixture edits were performed by this reviewer. CSV medians were read and recomputed; source inspection establishes executed algorithms, not measured attribution of runtime to individual functions.

## Observations from existing receipts

The primitive results in `.superpowers/sdd/2026-10-02-dashu-consolidation/primitive-results/summary.csv` compare current Dashu measurements with historical Astro/Rug measurements. Dashu multiplication takes approximately 2.0–2.5 times historical Astro time, matrix multiplication 1.85–2.35 times, and modified Gram-Schmidt 1.42–1.78 times. Exponential endpoint pairs are approximately 1.59–1.99 times faster than historical Astro. These are historical nominal-precision comparisons, with the limitations recorded in the benchmark review; they are not interleaved current measurements or matched matrix residual proofs.

The complete solver CSVs in `.superpowers/sdd/2026-10-02-dashu-consolidation/mp-solvers-results/receipts/` provide separate interleaved baseline/current evidence. Ratios below use the medians of the three recorded trials:

| Workload | 128 bits | 256 bits |
|---|---:|---:|
| Remez | Dashu 1.408x faster | Dashu 1.188x faster |
| Split Newton | Dashu 12.996x faster | Dashu 13.524x faster |
| Exact synthesis | Dashu 1.189x faster | Dashu 1.192x faster |
| Offline QSP | Dashu takes 1.122x the time | Dashu takes 1.520x the time |

Offline QSP records fewer Rust allocations at 128 bits and more at 256 bits; its peak additional live Rust bytes decrease at both precisions. These solver timings include allocation-counter atomics and correctness checks. Exact synthesis executes the same integer workload at both configured precision limits. Primitive ratios alone cannot explain or predict these complete-solver results.

**These receipts precede the proposed combined sine/cosine and pi-hoisting optimizations.** Preserve them under that provenance. Subsequent measurements need distinct frozen source hashes and result locations; changes under preparation are not evidence of a demonstrated improvement.

## Source facts and cost hypotheses

Pinned registry paths are under `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

1. **Integer representation and rounding work.** `dashu-float-0.6.2/src/mul.rs:155` computes the exact IBig product before rounding. `repr_ops.rs:171` constructs a normalized representation; `repr.rs:726` splits high/low significands for rounding and constructs the rounded representation. The binary normalization path removes trailing zeroes. `dashu-int-0.6.2/src/mul_ops.rs:175` allocates a four-word buffer even for a spilled two-word product; the larger path allocates a full product buffer. By comparison, `astro-float-num-0.3.7/src/mantissa/mantissa.rs:453` rounds/truncates the product mantissa buffer in place. Both use small multiplication algorithms at the measured 128–512-bit sizes. Additional normalization, splitting, and allocation are plausible contributors; no profile establishes their individual shares.

2. **Borrowed addition can clone a significand.** `dashu-float-0.6.2/src/add.rs:628` clones the left representation when stored exponents differ. Canonical trailing-zero removal can give different stored exponents even to similarly sized values. This is a source-visible allocation/copy path and a plausible contributor to matrix kernels, not a quantified attribution.

3. **The logarithm workload exercises an expensive branch.** Every primitive input is in `[0.5,1)`. `dashu-float-0.6.2/src/log.rs:398` increases working precision for inputs below one because logarithm reconstruction can cancel. The function uses an atanh series with mechanically tracked ball errors, wrapped in a Ziv certification loop (`log.rs:629`). This branch necessarily applies to these inputs and also matters to Weiss completion's `ln(1-|sample|²)`. Its contribution to the observed timing and allocation growth is plausible but unprofiled. The additional precision is deliberate numerical protection.

4. **Contexts themselves are cheap.** `dashu-float-0.6.2/src/repr.rs:668` constructs a context from a precision field and a marker. The fixture uses explicit typed contexts and retained constant caches. There is no observed decimal conversion, context allocation, or input parsing inside the primitive timed loops. Caching these small contexts is unlikely to address the measured differences.

5. **No accidental primitive workload mismatch identified.** Deterministic binary inputs, input pairing, counts, directed modes, fresh-result policy, matrix multiplication, and modified Gram-Schmidt match the archived fixture. Native retained guard digits can change intermediate values and cost, so nominal bit precision is not identical arithmetic storage or a matched-residual guarantee. The comparison documentation states this distinction.

## Safe optimization candidates and constraints

- **Combined sine/cosine:** `dashu-float-0.6.2/src/math/trig.rs:481` provides `Context::sin_cos`, sharing argument reduction and series work while returning separately checked results. QSP currently has paired calls in exact interval sine/cosine, complex exponentials, phase controls, and FFT roots. Two directed pair calls can replace four scalar calls for an exact argument. Both results must propagate errors, retain the selected rounding direction, preserve guard digits, and pass the existing independent interval/verifier checks. An improvement is expected from removed duplicate work but remains unmeasured.
- **Invariant pi retrieval:** obtain the nearest pi value once per `offline::fft::roots(length)` call, outside its root loop. Precision and the represented pi value are invariant. This removes repeated cache retrieval/rounding/cloning without changing the values used to construct angles.
- **Optional exact-argument logarithm route:** for a represented remainder near one, `ln_1p(remainder - 1)` can use the small-argument no-scaling path at `log.rs:318`. The subtraction must be established exact so that the logarithm argument remains identical; a nearby interval such as remainder in `[0.5,1]` supports that contract. Verify argument identity with regression coverage, preserve domain/error propagation, and retain independent final QSP certification. Negative delta still triggers added working precision in the pinned implementation, so this is a possible series-length improvement, not a claim to eliminate the guard cost or a measured speedup.
- **Lower-priority explicit squares:** `Context::sqr` may avoid some generic multiply work where operands are known identical. Large IBig multiplication already detects equal operands, so the benefit may be small. This needs profiling before expanding the implementation.
- **Avoid speculative operator replacement:** assignments/operators do not automatically provide reusable storage; some native operator paths change failure handling. Preserve the fallible context boundary. Fused operations also change rounding sequences and require their own mathematical/acceptance review.

No recommendation weakens directed rounding, enclosure guarantees, typed arithmetic failure propagation, precision admission, or the independent verifier. The user-approved Dashu-only architecture remains the constraint. Final performance claims must come from separately recorded post-change runs at the same requested solver accuracy.
