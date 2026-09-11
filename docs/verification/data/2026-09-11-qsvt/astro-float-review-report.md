# Astro-float certification review

2026-09-11. Exact reviewed packages: astro-float 0.9.6, astro-float-num 0.3.7, astro-float-macro 0.4.6, Cargo registry sources downloaded for this workspace. This is an implementation and API review, not a formal verification of the dependency. Package source VCS metadata records revision `6c1c1fb5cd6c410a8aaa2b1d0c0aae08bacbfdf5` with `dirty: true`; downloaded package contents are the authoritative source reviewed.

## Decision and critical defect

Individual explicit directed BigFloat operations are suitable as the arithmetic boundary, subject to retaining the dependency's documented correct-rounding contract and testing the adapter independently. Do not use `expr!`, `RoundingMode::None`, decimal conversions, or final-only rounding for certification. The crate explicitly excludes correct rounding from the expression macro (astro-float/src/lib.rs:148–160), despite guaranteeing individual arithmetic, mathematical functions, conversions, and constants at lines 45–48. These guarantees are upstream assertions, not proof produced by this review.

**Confirmed defect:** `BigFloat::from_f64` in this exact release imports every tested nonzero binary64 subnormal at half its mathematical value. `astro-float-num/src/num.rs:811–853` increments the encoded exponent only for normal inputs; the subnormal path omits the implicit exponent correction. An independent bit-decoded construction demonstrates exact ratios of 1/2 for bits 1, 2, and `000fffffffffffff`, whereas minimum normal and 1 are correct. Source and runnable probe are in `astro-probe/`. The original certification regression for the smallest subnormal would fail if migrated mechanically. Always use the shared exact IEEE-bit import helper, including for normals, zero, negative values, and policy tolerances. Do not call upstream `from_f64` at the certification boundary.

## Directed arithmetic and state

`defs.rs:184–205` labels Up/Down as “Round half” but the actual mantissa rounding implementation (`mantissa/mantissa.rs:1049–1081`) increments a positive retained significand whenever any discarded bit is nonzero for Up, and a negative retained significand for Down. These are full directed modes toward +infinity and -infinity, not tie-only modes. ToEven is nearest-even; None skips rounding.

Precision requests round upward to machine words. Record admitted effective precision explicitly and account for rounded limb storage. BigFloat.precision() reports significant mantissa information (zero can report zero), not a stable configured precision. Use the interval's stored u32 precision and/or mantissa_max_bit_len(), never infer the interval policy from zero's precision(). Retrying at precision below the previous word-rounded allocation achieves no new precision. Reject unsupported policy alignment or report actual effective precision consistently.

The inexact flag is sticky across arithmetic arguments, and can be explicitly changed. It is not an error bound or a certificate. An interval endpoint is an exact finite dyadic number denoting a bound even when it was produced by rounding a different real value. Clear inexact when admitting that endpoint to the adapter; retain enclosure provenance in the interval type. Do not interpret a cleared flag as proving the original mathematical computation exact. Source sin.rs:121–147 uses enlarged working precision, argument reduction, and a roundability retry; copying its internal None operations to project code would discard its enclosing algorithm.

BigFloat arithmetic returns values, including NaN with an optional allocation/invalid-argument error. Overflow/division-by-zero may return infinities rather than NaN; undefined forms may return NaN without an associated error. Every public adapter admission and every arithmetic result consumed as an endpoint must reject NaN, infinities, and reversed endpoints before comparison; checking err() alone is insufficient. Source ext.rs:791–819. Consts::new() is fallible; own one cache per verification attempt/context. Upstream adaptive transcendental scratch allocations are not a hard process-memory cap and must be described honestly alongside logical modeled storage.

## Exact binary64 interchange algorithm

Import: decode sign, 11-bit exponent, and 52-bit fraction. A normal value has integer significand 2^52+fraction and scale encoded_exponent-1023-52. A subnormal has significand fraction and scale -1074. Construct that integer exactly at admitted precision >=64, then adjust its BigFloat binary exponent by the exact scale. Handle zero separately and preserve its sign if required. Reject nonfinite input. No binary64 arithmetic, period reduction, or decimal parsing is needed.

There is no public BigFloat to_f64. The internal test-only conversion in num.rs:861–900 rounds toward zero and is not an acceptable copied API. Implement safe raw-mantissa extraction: as_raw_parts exposes little-endian words and exponent; the value is sign * integer(words) * 2^(exponent - total_word_bits). Select normal spacing 2^(floor(log2|x|)-52) or subnormal spacing 2^-1074. Collect the retained <=53 bits and detect all discarded bits. Directed rounding increments magnitude for positive Up or negative Down iff discarded bits are nonzero; nearest-even uses guard/sticky/low-bit parity. Handle carry into the next exponent, normal/subnormal boundary, values smaller than minsubnormal (including exactly half), signed zeros, and overflow explicitly. Treat extreme BigFloat exponents before potentially overflowing integer shift arithmetic. Round once directly to binary64; rounding to 53 requested BigFloat bits is wrong because precision rounds to words and because binary64 subnormal spacing is fixed.

