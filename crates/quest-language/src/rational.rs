//! Exact rational storage and certified binary64 conversion using native Dashu values.
use dashu_base::{BitTest, Signed, UnsignedAbs};
use dashu_int::IBig;

pub use dashu_ratio::RBig;

/// Decode finite binary64 bits as an exact rational. Signed zero is kept by
/// the caller's source identity because a rational has only one zero.
#[must_use]
pub fn dyadic_from_bits(bits: u64) -> Option<RBig> {
	let parts = mathcore::dyadic::parts(f64::from_bits(bits))?;
	let mantissa = parts.mantissa();
	if mantissa == 0 {
		return Some(RBig::ZERO);
	}
	let shift = parts.exponent();
	let mut numerator = IBig::from(mantissa);
	let mut denominator = IBig::from(1);
	if shift >= 0 {
		numerator = std::ops::Shl::shl(numerator, usize::try_from(shift).ok()?);
	} else {
		denominator = std::ops::Shl::shl(denominator, usize::try_from(shift.unsigned_abs()).ok()?);
	}
	if parts.negative() {
		numerator = std::ops::Neg::neg(numerator);
	}
	Some(RBig::from_parts_signed(numerator, denominator))
}

/// Correctly rounded binary64 conversion without intermediate integer floats.
#[must_use]
pub fn to_f64(value: &RBig) -> Option<f64> {
	if value.numerator().is_zero() {
		return Some(0.0);
	}
	let negative = value.numerator().is_negative();
	let numerator = &value.numerator().unsigned_abs();
	let denominator = value.denominator();
	let mut exponent = i128::try_from(numerator.bit_len())
		.ok()?
		.checked_sub(i128::try_from(denominator.bit_len()).ok()?)?;
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
	let mut mantissa = u64::try_from(quotient).ok()?;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PiConversionError {
	#[error("nonfinite exact angle")]
	NonFinite,
	#[error("exact angle rounding remains unresolved")]
	Precision,
}

/// Convert against mathematical pi, admitting only a unique rounded interval result.
///
/// Machin bounds are cached at increasing precisions. A rational input can lie
/// arbitrarily close to a binary64 midpoint, so unresolved cases fail at a fixed
/// 4096-bit precision cap instead of silently selecting a neighboring float.
/// # Errors
/// Reports nonfinite conversion, unresolved rounding, or exhausted precision bounds.
pub fn to_pi_f64(value: &RBig) -> Result<f64, PiConversionError> {
	static BOUNDS: [std::sync::OnceLock<Result<PiBounds, PiConversionError>>; 6] =
		[const { std::sync::OnceLock::new() }; 6];
	if value.numerator().is_zero() {
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

/// Convert exact `r+s*pi` by enclosing the combined sum before one rounding.
/// Neither term is independently rounded or required to be finite.
/// # Errors
/// Reports nonfinite conversion, unresolved rounding, or exhausted precision bounds.
pub fn to_affine_f64(radians: &RBig, pi: &RBig) -> Result<f64, PiConversionError> {
	static BOUNDS: [std::sync::OnceLock<Result<PiBounds, PiConversionError>>; 6] =
		[const { std::sync::OnceLock::new() }; 6];
	if pi.numerator().is_zero() {
		return to_f64(radians).ok_or(PiConversionError::NonFinite);
	}
	if radians.numerator().is_zero() {
		return to_pi_f64(pi);
	}
	let maximum = dyadic_from_bits(f64::MAX.to_bits()).ok_or(PiConversionError::Precision)?;
	for (bits, cache) in [128, 256, 512, 1024, 2048, 4096].into_iter().zip(&BOUNDS) {
		let bounds = cache
			.get_or_init(|| pi_bounds(bits))
			.as_ref()
			.map_err(|error| *error)?;
		let a = std::ops::Add::add(radians, &std::ops::Mul::mul(pi, &bounds.lower));
		let b = std::ops::Add::add(radians, &std::ops::Mul::mul(pi, &bounds.upper));
		let (lower, upper) = if a <= b { (a, b) } else { (b, a) };
		if lower > maximum || upper < std::ops::Neg::neg(&maximum) {
			return Err(PiConversionError::NonFinite);
		}
		if let (Some(left), Some(right)) = (to_f64(&lower), to_f64(&upper))
			&& left.to_bits() == right.to_bits()
		{
			return Ok(left);
		}
	}
	Err(PiConversionError::Precision)
}
struct PiBounds {
	lower: RBig,
	upper: RBig,
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
	let scale = std::ops::Shl::shl(IBig::from(1), bits);
	Ok(PiBounds {
		lower: RBig::from_parts_signed(lower, scale.clone()),
		upper: RBig::from_parts_signed(upper, scale),
	})
}
fn arctangent_bounds(reciprocal: u16, bits: usize) -> Result<(IBig, IBig), PiConversionError> {
	let scale = std::ops::Shl::shl(IBig::from(1), bits);
	let square = std::ops::Mul::mul(IBig::from(reciprocal), IBig::from(reciprocal));
	let mut power = IBig::from(reciprocal);
	let mut sum = IBig::ZERO;
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
			let error = IBig::from(index.checked_add(1).ok_or(PiConversionError::Precision)?);
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
	use super::{RBig, pi_bounds, to_affine_f64, to_f64, to_pi_f64};
	use dashu_int::IBig;
	use googletest::prelude::*;

	#[gtest]
	fn direct_affine_conversion_admits_finite_cancellation_of_overflowing_terms()
	-> googletest::Result<()> {
		let bounds = pi_bounds(1024)?;
		let midpoint = std::ops::Div::div(
			std::ops::Add::add(&bounds.lower, &bounds.upper),
			IBig::from(2),
		);
		let pi_coefficient = RBig::from(std::ops::Shl::shl(IBig::from(1), 1100usize));
		let radians = std::ops::Neg::neg(std::ops::Mul::mul(&pi_coefficient, &midpoint));
		expect_true!(to_f64(&radians).is_none());
		expect_true!(to_pi_f64(&pi_coefficient).is_err());
		expect_true!(to_affine_f64(&radians, &pi_coefficient)?.is_finite());
		let direct = crate::Angle::affine(radians, pi_coefficient)?;
		let mut builder = crate::quantum::QuantumRegionBuilder::new(1, 0)?;
		builder.gate(crate::quantum::Gate::Rz(direct), &[builder.qubit(0)?], &[])?;
		expect_true!(builder.finish()?.bind(&[]).is_ok());
		Ok(())
	}

	#[gtest]
	fn rational_conversion_rounds_ties_and_subnormals_once() {
		let power = |shift| std::ops::Shl::shl(IBig::from(1), shift);
		let half_ulp =
			RBig::from_parts_signed(std::ops::Add::add(power(53usize), 1), power(53usize));
		expect_eq!(to_f64(&half_ulp), Some(1.0));
		let higher_tie =
			RBig::from_parts_signed(std::ops::Add::add(power(53usize), 3), power(53usize));
		expect_eq!(
			to_f64(&higher_tie),
			Some(f64::from_bits(0x3ff0_0000_0000_0002))
		);
		expect_eq!(
			to_f64(&RBig::from_parts_signed(IBig::from(1), power(1074usize))),
			Some(f64::from_bits(1))
		);
		expect_eq!(
			to_f64(&RBig::from_parts_signed(IBig::from(1), power(1075usize))),
			Some(0.0)
		);
		expect_eq!(
			to_f64(&RBig::from_parts_signed(IBig::from(-3), power(1075usize))),
			Some(f64::from_bits(0x8000_0000_0000_0002))
		);
		expect_true!(to_f64(&RBig::from(power(1024usize))).is_none());
	}
	#[gtest]
	fn rational_pi_keeps_subnormals_until_the_final_rounding() {
		let denominator = std::ops::Shl::shl(IBig::from(1), 1075usize);
		let tiny = RBig::from_parts_signed(IBig::from(1), denominator);
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
		let denominator = std::ops::Shl::shl(IBig::from(1), 128usize);
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
			let ratio = RBig::from_parts_signed(IBig::from(numerator), denominator.clone());
			expect_eq!(to_pi_f64(&ratio).map(f64::to_bits), Ok(bits));
		}
	}
	#[gtest]
	fn rational_pi_rejects_uncertainty_beyond_the_precision_cap() -> googletest::Result<()> {
		let fine = super::pi_bounds(8192)
			.map_err(|_| std::io::Error::other("reference bounds unavailable"))?;
		let approximate_pi =
			std::ops::Div::div(std::ops::Add::add(fine.lower, fine.upper), IBig::from(2));
		let denominator = std::ops::Shl::shl(IBig::from(1), 53usize);
		let midpoint = RBig::from_parts_signed(std::ops::Add::add(&denominator, 1), denominator);
		let ratio = std::ops::Div::div(midpoint, approximate_pi);
		expect_eq!(to_pi_f64(&ratio), Err(super::PiConversionError::Precision));
		Ok(())
	}
}
