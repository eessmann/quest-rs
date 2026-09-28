use crate::interval::{Grid, Interval, floor_div};
use crate::{AngleTarget, Axis, Cyclotomic, Error, Limits, Rational, Result, Sequence, Target};
use num_bigint::BigInt;
use num_traits::{Signed, Zero};
/// A mathematical full-matrix certificate. The squared Frobenius bound also bounds
/// the squared operator norm. Exact candidate, target and tolerance identities are owned.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ApproxCertificate {
    candidate: Sequence,
    target: Target,
    epsilon_bits: u64,
    bound_squared: Rational,
    precision_bits: usize,
}
impl ApproxCertificate {
    #[must_use]
    pub const fn candidate(&self) -> &Sequence {
        &self.candidate
    }
    #[must_use]
    pub const fn target(&self) -> &Target {
        &self.target
    }
    #[must_use]
    pub const fn epsilon_bits(&self) -> u64 {
        self.epsilon_bits
    }
    #[must_use]
    pub const fn bound_squared(&self) -> &Rational {
        &self.bound_squared
    }
    #[must_use]
    pub const fn precision_bits(&self) -> usize {
        self.precision_bits
    }
}
/// Certify a one-qubit rotation against an exact input identity and dyadic tolerance.
/// # Errors
/// Rejects invalid inputs, exhausted resources and unproved tolerances.
pub fn certify_rotation(
    candidate: &Sequence,
    target: &Target,
    epsilon_bits: u64,
    limits: Limits,
) -> Result<ApproxCertificate> {
    crate::matrix::admit(candidate, limits)?;
    if candidate.qubits != 1 {
        return Err(Error::Invalid(
            "rotation certificates require one qubit".into(),
        ));
    }
    let epsilon = dyadic_from_bits(epsilon_bits, limits)?;
    if epsilon <= Rational::zero() {
        return Err(Error::Invalid(
            "tolerance must be positive and finite".into(),
        ));
    }
    let input = Input::new(&target.angle, limits)?;
    let interval_limits = rotation_interval_limits(candidate.operations.len(), limits)?;
    let candidate_matrix = crate::reconstruct(candidate, limits)?;
    let tolerance_squared = std::ops::Mul::mul(&epsilon, &epsilon);
    for bits in [64, 128, 256, 512, 1024, 2048, 4096] {
        if bits > limits.precision_bits {
            break;
        }
        let grid = Grid::new(bits, interval_limits)?;
        let target_entries = match rotation_entries(&input, target.axis, &grid) {
            Ok(entries) => entries,
            Err(Error::NotCertified) => continue,
            Err(error) => return Err(error),
        };
        let (lower, upper) = difference_bounds(candidate_matrix.entries(), &target_entries, &grid)?;
        if upper <= tolerance_squared {
            return Ok(ApproxCertificate {
                candidate: candidate.clone(),
                target: target.clone(),
                epsilon_bits,
                bound_squared: upper,
                precision_bits: bits,
            });
        }
        if lower > tolerance_squared {
            return Err(Error::NotCertified);
        }
    }
    Err(Error::NotCertified)
}
/// Admit a rotation target before an external candidate producer starts.
///
/// Checks exact input identity and the minimum one-qubit certificate scratch.
/// Candidate-specific budgets are checked by `certify_rotation` afterward.
/// # Errors
/// Rejects invalid identities and targets that cannot fit the first proof grid.
pub fn admit_rotation_target(target: &Target, limits: Limits) -> Result<()> {
    let _input = Input::new(&target.angle, limits)?;
    let interval_limits = rotation_interval_limits(0, limits)?;
    let _grid = Grid::new(64, interval_limits)?;
    Ok(())
}
fn rotation_interval_limits(gates: usize, limits: Limits) -> Result<Limits> {
    let reserved = crate::matrix::memory(gates, 2, limits)?;
    let remaining = limits
        .bytes
        .checked_sub(
            usize::try_from(reserved)
                .map_err(|_| Error::Resource("reserved matrix bytes".into()))?,
        )
        .ok_or_else(|| Error::Resource("remaining interval bytes".into()))?;
    Ok(Limits {
        bytes: remaining,
        ..limits
    })
}
/// Decode finite binary64 bits exactly as a rational, without decimal or libm conversion.
/// # Errors
/// Rejects nonfinite bit patterns and resource excesses.
pub fn dyadic_from_bits(bits: u64, limits: Limits) -> Result<Rational> {
    let exponent = (bits >> 52) & 0x7ff;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    if exponent == 0x7ff {
        return Err(Error::Invalid("nonfinite binary64 input".into()));
    }
    let mantissa = if exponent == 0 {
        fraction
    } else {
        fraction | 0x0010_0000_0000_0000
    };
    if mantissa == 0 {
        return Ok(Rational::zero());
    }
    let shift = if exponent == 0 {
        -1074
    } else {
        i32::try_from(exponent)
            .map_err(|_| Error::Resource("float exponent".into()))?
            .checked_sub(1075)
            .ok_or_else(|| Error::Resource("float exponent".into()))?
    };
    let mut numerator = BigInt::from(mantissa);
    let mut denominator = BigInt::from(1);
    if shift >= 0 {
        numerator = std::ops::Shl::shl(
            numerator,
            u32::try_from(shift).map_err(|_| Error::Resource("float numerator shift".into()))?,
        );
    } else {
        denominator = std::ops::Shl::shl(denominator, shift.unsigned_abs());
    }
    if bits >> 63 != 0 {
        numerator = std::ops::Neg::neg(numerator);
    }
    check_rational(&numerator, &denominator, limits)?;
    Ok(Rational::new(numerator, denominator))
}
fn check_rational(numerator: &BigInt, denominator: &BigInt, limits: Limits) -> Result<()> {
    if denominator.is_zero() {
        return Err(Error::Invalid("zero rational denominator".into()));
    }
    let bits = numerator.bits().max(denominator.bits());
    if bits > limits.coefficient_bits {
        return Err(crate::types::budget(
            "coefficient bits",
            bits,
            limits.coefficient_bits,
        ));
    }
    crate::types::allocation(bits, 16, limits)
}
enum Input {
    Radians(Rational),
    Pi(Rational),
    AffinePi { radians: Rational, pi: Rational },
}
impl Input {
    fn new(angle: &AngleTarget, limits: Limits) -> Result<Self> {
        match angle {
            AngleTarget::DyadicRadians { bits } => Ok(Self::Radians(std::ops::Div::div(
                dyadic_from_bits(*bits, limits)?,
                BigInt::from(2),
            ))),
            AngleTarget::RationalPi {
                numerator,
                denominator,
            } => {
                check_rational(numerator, denominator, limits)?;
                let half = std::ops::Div::div(
                    Rational::new(numerator.clone(), denominator.clone()),
                    BigInt::from(2),
                );
                let period = std::ops::Mul::mul(half.denom(), 2);
                let quotient = floor_div(&std::ops::Add::add(half.numer(), half.denom()), &period)?;
                Ok(Self::Pi(std::ops::Sub::sub(
                    half,
                    Rational::from_integer(std::ops::Mul::mul(quotient, 2)),
                )))
            }
            AngleTarget::AffinePi {
                radians_numerator,
                radians_denominator,
                pi_numerator,
                pi_denominator,
            } => {
                check_affine_leaf(radians_numerator, radians_denominator, limits)?;
                check_affine_leaf(pi_numerator, pi_denominator, limits)?;
                let radians = std::ops::Div::div(
                    Rational::new(radians_numerator.clone(), radians_denominator.clone()),
                    BigInt::from(2),
                );
                let pi = std::ops::Div::div(
                    Rational::new(pi_numerator.clone(), pi_denominator.clone()),
                    BigInt::from(2),
                );
                check_affine_intermediate(&radians, limits)?;
                check_affine_intermediate(&pi, limits)?;
                Ok(Self::AffinePi { radians, pi })
            }
        }
    }
    fn reduced(&self, grid: &Grid) -> Result<Interval> {
        let pi = grid.pi()?;
        match self {
            Self::Pi(coefficient) => grid.mul(&grid.rational(coefficient)?, &pi),
            Self::Radians(value) => {
                let midpoint = Rational::new(
                    std::ops::Add::add(&pi.lower, &pi.upper),
                    std::ops::Mul::mul(&grid.scale, 2),
                );
                let quotient = std::ops::Div::div(
                    std::ops::Add::add(value, &midpoint),
                    std::ops::Mul::mul(&midpoint, BigInt::from(2)),
                );
                let periods = floor_div(quotient.numer(), quotient.denom())?;
                Ok(grid
                    .rational(value)?
                    .sub(&pi.scaled(&std::ops::Mul::mul(periods, 2))))
            }
            Self::AffinePi {
                radians,
                pi: coefficient,
            } => {
                let radians_grid = grid.rational(radians)?;
                let coefficient_grid = grid.rational(coefficient)?;
                let raw = radians_grid.add(&grid.mul(&coefficient_grid, &pi)?);
                check_affine_interval(&raw, grid)?;
                let pi_sum = std::ops::Add::add(&pi.lower, &pi.upper);
                let numerator =
                    std::ops::Add::add(std::ops::Add::add(&raw.lower, &raw.upper), &pi_sum);
                let periods = floor_div(&numerator, &std::ops::Mul::mul(&pi_sum, 2))?;
                check_affine_bits(periods.bits(), grid.limits)?;
                let reduced_coefficient = std::ops::Sub::sub(
                    coefficient,
                    Rational::from_integer(std::ops::Mul::mul(periods, 2)),
                );
                check_affine_intermediate(&reduced_coefficient, grid.limits)?;
                let reduced =
                    radians_grid.add(&grid.mul(&grid.rational(&reduced_coefficient)?, &pi)?);
                check_affine_interval(&reduced, grid)?;
                Ok(reduced)
            }
        }
    }
}
fn check_affine_leaf(numerator: &BigInt, denominator: &BigInt, limits: Limits) -> Result<()> {
    if denominator.is_zero() {
        return Err(Error::Invalid("zero affine-pi denominator".into()));
    }
    let bits = numerator.bits().max(denominator.bits());
    let cap = limits.coefficient_bits.min(16_384);
    if bits > cap {
        return Err(crate::types::budget("coefficient bits", bits, cap));
    }
    // Halving a rational may append one denominator bit before normalization.
    let half_bits = bits
        .checked_add(1)
        .ok_or_else(|| Error::Resource("affine-pi half-angle width".into()))?;
    crate::types::allocation(half_bits, 32, limits)
}
fn check_affine_bits(bits: u64, limits: Limits) -> Result<()> {
    let cap = limits
        .coefficient_bits
        .min(16_384)
        .checked_mul(4)
        .and_then(|bits| bits.checked_add(128))
        .ok_or_else(|| Error::Resource("affine-pi intermediate width".into()))?;
    if bits > cap {
        return Err(crate::types::budget(
            "affine-pi intermediate bits",
            bits,
            cap,
        ));
    }
    crate::types::allocation(bits, 32, limits)
}
fn check_affine_intermediate(value: &Rational, limits: Limits) -> Result<()> {
    check_affine_bits(value.numer().bits().max(value.denom().bits()), limits)
}
fn check_affine_interval(value: &Interval, grid: &Grid) -> Result<()> {
    let bits = value.lower.bits().max(value.upper.bits());
    let precision_bits = crate::types::size(grid.bits)?;
    let cap = grid
        .limits
        .coefficient_bits
        .min(16_384)
        .checked_mul(4)
        .and_then(|value| value.checked_add(precision_bits))
        .and_then(|value| value.checked_add(128))
        .ok_or_else(|| Error::Resource("affine-pi grid width".into()))?;
    if bits > cap {
        return Err(crate::types::budget("affine-pi grid bits", bits, cap));
    }
    crate::types::allocation(bits, 32, grid.limits)
}
struct ComplexInterval {
    real: Interval,
    imaginary: Interval,
}
impl ComplexInterval {
    fn conjugated(&self) -> Self {
        Self {
            real: self.real.clone(),
            imaginary: self.imaginary.negated(),
        }
    }
    fn add(&self, right: &Self) -> Self {
        Self {
            real: self.real.add(&right.real),
            imaginary: self.imaginary.add(&right.imaginary),
        }
    }
    fn multiply(&self, right: &Self, grid: &Grid) -> Result<Self> {
        Ok(Self {
            real: grid
                .mul(&self.real, &right.real)?
                .sub(&grid.mul(&self.imaginary, &right.imaginary)?),
            imaginary: grid
                .mul(&self.real, &right.imaginary)?
                .add(&grid.mul(&self.imaginary, &right.real)?),
        })
    }
}
/// Eight closed integer-coordinate intervals at a common dyadic scale `2^-bits`.
/// The coordinates are row-major real/imaginary components of a one-qubit matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DyadicBox8 {
    bits: usize,
    lower: [BigInt; 8],
    upper: [BigInt; 8],
}
impl DyadicBox8 {
    /// Construct a point box in scaled integer coordinates.
    /// # Errors
    /// Rejects unsupported precision.
    pub fn point(bits: usize, coordinates: [BigInt; 8]) -> Result<Self> {
        if bits == 0 || bits > 4096 {
            return Err(Error::Invalid("MITM box precision".into()));
        }
        Ok(Self {
            bits,
            lower: coordinates.clone(),
            upper: coordinates,
        })
    }
    /// Convert a checked exact one-qubit matrix to outward coordinate intervals.
    /// # Errors
    /// Rejects invalid shape, insufficient resources, and unsupported precision.
    pub fn from_exact(matrix: &crate::ExactMatrix, bits: usize, limits: Limits) -> Result<Self> {
        if matrix.qubits() != 1 || matrix.entries().len() != 4 {
            return Err(Error::Invalid("MITM enclosure requires one qubit".into()));
        }
        let grid = Grid::new(bits, limits)?;
        let root_half = grid.root_two()?.divided(&BigInt::from(2))?;
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(4)
            .map_err(|_| Error::Resource("MITM enclosure allocation".into()))?;
        for entry in matrix.entries() {
            entries.push(cyclotomic_interval(entry, &root_half, &grid)?);
        }
        Self::from_intervals(&entries, bits)
    }
    fn from_intervals(entries: &[ComplexInterval], bits: usize) -> Result<Self> {
        let [a, b, c, d] = entries else {
            return Err(Error::Invalid("MITM enclosure shape".into()));
        };
        let lower = [
            a.real.lower.clone(),
            a.imaginary.lower.clone(),
            b.real.lower.clone(),
            b.imaginary.lower.clone(),
            c.real.lower.clone(),
            c.imaginary.lower.clone(),
            d.real.lower.clone(),
            d.imaginary.lower.clone(),
        ];
        let upper = [
            a.real.upper.clone(),
            a.imaginary.upper.clone(),
            b.real.upper.clone(),
            b.imaginary.upper.clone(),
            c.real.upper.clone(),
            c.imaginary.upper.clone(),
            d.real.upper.clone(),
            d.imaginary.upper.clone(),
        ];
        Ok(Self { bits, lower, upper })
    }
    #[must_use]
    pub const fn bits(&self) -> usize {
        self.bits
    }
    /// Return a closed coordinate interval in scaled integer units.
    /// # Errors
    /// Rejects a coordinate outside 0..8.
    pub fn coordinate(&self, index: usize) -> Result<(&BigInt, &BigInt)> {
        Ok((
            self.lower
                .get(index)
                .ok_or_else(|| Error::Invalid("MITM coordinate".into()))?,
            self.upper
                .get(index)
                .ok_or_else(|| Error::Invalid("MITM coordinate".into()))?,
        ))
    }
    /// Expand this box to include another at the same precision.
    /// # Errors
    /// Rejects mixed scales.
    pub fn include(&mut self, other: &Self) -> Result<()> {
        if self.bits != other.bits {
            return Err(Error::Invalid("MITM box scale".into()));
        }
        for (left, right) in self.lower.iter_mut().zip(&other.lower) {
            *left = left.clone().min(right.clone());
        }
        for (left, right) in self.upper.iter_mut().zip(&other.upper) {
            *left = left.clone().max(right.clone());
        }
        Ok(())
    }
    /// Exact squared lower distance numerator in units of `2^(-2*bits)`.
    /// A boundary contact has zero gap and must not be pruned.
    /// # Errors
    /// Rejects mixed scales.
    pub fn gap_squared(&self, other: &Self) -> Result<BigInt> {
        if self.bits != other.bits {
            return Err(Error::Invalid("MITM box scale".into()));
        }
        let mut sum = BigInt::zero();
        for (((self_lower, self_upper), other_lower), other_upper) in self
            .lower
            .iter()
            .zip(&self.upper)
            .zip(&other.lower)
            .zip(&other.upper)
        {
            let gap = std::ops::Sub::sub(self_lower, other_upper)
                .max(std::ops::Sub::sub(other_lower, self_upper))
                .max(BigInt::zero());
            std::ops::AddAssign::add_assign(&mut sum, std::ops::Mul::mul(&gap, &gap));
        }
        Ok(sum)
    }
    /// Conservative retained-byte charge for boxes and independent bigint buffers.
    /// # Errors
    /// Rejects storage arithmetic overflow.
    pub fn retained_bytes(&self) -> Result<usize> {
        let mut bytes = std::mem::size_of::<Self>();
        for value in self.lower.iter().chain(&self.upper) {
            let limbs = usize::try_from(value.bits().div_ceil(64))
                .map_err(|_| Error::Resource("MITM box storage".into()))?;
            bytes = bytes
                .checked_add(
                    limbs
                        .checked_mul(16)
                        .and_then(|n| n.checked_add(24))
                        .ok_or_else(|| Error::Resource("MITM box storage".into()))?,
                )
                .ok_or_else(|| Error::Resource("MITM box storage".into()))?;
        }
        Ok(bytes)
    }
}
/// Enclose a one-qubit rotation using the independent certified angle path.
/// # Errors
/// Rejects malformed or over-budget target identity and interval failures.
pub fn rotation_enclosure(target: &Target, bits: usize, limits: Limits) -> Result<DyadicBox8> {
    let input = Input::new(&target.angle, limits)?;
    let grid = Grid::new(bits, limits)?;
    DyadicBox8::from_intervals(&rotation_entries(&input, target.axis, &grid)?, bits)
}
/// Enclose `right† * target` with outward complex interval arithmetic.
/// # Errors
/// Rejects invalid shape, target identity, resources or interval failures.
pub fn adjoint_times_rotation_enclosure(
    right: &crate::ExactMatrix,
    target: &Target,
    bits: usize,
    limits: Limits,
) -> Result<DyadicBox8> {
    if right.qubits() != 1 || right.entries().len() != 4 {
        return Err(Error::Invalid("MITM query requires one qubit".into()));
    }
    let input = Input::new(&target.angle, limits)?;
    let grid = Grid::new(bits, limits)?;
    let target_entries = rotation_entries(&input, target.axis, &grid)?;
    let root_half = grid.root_two()?.divided(&BigInt::from(2))?;
    let mut exact = Vec::new();
    exact
        .try_reserve_exact(4)
        .map_err(|_| Error::Resource("MITM query allocation".into()))?;
    for entry in right.entries() {
        exact.push(cyclotomic_interval(entry, &root_half, &grid)?);
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(4)
        .map_err(|_| Error::Resource("MITM query allocation".into()))?;
    let [e00, e01, e10, e11] = exact.as_slice() else {
        return Err(Error::Invalid("MITM exact shape".into()));
    };
    let [t00, t01, t10, t11] = target_entries.as_slice() else {
        return Err(Error::Invalid("MITM target shape".into()));
    };
    result.push(
        e00.conjugated()
            .multiply(t00, &grid)?
            .add(&e10.conjugated().multiply(t10, &grid)?),
    );
    result.push(
        e00.conjugated()
            .multiply(t01, &grid)?
            .add(&e10.conjugated().multiply(t11, &grid)?),
    );
    result.push(
        e01.conjugated()
            .multiply(t00, &grid)?
            .add(&e11.conjugated().multiply(t10, &grid)?),
    );
    result.push(
        e01.conjugated()
            .multiply(t01, &grid)?
            .add(&e11.conjugated().multiply(t11, &grid)?),
    );
    DyadicBox8::from_intervals(&result, bits)
}
fn rotation_entries(input: &Input, axis: Axis, grid: &Grid) -> Result<Vec<ComplexInterval>> {
    let (sine, cosine) = grid.sin_cos(&input.reduced(grid)?)?;
    let zero = grid.integer(0);
    let pairs = match axis {
        Axis::Z => [
            (cosine.clone(), sine.negated()),
            (zero.clone(), zero.clone()),
            (zero.clone(), zero),
            (cosine, sine),
        ],
        Axis::X => [
            (cosine.clone(), zero.clone()),
            (zero.clone(), sine.negated()),
            (zero.clone(), sine.negated()),
            (cosine, zero),
        ],
        Axis::Y => [
            (cosine.clone(), zero.clone()),
            (sine.negated(), zero.clone()),
            (sine, zero.clone()),
            (cosine, zero),
        ],
    };
    Ok(pairs
        .into_iter()
        .map(|(real, imaginary)| ComplexInterval { real, imaginary })
        .collect())
}
fn cyclotomic_interval(
    value: &Cyclotomic,
    root_half: &Interval,
    grid: &Grid,
) -> Result<ComplexInterval> {
    let [a, b, c, d] = value.coefficients();
    let real = Interval::point(std::ops::Shl::shl(a, grid.bits))
        .add(&root_half.scaled(&std::ops::Sub::sub(b, d)));
    let imaginary = Interval::point(std::ops::Shl::shl(c, grid.bits))
        .add(&root_half.scaled(&std::ops::Add::add(b, d)));
    let denominator = std::ops::Shl::shl(BigInt::from(1), value.denominator_exponent());
    Ok(ComplexInterval {
        real: real.divided(&denominator)?,
        imaginary: imaginary.divided(&denominator)?,
    })
}
fn difference_bounds(
    candidate: &[Cyclotomic],
    target: &[ComplexInterval],
    grid: &Grid,
) -> Result<(Rational, Rational)> {
    if candidate.len() != target.len() {
        return Err(Error::Invalid("matrix comparison dimension".into()));
    }
    let root_half = grid.root_two()?.divided(&BigInt::from(2))?;
    let mut lower = BigInt::zero();
    let mut upper = BigInt::zero();
    for (candidate, target) in candidate.iter().zip(target) {
        let candidate = cyclotomic_interval(candidate, &root_half, grid)?;
        for difference in [
            candidate.real.sub(&target.real),
            candidate.imaginary.sub(&target.imaginary),
        ] {
            let high = difference.magnitude();
            let low = if difference.lower <= BigInt::zero() && difference.upper >= BigInt::zero() {
                BigInt::zero()
            } else {
                difference.lower.abs().min(difference.upper.abs())
            };
            std::ops::AddAssign::add_assign(&mut lower, std::ops::Mul::mul(&low, &low));
            std::ops::AddAssign::add_assign(&mut upper, std::ops::Mul::mul(&high, &high));
        }
    }
    let denominator = std::ops::Mul::mul(&grid.scale, &grid.scale);
    Ok((
        Rational::new(lower, denominator.clone()),
        Rational::new(upper, denominator),
    ))
}