Certification acceptance must compare the original BigFloat bound with the exactly imported binary64 tolerance. Precompute outward summary f64 values when a Bound is created so read-only summary accessors remain infallible. Never accept based on a nearest or outward f64 summary. Preserve the existing above-tolerance-by-2^-100 and subnormal boundary tests.

## Rigorous FFT twiddles without sin_pi

For power-of-two N, reduce k modulo N with integer arithmetic before multiplying. Handle axes exactly, including N=1,2,4. For N>=8 let q=(k mod N)/(N/4), r=(k mod N)%(N/4). If r>N/8, replace r by N/4-r and record an exact sine/cosine swap. The residual angle a=2*pi*r/N lies in [0,pi/4]. The dyadic t=2r/N is exactly representable at admitted precision and current u32 FFT-size limit.

Compute pi_lo=Consts.pi(p,Down), pi_hi=Consts.pi(p,Up), then a_lo=mul_down(t,pi_lo), a_hi=mul_up(t,pi_hi). Check both are finite, ordered, and in [0,1]; since 1<pi/2, sine increases and cosine decreases throughout that entire verified interval. Return sin bounds [sin_down(a_lo), sin_up(a_hi)] and cos bounds [cos_down(a_hi), cos_up(a_lo)]. Undo the optional swap, then rotate exactly by quadrant: q=0 -> (s,c), 1 -> (c,-s), 2 -> (-s,-c), 3 -> (-c,s). Negation reverses interval endpoints. This avoids dependence on deciding rounded points' proximity to extrema. Directly evaluating sin/cos at a single rounded pi product does not enclose the exact twiddle.

Frozen arbitrary binary64 phases use directed sin/cos of the exact decoded phase itself, with upstream multiprecision argument reduction. Never reduce by binary64 TAU, including for large phases such as 1e20. The existing independent large-phase fixture specifically detects that mistake.

## Migration verification requirements

- Run the independent subnormal probe and retain bit-level regressions for both signs, maximum subnormal, minimum normal, half-minsubnormal, and their outward summaries.
- Cover directed operations with positive/negative non-tie and tie discarded bits, cancellation, exact zero, infinity/NaN rejection, word precision boundaries, and explicit precision budgets.
- Check f64 export at exact representables, adjacent-bin midpoints, normal/subnormal transition, carry/overflow, and exponents far outside binary64 range; compare raw integer expected bit patterns rather than using the same converter as oracle.
- Test all small FFT quadrants, exact axes, octant boundaries, modular-index equivalence, conjugacy, and directed square-root enclosure for sqrt(1/2); retain the exact rational direct-convolution-contained-in-FFT test.
- Retain original-frozen-payload retries, above-tolerance precision comparison, large unreduced phase, invalid original-source conversion, analytic all-four-entry reconstruction, and complex cancellation tests. Replace MPFR-generated reference values with independently fixed binary64/bit/rational fixtures where possible; do not make synthesis and verifier share their numerical construction.
- Native MPI and historical C++ MPFR evidence are independent of this dependency migration. No new production Rug dependency or automatic high-precision fallback is justified.

