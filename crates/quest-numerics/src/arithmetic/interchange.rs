#![allow(
	clippy::arithmetic_side_effects,
	reason = "Binary64 fields and grid shifts are bounded; i128 bookkeeping is wider than every native exponent and significand length"
)]
//! Checked exact dyadic interchange; no decimal or intermediate float conversion.
use super::{ArithmeticError, Binary};
use dashu_base::{BitTest, Sign, UnsignedAbs};
use dashu_float::{Context, Repr};
use dashu_int::{IBig, UBig};
use mathcore::arithmetic::ArithmeticError as CoreError;
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
type Result<T> = std::result::Result<T, ArithmeticError>;
/// Import the exact represented binary64 value, including subnormals and -0.
/// # Errors
/// Rejects nonfinite input and precision outside 53..=1,048,576 bits.
pub fn exact_from_f64(value: f64, precision: u32) -> Result<Binary> {
	if !value.is_finite() {
		return Err(ArithmeticError::Core(CoreError::Nonfinite));
	}
	if !(53..=1_048_576).contains(&precision) {
		return Err(ArithmeticError::Core(CoreError::Interchange(
			"binary64 precision must be 53..=1048576 bits",
		)));
	}
	let parts = mathcore::dyadic::parts(value).ok_or(CoreError::Nonfinite)?;
	let repr = if parts.mantissa() == 0 {
		if parts.negative() {
			Repr::neg_zero()
		} else {
			Repr::zero()
		}
	} else {
		let significand = IBig::from(parts.mantissa());
		Repr::new(
			if parts.negative() {
				-significand
			} else {
				significand
			},
			isize::try_from(parts.exponent())
				.map_err(|_| CoreError::Interchange("binary64 exponent"))?,
		)
	};
	Ok(Binary::from_repr(
		repr,
		Context::new(
			usize::try_from(precision)
				.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 precision")))?,
		),
	))
}
fn overflow(negative: bool, direction: BinaryRounding) -> f64 {
	let infinite = matches!(direction, BinaryRounding::Nearest)
		|| matches!(
			(direction, negative),
			(BinaryRounding::Up, false) | (BinaryRounding::Down, true)
		);
	let magnitude = if infinite { f64::INFINITY } else { f64::MAX };
	if negative { -magnitude } else { magnitude }
}
/// Correctly round an exact stored dyadic to binary64, with gradual underflow.
/// Infinity is a valid outward result for finite values exceeding binary64.
/// # Errors
/// Rejects nonfinite input or unrepresentable interchange bookkeeping.
pub fn to_f64(value: &Binary, direction: BinaryRounding) -> Result<f64> {
	let repr = value.repr();
	if !repr.is_finite() {
		return Err(ArithmeticError::Core(CoreError::Nonfinite));
	}
	let negative = repr.sign() == Sign::Negative;
	let sign = if negative { 1u64 << 63 } else { 0 };
	if repr.significand().is_zero() {
		return Ok(f64::from_bits(sign));
	}
	let significand = repr.significand().unsigned_abs();
	// i128 admits every isize exponent plus usize-sized bit length.
	let top = i128::try_from(significand.bit_len())
		.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 significand")))?
		+ i128::try_from(repr.exponent())
			.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 exponent")))?;
	if top > 1024 {
		return Ok(overflow(negative, direction));
	}
	if top < -1074 {
		let away = matches!(
			(direction, negative),
			(BinaryRounding::Up, false) | (BinaryRounding::Down, true)
		);
		return Ok(f64::from_bits(sign | u64::from(away)));
	}
	let quantum = (top - 53).max(-1074);
	let shift = quantum
		- i128::try_from(repr.exponent())
			.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 exponent")))?;
	let (mut quotient, remainder, halfway) = if shift > 0 {
		let shift = usize::try_from(shift)
			.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 shift")))?;
		let quotient = &significand >> shift;
		let remainder = &significand - (&quotient << shift);
		(quotient, remainder, UBig::ONE << shift.saturating_sub(1))
	} else {
		let shift = usize::try_from(-shift)
			.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 shift")))?;
		(significand << shift, UBig::ZERO, UBig::ZERO)
	};
	let increment = if remainder == UBig::ZERO {
		false
	} else {
		match direction {
			BinaryRounding::Nearest => {
				remainder > halfway || (remainder == halfway && quotient.bit(0))
			}
			BinaryRounding::Down => negative,
			BinaryRounding::Up => !negative,
		}
	};
	if increment {
		quotient += UBig::ONE;
	}
	let q = u64::try_from(quotient)
		.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 significand")))?;
	if q == 0 {
		return Ok(f64::from_bits(sign));
	}
	let length = i128::from(64 - q.leading_zeros());
	let rounded_top = quantum + length;
	if rounded_top > 1024 {
		return Ok(overflow(negative, direction));
	}
	let encoded = if rounded_top <= -1022 {
		q
	} else {
		// q is at most 54 bits; a rounding carry discards one exact trailing zero.
		let normalized = if length > 53 {
			q >> 1
		} else {
			q << u32::try_from(53 - length).map_err(|_| {
				ArithmeticError::Core(CoreError::Interchange("binary64 normalization"))
			})?
		};
		let exponent = u64::try_from(rounded_top + 1022)
			.map_err(|_| ArithmeticError::Core(CoreError::Interchange("binary64 exponent")))?;
		(exponent << 52) | (normalized & ((1 << 52) - 1))
	};
	Ok(f64::from_bits(sign | encoded))
}
