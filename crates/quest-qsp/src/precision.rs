//! Checked binary64 interchange for the separate arbitrary-precision stages.
//!
//! Imports and exports use binary significands directly. No decimal conversion,
//! double rounding or host floating-point rounding mode participates.
use astro_float::{BigFloat, Sign, Word};

/// Failures of checked arbitrary-precision arithmetic or binary interchange.
#[derive(Debug, thiserror::Error)]
pub enum PrecisionError {
    /// The arbitrary-precision backend returned an error.
    #[error("arbitrary-precision backend: {0}")]
    Backend(#[from] astro_float::Error),
    /// An operation or import produced infinity or NaN.
    #[error("arbitrary-precision arithmetic produced a nonfinite value")]
    Nonfinite,
    /// Precision or significand/exponent data cannot satisfy the requested conversion.
    #[error("invalid arbitrary-precision interchange: {0}")]
    Interchange(&'static str),
}
/// Requested direction for checked dyadic-to-binary64 export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryRounding {
    /// Round toward negative infinity.
    Down,
    /// Round to nearest, choosing the even significand in a tie.
    Nearest,
    /// Round toward positive infinity.
    Up,
}
type Result<T> = std::result::Result<T, PrecisionError>;

/// Admit a finite represented value as an exact dyadic endpoint.
/// # Errors
/// Propagates backend allocation/arithmetic failures and rejects infinity/NaN.
pub fn checked(mut value: BigFloat) -> Result<BigFloat> {
    if let Some(error) = value.err() {
        return Err(error.into());
    }
    if value.is_nan() || value.is_inf() {
        return Err(PrecisionError::Nonfinite);
    }
    // Rounding provenance belongs to the containing interval, not a hidden
    // uncertainty in the dyadic endpoint's actual stored significand.
    value.set_inexact(false);
    Ok(value)
}

/// Import the exact represented binary64 value, including subnormals and -0.
/// # Errors
/// Rejects nonfinite input, insufficient precision and backend allocation failure.
pub fn exact_from_f64(value: f64, precision: u32) -> Result<BigFloat> {
    if !value.is_finite() {
        return Err(PrecisionError::Nonfinite);
    }
    if precision < 53 {
        return Err(PrecisionError::Interchange(
            "binary64 needs at least 53 bits",
        ));
    }
    // BigFloat's integer constructor requires storage for all 64 source bits,
    // even when this binary64 significand uses at most 53 of them.
    let p =
        usize::try_from(precision.max(64)).map_err(|_| PrecisionError::Interchange("precision"))?;
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent =
        i32::try_from((bits >> 52) & 0x7ff).map_err(|_| PrecisionError::Interchange("exponent"))?;
    let mut significand = bits & ((1_u64 << 52) - 1);
    let scale = if exponent == 0 {
        -1074
    } else {
        significand |= 1_u64 << 52;
        exponent
            .checked_sub(1075)
            .ok_or(PrecisionError::Interchange("exponent"))?
    };
    let mut result = checked(BigFloat::from_u64(significand, p))?;
    if significand != 0 {
        let exponent = result
            .exponent()
            .and_then(|e| e.checked_add(scale))
            .ok_or(PrecisionError::Interchange("exponent"))?;
        result.set_exponent(exponent);
    }
    result.set_sign(if negative { Sign::Neg } else { Sign::Pos });
    checked(result)
}
const WORD_BITS: usize = astro_float::WORD_BIT_SIZE;
fn bit(words: &[Word], position: i64) -> bool {
    usize::try_from(position)
        .ok()
        .and_then(|p| {
            words
                .get(p / WORD_BITS)
                .map(|w| (w >> (p % WORD_BITS)) & 1 != 0)
        })
        .unwrap_or(false)
}
fn any_below(words: &[Word], exclusive: i64) -> bool {
    if exclusive <= 0 {
        return false;
    }
    let Ok(count) = usize::try_from(exclusive) else {
        return words.iter().any(|w| *w != 0);
    };
    let whole = count / WORD_BITS;
    if words.iter().take(whole).any(|w| *w != 0) {
        return true;
    }
    let remaining = count % WORD_BITS;
    remaining != 0
        && words
            .get(whole)
            .is_some_and(|w| w & (Word::MAX >> WORD_BITS.saturating_sub(remaining)) != 0)
}
fn overflow(negative: bool, rounding: BinaryRounding) -> f64 {
    let infinite = rounding == BinaryRounding::Nearest
        || (rounding == BinaryRounding::Up && !negative)
        || (rounding == BinaryRounding::Down && negative);
    let value = if infinite { f64::INFINITY } else { f64::MAX };
    if negative { -value } else { value }
}

/// Correctly round one exact stored dyadic to binary64, with gradual underflow.
/// Infinity is a valid outward result for finite values exceeding binary64.
/// # Errors
/// Rejects nonfinite input and unrepresentable backend metadata.
pub fn to_f64(value: &BigFloat, rounding: BinaryRounding) -> Result<f64> {
    if let Some(error) = value.err() {
        return Err(error.into());
    }
    let (words, _, sign, exponent, _) = value.as_raw_parts().ok_or(PrecisionError::Nonfinite)?;
    let negative = sign == Sign::Neg;
    let sign_bit = if negative { 1_u64 << 63 } else { 0 };
    let Some((word_index, word)) = words.iter().enumerate().rfind(|(_, w)| **w != 0) else {
        return Ok(f64::from_bits(sign_bit));
    };
    let total = words
        .len()
        .checked_mul(WORD_BITS)
        .and_then(|n| i64::try_from(n).ok())
        .ok_or(PrecisionError::Interchange("mantissa length"))?;
    let top = word_index
        .checked_mul(WORD_BITS)
        .and_then(|n| {
            n.checked_add(
                WORD_BITS
                    .saturating_sub(usize::try_from(word.leading_zeros()).ok()?)
                    .saturating_sub(1),
            )
        })
        .and_then(|n| i64::try_from(n).ok())
        .ok_or(PrecisionError::Interchange("mantissa bit"))?;
    let unit = i64::from(exponent)
        .checked_sub(total)
        .ok_or(PrecisionError::Interchange("binary scale"))?;
    let high = unit
        .checked_add(top)
        .ok_or(PrecisionError::Interchange("binary scale"))?;
    if high > 1023 {
        return Ok(overflow(negative, rounding));
    }
    let quantum = high.saturating_sub(52).max(-1074);
    let shift = quantum
        .checked_sub(unit)
        .ok_or(PrecisionError::Interchange("binary scale"))?;
    let mut significand = 0_u64;
    for i in 0..53_u32 {
        if bit(words, shift.saturating_add(i64::from(i))) {
            significand |= 1_u64 << i;
        }
    }
    let remainder = any_below(words, shift);
    let increment = match rounding {
        BinaryRounding::Up => remainder && !negative,
        BinaryRounding::Down => remainder && negative,
        BinaryRounding::Nearest => {
            bit(words, shift.saturating_sub(1))
                && (any_below(words, shift.saturating_sub(1)) || significand & 1 != 0)
        }
    };
    if increment {
        significand = significand
            .checked_add(1)
            .ok_or(PrecisionError::Interchange("binary rounding"))?;
    }
    if significand == 0 {
        return Ok(f64::from_bits(sign_bit));
    }
    let top = 63_u32.saturating_sub(significand.leading_zeros());
    let high = quantum.saturating_add(i64::from(top));
    if high > 1023 {
        return Ok(overflow(negative, rounding));
    }
    if high < -1022 {
        return Ok(f64::from_bits(sign_bit | significand));
    }
    let normalized = if top > 52 {
        significand >> top.saturating_sub(52)
    } else {
        significand << 52_u32.saturating_sub(top)
    };
    let biased = u64::try_from(high.saturating_add(1023))
        .map_err(|_| PrecisionError::Interchange("binary exponent"))?;
    Ok(f64::from_bits(
        sign_bit | (biased << 52) | (normalized & ((1_u64 << 52) - 1)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use astro_float::RoundingMode;
    use googletest::prelude::*;
    #[gtest]
    fn finite_binary64_import_roundtrips_exact_bits_in_every_rounding_mode()
    -> googletest::Result<()> {
        for bits in [
            0,
            1,
            2,
            3,
            (1_u64 << 52) - 1,
            1_u64 << 52,
            0x3ff0_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
        ] {
            for sign in [0, 1_u64 << 63] {
                let value = exact_from_f64(f64::from_bits(bits | sign), 256)?;
                for rounding in [
                    BinaryRounding::Down,
                    BinaryRounding::Nearest,
                    BinaryRounding::Up,
                ] {
                    expect_eq!(to_f64(&value, rounding)?.to_bits(), bits | sign);
                }
            }
        }
        Ok(())
    }
    #[gtest]
    fn half_subnormal_and_midpoint_round_in_the_requested_direction() -> googletest::Result<()> {
        let tiny = exact_from_f64(f64::from_bits(1), 256)?;
        let half = checked(tiny.div(&BigFloat::from_u32(2, 256), 256, RoundingMode::ToEven))?;
        expect_eq!(to_f64(&half, BinaryRounding::Down)?.to_bits(), 0);
        expect_eq!(to_f64(&half, BinaryRounding::Nearest)?.to_bits(), 0);
        expect_eq!(to_f64(&half, BinaryRounding::Up)?.to_bits(), 1);
        let one = exact_from_f64(1.0, 256)?;
        let next = exact_from_f64(1.0_f64.next_up(), 256)?;
        let midpoint = checked(one.add(&next, 256, RoundingMode::ToEven).div(
            &BigFloat::from_u32(2, 256),
            256,
            RoundingMode::ToEven,
        ))?;
        expect_eq!(
            to_f64(&midpoint, BinaryRounding::Down)?.to_bits(),
            1.0_f64.to_bits()
        );
        expect_eq!(
            to_f64(&midpoint, BinaryRounding::Nearest)?.to_bits(),
            1.0_f64.to_bits()
        );
        expect_eq!(
            to_f64(&midpoint, BinaryRounding::Up)?.to_bits(),
            1.0_f64.next_up().to_bits()
        );
        Ok(())
    }
}
