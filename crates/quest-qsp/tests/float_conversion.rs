#![cfg(feature = "certification")]
#![expect(
	clippy::arithmetic_side_effects,
	reason = "Independent bounded integer and native binary dyadics define exact interchange test fixtures"
)]

use dashu_float::{
	Context, Repr,
	round::mode::{Down, HalfEven},
};
use dashu_int::IBig;
use googletest::prelude::*;
use quest_qsp::precision::Binary;
use quest_qsp::precision::{BinaryRounding, PrecisionError, checked, exact_from_f64, to_f64};

const SIGN: u64 = 1_u64 << 63;
const MAX: u64 = 0x7fef_ffff_ffff_ffff;

// Independent integer fixture: numerator * 2^scale, constructed without either
// QSP interchange function or a preliminary binary64 conversion.
fn dyadic(numerator: u128, scale: isize, negative: bool, width: u32) -> Result<Binary> {
	let value = if numerator == 0 && negative {
		Binary::from_repr(Repr::neg_zero(), Context::new(usize::try_from(width)?))
	} else {
		let n = IBig::from(numerator);
		Binary::from_parts(if negative { -n } else { n }, scale)
			.with_precision(usize::try_from(width)?)
			.value()
	};
	assert_that!(value.repr().is_infinite(), eq(false));
	Ok(value)
}

fn components(bits: u64) -> Result<(u128, isize)> {
	let exponent = isize::try_from((bits >> 52) & 0x7ff)?;
	let fraction = u128::from(bits & 0x000f_ffff_ffff_ffff);
	Ok(if exponent == 0 {
		(fraction, -1074)
	} else {
		(
			fraction | (1_u128 << 52),
			exponent
				.checked_sub(1075)
				.ok_or_else(|| std::io::Error::other("scale"))?,
		)
	})
}

fn seeded_bits() -> impl Iterator<Item = u64> {
	let mut state = 0x41c6_4e6d_c0ff_ee01_u64;
	(0..2048).map(move |_| {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		state
	})
}

#[gtest]
fn seeded_imports_equal_independent_integer_dyadics_at_multiple_precisions() -> Result<()> {
	for bits in seeded_bits().filter(|bits| (bits >> 52) & 0x7ff != 0x7ff) {
		let (numerator, scale) = components(bits)?;
		let expected = dyadic(numerator, scale, bits & SIGN != 0, 256)?;
		for precision in [53, 54, 63, 64, 65, 127, 128, 256] {
			let actual = exact_from_f64(f64::from_bits(bits), precision).map_err(|error| {
				std::io::Error::other(format!("bits={bits:#018x} precision={precision}: {error}"))
			})?;
			assert_that!(&actual, eq(&expected));
			for rounding in [
				BinaryRounding::Down,
				BinaryRounding::Nearest,
				BinaryRounding::Up,
			] {
				assert_that!(to_f64(&actual, rounding)?.to_bits(), eq(bits));
				assert_that!(to_f64(&expected, rounding)?.to_bits(), eq(bits));
			}
		}
	}
	Ok(())
}

#[gtest]
fn midpoint_neighbors_round_with_sign_parity_and_gradual_underflow() -> Result<()> {
	let boundaries = [
		0,
		1,
		2,
		3,
		0x000f_ffff_ffff_fffe,
		0x000f_ffff_ffff_ffff,
		0x0010_0000_0000_0000,
		0x3fef_ffff_ffff_ffff,
		0x3ff0_0000_0000_0000,
		0x3ff0_0000_0000_0001,
		MAX.checked_sub(1)
			.ok_or_else(|| std::io::Error::other("max predecessor"))?,
		MAX,
	];
	for low in boundaries
		.into_iter()
		.chain(seeded_bits().take(1024).map(|bits| bits % MAX))
	{
		let high = low
			.checked_add(1)
			.ok_or_else(|| std::io::Error::other("successor"))?;
		let (numerator, scale) = components(low)?;
		let midpoint = numerator
			.checked_mul(2)
			.and_then(|n| n.checked_add(1))
			.and_then(|n| n.checked_shl(60))
			.ok_or_else(|| std::io::Error::other("midpoint numerator"))?;
		let scale = scale
			.checked_sub(61)
			.ok_or_else(|| std::io::Error::other("midpoint scale"))?;
		for side in [-1_i32, 0, 1] {
			let numerator = match side {
				-1 => midpoint.checked_sub(1),
				0 => Some(midpoint),
				_ => midpoint.checked_add(1),
			}
			.ok_or_else(|| std::io::Error::other("midpoint perturbation"))?;
			let nearest = if side < 0 || (side == 0 && low & 1 == 0) {
				low
			} else {
				high
			};
			for negative in [false, true] {
				let sign = if negative { SIGN } else { 0 };
				let value = dyadic(numerator, scale, negative, 128)?;
				let down = if negative { high } else { low };
				let up = if negative { low } else { high };
				assert_that!(
					to_f64(&value, BinaryRounding::Down)?.to_bits(),
					eq(down | sign)
				);
				assert_that!(to_f64(&value, BinaryRounding::Up)?.to_bits(), eq(up | sign));
				assert_that!(
					to_f64(&value, BinaryRounding::Nearest)?.to_bits(),
					eq(nearest | sign)
				);
			}
		}
	}
	Ok(())
}

