//! Exact rational storage over the workspace's current bigint version.
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};

/// Arbitrary rational numbers using the workspace's `num-bigint` version.
///
/// `num-rational`'s built-in alias uses bigint 0.4. This project-owned alias
/// uses the same generic rational implementation with bigint 0.5 instead.
pub type BigRational = num_rational::Ratio<BigInt>;

/// Correctly rounded binary64 conversion without intermediate integer floats.
pub fn to_f64(value: &BigRational) -> Option<f64> {
    if value.denom().is_zero() {
        return None;
    }
    if value.numer().is_zero() {
        return Some(0.0);
    }
    let negative = value.numer().is_negative() != value.denom().is_negative();
    let numerator = value.numer().magnitude();
    let denominator = value.denom().magnitude();
    let mut exponent = i128::from(numerator.bits()).checked_sub(i128::from(denominator.bits()))?;
    let sign = if negative { 0x8000_0000_0000_0000 } else { 0 };
    if exponent > 1024 {
        return None;
    }
    if exponent < -1075 {
        return Some(f64::from_bits(sign));
    }
    let below_power = if exponent >= 0 {
        numerator < &std::ops::Shl::shl(denominator, usize::try_from(exponent).ok()?)
    } else {
        std::ops::Shl::shl(numerator, usize::try_from(exponent.checked_neg()?).ok()?) < *denominator
    };
    if below_power {
        exponent = exponent.checked_sub(1)?;
    }
    let scale = if exponent < -1022 {
        1074
    } else {
        52i128.checked_sub(exponent)?
    };
    let (scaled_numerator, scaled_denominator) = if scale >= 0 {
        (
            std::ops::Shl::shl(numerator, usize::try_from(scale).ok()?),
            denominator.clone(),
        )
    } else {
        (
            numerator.clone(),
            std::ops::Shl::shl(denominator, usize::try_from(scale.checked_neg()?).ok()?),
        )
    };
    let quotient = std::ops::Div::div(&scaled_numerator, &scaled_denominator);
    let remainder = std::ops::Rem::rem(&scaled_numerator, &scaled_denominator);
    let twice_remainder = std::ops::Shl::shl(remainder, 1usize);
    let mut mantissa = quotient.to_u64()?;
    if twice_remainder > scaled_denominator
        || (twice_remainder == scaled_denominator && mantissa & 1 == 1)
    {
        mantissa = mantissa.checked_add(1)?;
    }
    if exponent < -1022 {
        return Some(f64::from_bits(sign | mantissa));
    }
    if mantissa == 0x0020_0000_0000_0000 {
        mantissa >>= 1;
        exponent = exponent.checked_add(1)?;
    }
    if exponent > 1023 {
        return None;
    }
    let stored_exponent = u64::try_from(exponent.checked_add(1023)?).ok()?;
    let fraction = mantissa.checked_sub(0x0010_0000_0000_0000)?;
    Some(f64::from_bits(
        sign | stored_exponent.checked_shl(52)? | fraction,
    ))
}

/// Failure to obtain a finite, certified binary64 value for a rational multiple of pi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiConversionError {
    NonFinite,
    Precision,
}

/// Convert against mathematical pi, admitting only a unique rounded interval result.
///
/// Machin bounds are cached at increasing precisions. A rational input can lie
/// arbitrarily close to a binary64 midpoint, so unresolved cases fail at a fixed
/// 4096-bit precision cap instead of silently selecting a neighboring float.
pub fn to_pi_f64(value: &BigRational) -> Result<f64, PiConversionError> {
    static BOUNDS: [std::sync::OnceLock<Result<PiBounds, PiConversionError>>; 6] =
        [const { std::sync::OnceLock::new() }; 6];
    if value.denom().is_zero() {
        return Err(PiConversionError::NonFinite);
    }
    if value.numer().is_zero() {
        return Ok(0.0);
    }
    for (bits, cache) in [128, 256, 512, 1024, 2048, 4096].into_iter().zip(&BOUNDS) {
        let bounds = cache
            .get_or_init(|| pi_bounds(bits))
            .as_ref()
            .map_err(|error| *error)?;
        let lower = to_f64(&std::ops::Mul::mul(value, &bounds.lower));
        let upper = to_f64(&std::ops::Mul::mul(value, &bounds.upper));
        match (lower, upper) {
            (Some(lower), Some(upper)) if lower.to_bits() == upper.to_bits() => return Ok(lower),
            (None, None) => return Err(PiConversionError::NonFinite),
            _ => {}
        }
    }
    Err(PiConversionError::Precision)
}
struct PiBounds {
    lower: BigRational,
    upper: BigRational,
}
fn pi_bounds(bits: usize) -> Result<PiBounds, PiConversionError> {
    // Machin's identity: pi = 16 atan(1/5) - 4 atan(1/239).
    let (five_lower, five_upper) = arctangent_bounds(5, bits)?;
    let (large_lower, large_upper) = arctangent_bounds(239, bits)?;
    let lower = std::ops::Sub::sub(
        std::ops::Mul::mul(five_lower, 16),
        std::ops::Mul::mul(large_upper, 4),
    );
    let upper = std::ops::Sub::sub(
        std::ops::Mul::mul(five_upper, 16),
        std::ops::Mul::mul(large_lower, 4),
    );
    let scale = std::ops::Shl::shl(BigInt::from(1), bits);
    Ok(PiBounds {
        lower: BigRational::new(lower, scale.clone()),
        upper: BigRational::new(upper, scale),
    })
}
fn arctangent_bounds(reciprocal: u16, bits: usize) -> Result<(BigInt, BigInt), PiConversionError> {
    let scale = std::ops::Shl::shl(BigInt::from(1), bits);
    let square = std::ops::Mul::mul(BigInt::from(reciprocal), BigInt::from(reciprocal));
    let mut power = BigInt::from(reciprocal);
    let mut sum = BigInt::zero();
    for index in 0..bits {
        let odd = index
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or(PiConversionError::Precision)?;
        let denominator = std::ops::Mul::mul(&power, odd);
        let term = std::ops::Div::div(&scale, denominator);
        if term.is_zero() {
            // Each of index truncated terms has absolute error < one scaled
            // unit. The alternating-series remainder is below one further unit.
            let error = BigInt::from(index.checked_add(1).ok_or(PiConversionError::Precision)?);
            return Ok((
                std::ops::Sub::sub(&sum, &error),
                std::ops::Add::add(sum, error),
            ));
        }
        if index.is_multiple_of(2) {
            std::ops::AddAssign::add_assign(&mut sum, term);
        } else {
            std::ops::SubAssign::sub_assign(&mut sum, term);
        }
        std::ops::MulAssign::mul_assign(&mut power, &square);
    }
    Err(PiConversionError::Precision)
}