Primary references: downloaded exact package source above; [astro-float documentation](https://docs.rs/astro-float/0.9.6/astro_float/), [upstream repository](https://github.com/stencillogic/astro-float), [published numeric package source](https://docs.rs/crate/astro-float-num/0.3.7/source/src/num.rs). Cached online documentation for 0.9.5 was readable, while exact-version docs fetch failed; conclusions above were checked against the downloaded 0.9.6/0.3.7 Rust sources.

## Implemented certification migration

After the review, the parent authorized conversion of `quest-qsp/src/certification/**` and its tests. The implementation uses the parent-owned `precision::{checked,exact_from_f64,to_f64,BinaryRounding}` adapter and direct Astro operations. `MpInterval` stores explicit u32 working precision; bounds are admitted through checked finite endpoints. One Consts cache belongs to each verification Context. Integer octant reduction, directed pi products, monotone sine/cosine endpoints, and exact axis/sign/swap operations implement FFT roots. Frozen arbitrary phase input is imported bit-exactly and never reduced in binary64. All certification production and test Rug references were removed.

The public Bound getters return read-only BigFloat endpoints; binary64 summaries are precomputed outward at Bound admission. Acceptance still compares exact original multiprecision endpoints to exactly imported binary64 tolerances. The independent large-phase reference is a fixed binary64 fixture rather than a value computed by the verifier itself. Existing cancellation, analytic matrix, original-source conversion, retry, and subnormal regressions remain.

Policy initial/max precision must now be machine-word aligned. Storage modeling rounds to allocated word capacity and includes 64 bytes per scalar plus the conservative coefficient/tree/FFT slot envelope and immutable source bytes. Retry reports and root/cache storage are dropped at the end of each loop iteration before constructing the next Context; only compact attempt metadata survives, while work is accumulated and checked across retries. As documented on CertificationPolicy, this model is neither a hard allocator cap nor preemption of Astro's internal adaptive correct-rounding iterations. All large-catalog attempts are checked against the same original requested tolerances without fallback.

Validation before catalog runs:

- Isolated exact-release probe confirms the import defect and directed non-tie rounding. Reproduction source, manifest/lock, and `results.txt` are in `astro-probe/`; no dependency source was patched.
- Focused certification/lib integration suite: **22 passed**, run `fc294370-dc6a-4f16-ba31-3e1e1a813774`, 3.144 seconds. The separate analytic degree-8192 fixture remains opt-in; real required catalog degree-8105 runs are recorded separately below.
- Strict `cargo clippy -p quest-qsp --features certification --tests --locked --offline -- -D warnings` passed.
- Certification-enabled docs: **one compiling example and two compile-fail examples passed**.
- Catalog binaries built in release with `CARGO_BUILD_JOBS=2`, locked offline dependencies. New files use the `astro-` prefix; historical MPFR catalog JSONL remains untouched.

Independent review subsequently identified possible suppression of an allocation-error NaN sentinel by an exact-zero multiplication or absolute-value comparison. Allocation-free endpoint checks now precede arithmetic and shortcuts; absolute bounds also check newly allocated absolute endpoint copies before ordering. A focused internal regression supplies a MemoryAllocation sentinel as test data and verifies propagation through multiplication in both operand orders, absolute bounds, square, addition, and division. No production fault-injection mechanism was added. Final focused validation after this correction: **23/23 passed**, run `0194dded-4843-4c1e-bac7-3ca6d04ef1c6`, 3.232 seconds; strict certification tests Clippy and formatting/diff checks passed. Catalog binaries below were frozen before this error-propagation-only correction, as requested by the parent; their finite arithmetic path is unchanged.

A second review extended the same guard to whole-polynomial convolution: both input slices are validated before either all-zero shortcut, so a zero polynomial cannot conceal a backend error in the other operand at any recursive tree level. Tests cover both operand orders and Direct/IntervalFft modes. The private direct kernel's zero skips are reached only after that preflight; scalar and matrix norm reductions consume checked magnitudes/arithmetic and revalidate final Bound construction. Final focused suite: **24/24 passed**, run `dfc25322-1077-42ea-9c1e-3f1237d4431d`, 3.232 seconds, with strict certification tests Clippy passing afterward.

## Required full catalog acceptance

Both release runners exited zero, with empty stderr files. **All 43 cases certified at the first 256-bit attempt:** 21 original canonical inverse-catalog families, the corresponding 21 generalized real circular targets, and the separately frozen complex degree-8105 target. Every response, completion, conversion, reconstruction, and unitarity upper bound met the unchanged 1e-11 policy; no offline synthesis, normalization, precision fallback, or skipped family was used.

Evidence files:

- `astro-canonical-catalog-certification.jsonl`: 21 rows, all five bounds, stage timings, attempt precision, work, and modeled storage.
- `astro-generalized-catalog-certification.jsonl`: 22 rows, all five bounds, stage timings and attempt counts; generalized executable checks every exported control-component bit remains unchanged.
- `astro-catalog-summary.json`: mechanically validated row counts/statuses/tolerances, aggregate maxima, and complete degree-8105 rows.
- `astro-catalog-binary-sha256.txt`: frozen release executable identities. Matching `astro-*-catalog-stderr.log` files are empty.

| Degree-8105 route | Response upper | Reconstruction upper | Certification seconds |
| --- | ---: | ---: | ---: |
| Canonical | 2.2780094700559387e-15 | 5.33487399449989e-15 | 43.846831025 |
| Generalized real | 1.6749583519531505e-14 | 3.8068629549063936e-14 | 43.632104199 |
| Generalized complex | 6.324147457191736e-15 | 1.41748050048068e-14 | 44.749678543 |

Across all canonical cases the largest response/completion/reconstruction upper bounds were 6.728137294228876e-14, 1.2303431820348664e-12, and 9.247860170117727e-13. The generalized set including complex had corresponding maxima 6.990191185414253e-14, 1.2303431820348664e-12, and 9.251263209266036e-13. All conversions were exactly zero. The maximum canonical modeled peak storage was 848684976 bytes, and degree-8105 used 6657816288 precision-weighted work units.

Summed certification wall times were 207.668926141 seconds for canonical and 251.739136249 seconds for generalized including complex. The two serial runners executed concurrently on this host, so these measurements are acceptance timing evidence, not a controlled comparative benchmark against historical MPFR. The complex target is the same explicitly defined binary64 snapshot as the prior generalized runner; it does not assert exact preservation of a global phase under rounded multiplication. Historical MPFR JSONL remains unchanged.