#[gtest]
fn sticky_bits_cross_multiple_limb_boundaries() -> Result<()> {
	// 1 + 2^-53 + 2^-200 is strictly above the even midpoint at one.
	for width in [256, 512] {
		for negative in [false, true] {
			let numerator = (IBig::from(1) << 200) + (IBig::from(1) << 147) + 1;
			let mut value = Binary::from_parts(numerator, -200)
				.with_precision(usize::try_from(width)?)
				.value();
			if negative {
				value = -value;
			}
			let expected = 0x3ff0_0000_0000_0001 | if negative { SIGN } else { 0 };
			assert_that!(
				to_f64(&value, BinaryRounding::Nearest)?.to_bits(),
				eq(expected)
			);
		}
	}
	Ok(())
}

#[gtest]
fn backend_exponent_extremes_and_signed_zero_keep_directional_semantics() -> Result<()> {
	for negative in [false, true] {
		let sign_bit = if negative { SIGN } else { 0 };
		let tiny = dyadic(1, isize::MIN + 2, negative, 128)?;
		let huge = dyadic(1, isize::MAX - 2, negative, 128)?;
		for (mode, tiny_bits, huge_bits) in [
			(BinaryRounding::Nearest, 0, f64::INFINITY.to_bits()),
			(
				BinaryRounding::Down,
				u64::from(negative),
				if negative {
					f64::INFINITY.to_bits()
				} else {
					MAX
				},
			),
			(
				BinaryRounding::Up,
				u64::from(!negative),
				if negative {
					MAX
				} else {
					f64::INFINITY.to_bits()
				},
			),
		] {
			assert_that!(to_f64(&tiny, mode)?.to_bits(), eq(sign_bit | tiny_bits));
			assert_that!(to_f64(&huge, mode)?.to_bits(), eq(sign_bit | huge_bits));
			assert_that!(
				to_f64(&dyadic(0, 0, negative, 128)?, mode)?.to_bits(),
				eq(sign_bit)
			);
		}
	}
	Ok(())
}

#[gtest]
fn invalid_inputs_keep_nonfinite_and_precision_errors_distinct() {
	for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
		expect_true!(matches!(
			exact_from_f64(value, 256),
			Err(PrecisionError::Nonfinite)
		));
	}
	for precision in [0, 1, 32, 52, 1_048_577, u32::MAX] {
		expect_true!(matches!(
			exact_from_f64(1.0, precision),
			Err(PrecisionError::Interchange(_))
		));
	}
	for value in [Binary::INFINITY, Binary::NEG_INFINITY] {
		expect_true!(matches!(
			to_f64(&value, BinaryRounding::Nearest),
			Err(PrecisionError::Nonfinite)
		));
		expect_true!(matches!(checked(value), Err(PrecisionError::Nonfinite)));
	}
}

#[gtest]
fn directed_rounding_result_remains_an_exact_represented_dyadic() -> Result<()> {
	let one = Binary::from(1).with_precision(64).value();
	let three = Binary::from(3).with_precision(64).value();
	let rounded = Context::<Down>::new(64)
		.div(one.repr(), three.repr())?
		.value()
		.with_rounding::<HalfEven>();
	assert_that!(&rounded * &three, lt(&Binary::from(1)));
	let admitted = checked(rounded.clone())?;
	assert_that!(&admitted, eq(&rounded));
	for mode in [
		BinaryRounding::Down,
		BinaryRounding::Nearest,
		BinaryRounding::Up,
	] {
		assert_that!(
			to_f64(&admitted, mode)?.to_bits(),
			eq(to_f64(&rounded, mode)?.to_bits())
		);
	}
	Ok(())
}