#[cfg(test)]
mod tests {
    use super::{BigRational, to_f64, to_pi_f64};
    use googletest::prelude::*;
    use num_bigint::BigInt;

    #[gtest]
    fn rational_conversion_rounds_ties_and_subnormals_once() {
        let power = |shift| std::ops::Shl::shl(BigInt::from(1), shift);
        let half_ulp = BigRational::new(std::ops::Add::add(power(53usize), 1), power(53usize));
        expect_eq!(to_f64(&half_ulp), Some(1.0));
        let higher_tie = BigRational::new(std::ops::Add::add(power(53usize), 3), power(53usize));
        expect_eq!(
            to_f64(&higher_tie),
            Some(f64::from_bits(0x3ff0_0000_0000_0002))
        );
        expect_eq!(
            to_f64(&BigRational::new(BigInt::from(1), power(1074usize))),
            Some(f64::from_bits(1))
        );
        expect_eq!(
            to_f64(&BigRational::new(BigInt::from(1), power(1075usize))),
            Some(0.0)
        );
        expect_eq!(
            to_f64(&BigRational::new(BigInt::from(-3), power(1075usize))),
            Some(f64::from_bits(0x8000_0000_0000_0002))
        );
        expect_true!(to_f64(&BigRational::from_integer(power(1024usize))).is_none());
    }
    #[gtest]
    fn rational_pi_keeps_subnormals_until_the_final_rounding() {
        let denominator = std::ops::Shl::shl(BigInt::from(1), 1075usize);
        let tiny = BigRational::new(BigInt::from(1), denominator);
        expect_eq!(to_pi_f64(&tiny).map(f64::to_bits), Ok(2));
        expect_eq!(
            to_pi_f64(&std::ops::Neg::neg(tiny)).map(f64::to_bits),
            Ok(0x8000_0000_0000_0002)
        );
    }
    #[gtest]
    fn rational_pi_refines_around_an_independently_computed_midpoint() {
        // These adjacent dyadic ratios straddle (1 + 2^-53) / pi.
        // Independent 1400/1600-digit Gauss-Legendre AGM oracles agree.
        let denominator = std::ops::Shl::shl(BigInt::from(1), 128usize);
        for (numerator, bits) in [
            (
                108_315_241_484_954_830_072_309_728_913_637_971_665u128,
                0x3ff0_0000_0000_0000,
            ),
            (
                108_315_241_484_954_830_072_309_728_913_637_971_666u128,
                0x3ff0_0000_0000_0001,
            ),
        ] {
            let ratio = BigRational::new(BigInt::from(numerator), denominator.clone());
            expect_eq!(to_pi_f64(&ratio).map(f64::to_bits), Ok(bits));
        }
    }
    #[gtest]
    fn rational_pi_rejects_uncertainty_beyond_the_precision_cap() -> googletest::Result<()> {
        let fine = super::pi_bounds(8192)
            .map_err(|_| std::io::Error::other("reference bounds unavailable"))?;
        let approximate_pi =
            std::ops::Div::div(std::ops::Add::add(fine.lower, fine.upper), BigInt::from(2));
        let denominator = std::ops::Shl::shl(BigInt::from(1), 53usize);
        let midpoint = BigRational::new(std::ops::Add::add(&denominator, 1), denominator);
        let ratio = std::ops::Div::div(midpoint, approximate_pi);
        expect_eq!(to_pi_f64(&ratio), Err(super::PiConversionError::Precision));
        Ok(())
    }
}
