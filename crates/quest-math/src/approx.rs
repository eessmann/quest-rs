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
    let reserved = crate::matrix::memory(candidate.operations.len(), 2, limits)?;
    let remaining = limits
        .bytes
        .checked_sub(
            usize::try_from(reserved)
                .map_err(|_| Error::Resource("reserved matrix bytes".into()))?,
        )
        .ok_or_else(|| Error::Resource("remaining interval bytes".into()))?;
    let interval_limits = Limits {
        bytes: remaining,
        ..limits
    };
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
        }
    }
}
struct ComplexInterval {
    real: Interval,
    imaginary: Interval,
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
