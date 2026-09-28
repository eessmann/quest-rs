# quest-math

Project-owned, bounded verification for exact Clifford+T circuits and approximate
single-qubit rotations. Candidate generation is external to this crate. Every
certificate includes the complete owned candidate and target, and every comparison
preserves scalar phase.

```rust
use quest_math::{Gate, Limits, Operation, Sequence, verify_exact};
let identity = Sequence { qubits: 1, operations: vec![] };
let hh = Sequence {
    qubits: 1,
    operations: [Gate::H, Gate::H].into_iter().map(|gate| Operation {
        gate, targets: vec![0], controls: vec![],
    }).collect(),
};
let certificate = verify_exact(&hh, &identity, Limits::default())?;
assert_eq!(certificate.candidate(), &hh);
# Ok::<(), quest_math::Error>(())
```

`Sequence`, `Target`, and the other input DTOs are untrusted data. Entry points
validate their widths, arities, unique disjoint targets and controls, finite
binary64 inputs, and resource bounds. Certificates have private constructors and
support serialization, but do not support deserialization. Deserializing a DTO
never grants a certificate.

## Exact arithmetic and matrix convention

`Cyclotomic` represents `(a + bω + cω² + dω³) / 2^k`, where
`ω = exp(iπ/4)` and `ω⁴ = -1`. Integer coefficients and the denominator exponent
are private. Construction strips common powers of two; zero has a unique
representation. Addition, multiplication, and conjugation are integer operations.
Coefficient and scratch limits apply before expensive arithmetic.

Sequences execute from left to right; each operation left-multiplies the matrix
built so far. Matrices are row major, computational basis bit zero is qubit zero,
and the first ordered target is local bit zero. `Cx` controls its first target
and flips its second. Additional positive and negative controls are disjoint from
targets. `W` has no targets and multiplies by `ω`, including when controlled.
`reconstruct` supports at most four qubits, including a zero-qubit scalar circuit.
`verify_exact` compares every canonical matrix entry. `recover_eighth_root_phase`
tries the eight scalar powers independently and returns a corrected owned
sequence plus its exact certificate only if a full matrix match succeeds.

## Approximation proof

`certify_rotation` accepts a one-qubit candidate and `Rx`, `Ry`, or `Rz` target.
`DyadicRadians { bits }` denotes the exact rational number encoded by finite
binary64 bits. `RationalPi` denotes a rational multiple of mathematical π.
`AffinePi` denotes exact rational radians `r + sπ` with separate numerator and
denominator fields for each coefficient. The two terms remain exact through
half-angle reduction and are independently enclosed on each precision grid.
These identities remain distinct, even when a binary64 value happens to equal
an ordinary approximation to π. Epsilon likewise denotes an exact, strictly
positive finite dyadic number. Signed zero in the input DTO remains recorded.

All production interval endpoints are integers in units of `2^-p`:

1. π is enclosed using `16 atan(1/5) - 4 atan(1/239)`. Alternating rational
   series terms are floored at the chosen scale. After `k` included terms, the
   sum of term-rounding errors is less than `k` units; the omitted alternating
   tail contributes less than one more unit. These errors are carried outward
   through the Machin combination.
2. Rational multiples of π are reduced exactly before interval multiplication.
   Affine half-angles use `r/2 + (s/2 - 2k)π` for an integer `k` chosen from
   a grid enclosure. The enclosure of both terms remains outward; input and
   intermediate coefficient growth are bounded before admission.
   General dyadic half-angles subtract an integer multiple of an interval for
   `2π`; subtraction preserves containment even when the chosen integer is only
   estimated from the midpoint. A reduced interval outside `[-4,4]` triggers
   refinement or rejection. Half-angle reduction preserves the `4π` period of
   rotations, so `Rz(2π) = -I`.
3. Sine and cosine use interval Taylor recurrences on `[-4,4]`. Once the terms
   decrease, the magnitude of the first omitted term bounds the alternating
   remainder for every point in the interval. All recurrence multiplication and
   division round outward; the partial sum is widened by that remainder.
4. An integer square-root algorithm encloses `sqrt(2)` between adjacent dyadic
   numbers. It encloses each exact cyclotomic coefficient without floating point.
5. For every complex matrix difference, real and imaginary intervals are squared
   outward, with a zero lower bound when they straddle zero. Summing their upper
   bounds gives a rational upper bound on the **squared Frobenius norm**. A
   certificate is issued only when this is no greater than exact `epsilon²`.

The Frobenius norm bounds the operator norm. Consequently these certificates
also provide an operator error bound preserved by unitary conjugation,
embedding into a controlled block, and tensoring with identity. Operator errors
of sequential replacements add by the triangle inequality. The Frobenius bound
itself need not remain unchanged under tensoring with identity. A candidate may
satisfy an operator-norm tolerance while failing this more conservative
Frobenius test.

`lift_controlled_rotation` turns an admitted one-qubit approximation certificate
into a signed-control certificate without new numerical approximation. The
declared target and controls must cover the entire output register. On the one
control-active block, the lifted candidate-minus-ideal matrix is exactly the
base difference; every other block is zero. Its squared Frobenius bound is
therefore the base bound. Allowing spectator qubits would repeat that block and
would require scaling the Frobenius bound, so the lifting API rejects incomplete
interfaces. It maps every base operation, including scalar `W`, and preserves
existing base controls by remapping their qubit-zero operand to the new target.

No libm residual, candidate-generator claim, or phase-insensitive overlap enters
the proof. The certificate concerns mathematical gates. Native binary64 matrix
realization and simulation introduce separate rounding errors.

## Bounds and rejection

The caller controls maximum gates, qubits, coefficient bits, allocation forecast,
precision bits, and Taylor terms. Four qubits and 4096 interval bits are hard
caps. Precision progresses through powers of two from 64 bits within the supplied
limit. A result that cannot be certified at permitted precision returns
`NotCertified`; there is no tolerance relaxation. Coefficient limits conservatively
apply to intermediate and supplied representations before normalization, so a
small final value can still exceed a construction budget.

The byte budget combines conservative matrix coefficient, operation/certificate
copy, and arithmetic scratch forecasts. Integer size arithmetic is checked before
allocation and work. This is a logical allocation forecast, not a proof about
process RSS: allocator bookkeeping, dependency implementation details, and cached
π intervals are outside that model. An untrusted worker should also have process
memory and execution-time limits. The crate does not impose a wall-clock deadline.

## Independent evidence

Tests cover exact ring identities; matrix ordering; signed controls; non-sorted
targets; phase rejection and recovery; exact versus dyadic π; all rotation axes;
subnormal and maximal finite binary64 inputs; and acceptance/rejection around
explicit tolerance boundaries. The golden `||T-I||_F² = 2-sqrt(2)` checks the
reported bound independently.

`tests/fixtures/generate_oracle.py` uses Python's standard-library Decimal,
Gauss–Legendre AGM π, and independent dense complex matrix multiplication. It
regenerates 24 committed fixed fixtures at 100 and 140 decimal digits and checks
agreement before writing bounds. The Rust tests exercise both sides of each
fixture's tolerance boundary and check the rational certificate against its
independent residual bracket. Run the generator from the repository root with
`python3 crates/quest-math/tests/fixtures/generate_oracle.py`.
