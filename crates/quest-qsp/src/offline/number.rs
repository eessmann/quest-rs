use super::{OfflineError, OfflineResult};
use crate::precision::{
	Binary, BinaryRounding, PrecisionError, exact_from_f64, integer, nearest_add, nearest_div,
	nearest_exp, nearest_mul, nearest_sin_cos, nearest_sqrt, nearest_sub, to_f64, zero,
};
use dashu_float::ConstCache;
#[derive(Clone, Debug)]
pub(super) struct Number {
	pub re: Binary,
	pub im: Binary,
}
impl Number {
	pub fn zero(p: u32) -> Self {
		Self {
			re: zero(p),
			im: zero(p),
		}
	}
	pub fn one(p: u32) -> Self {
		Self {
			re: integer(p, 1),
			im: zero(p),
		}
	}
	pub fn exact(value: crate::Complex64, p: u32) -> OfflineResult<Self> {
		Ok(Self {
			re: exact_from_f64(value.re, p)?,
			im: exact_from_f64(value.im, p)?,
		})
	}
	pub fn real(re: Binary) -> Self {
		Self {
			im: Binary::ZERO.with_precision(re.precision()).value(),
			re,
		}
	}
	pub fn is_zero(&self) -> bool {
		self.re == Binary::ZERO && self.im == Binary::ZERO
	}
	pub const fn is_finite(&self) -> bool {
		finite(&self.re) && finite(&self.im)
	}
	pub fn add(&self, rhs: &Self) -> OfflineResult<Self> {
		Ok(Self {
			re: add(&self.re, &rhs.re)?,
			im: add(&self.im, &rhs.im)?,
		})
	}
	pub fn sub(&self, rhs: &Self) -> OfflineResult<Self> {
		Ok(Self {
			re: sub(&self.re, &rhs.re)?,
			im: sub(&self.im, &rhs.im)?,
		})
	}
	pub fn neg(&self) -> Self {
		Self {
			re: neg(&self.re),
			im: neg(&self.im),
		}
	}
	pub fn conj(&self) -> Self {
		Self {
			re: self.re.clone(),
			im: neg(&self.im),
		}
	}
	pub fn mul(&self, rhs: &Self) -> OfflineResult<Self> {
		Ok(Self {
			re: sub(&mul(&self.re, &rhs.re)?, &mul(&self.im, &rhs.im)?)?,
			im: add(&mul(&self.re, &rhs.im)?, &mul(&self.im, &rhs.re)?)?,
		})
	}
	pub fn scale(&self, scalar: &Binary) -> OfflineResult<Self> {
		Ok(Self {
			re: mul(&self.re, scalar)?,
			im: mul(&self.im, scalar)?,
		})
	}
	pub fn div(&self, rhs: &Self) -> OfflineResult<Self> {
		let denominator = add(&mul(&rhs.re, &rhs.re)?, &mul(&rhs.im, &rhs.im)?)?;
		let numerator = self.mul(&rhs.conj())?;
		Ok(Self {
			re: div(&numerator.re, &denominator)?,
			im: div(&numerator.im, &denominator)?,
		})
	}
	pub fn abs(&self) -> OfflineResult<Binary> {
		sqrt(&add(&mul(&self.re, &self.re)?, &mul(&self.im, &self.im)?)?)
	}
	pub fn exp(&self, cache: &mut ConstCache) -> OfflineResult<Self> {
		let p = u32::try_from(self.re.precision())
			.map_err(|_| OfflineError::Budget("native precision"))?;
		let radius = nearest_exp(p, &self.re, cache)?;
		let (sin, cos) = nearest_sin_cos(p, &self.im, cache)?;
		Ok(Self {
			re: mul(&radius, &cos)?,
			im: mul(&radius, &sin)?,
		})
	}
	pub fn validate(&self) -> OfflineResult<()> {
		validate(&self.re)?;
		validate(&self.im)
	}
	pub fn binary64(&self) -> OfflineResult<crate::Complex64> {
		Ok(crate::Complex64::new(
			to_f64(&self.re, BinaryRounding::Nearest)?,
			to_f64(&self.im, BinaryRounding::Nearest)?,
		))
	}
}
pub(super) const fn finite(value: &Binary) -> bool {
	!value.repr().is_infinite()
}
pub(super) const fn validate(value: &Binary) -> OfflineResult<()> {
	if finite(value) {
		Ok(())
	} else {
		Err(OfflineError::Precision(PrecisionError::Nonfinite))
	}
}
pub(super) fn add(a: &Binary, b: &Binary) -> OfflineResult<Binary> {
	Ok(nearest_add(
		u32::try_from(a.precision().max(b.precision()))
			.map_err(|_| OfflineError::Budget("native precision"))?,
		a,
		b,
	)?)
}
pub(super) fn sub(a: &Binary, b: &Binary) -> OfflineResult<Binary> {
	Ok(nearest_sub(
		u32::try_from(a.precision().max(b.precision()))
			.map_err(|_| OfflineError::Budget("native precision"))?,
		a,
		b,
	)?)
}
pub(super) fn mul(a: &Binary, b: &Binary) -> OfflineResult<Binary> {
	Ok(nearest_mul(
		u32::try_from(a.precision().max(b.precision()))
			.map_err(|_| OfflineError::Budget("native precision"))?,
		a,
		b,
	)?)
}
pub(super) fn div(a: &Binary, b: &Binary) -> OfflineResult<Binary> {
	Ok(nearest_div(
		u32::try_from(a.precision().max(b.precision()))
			.map_err(|_| OfflineError::Budget("native precision"))?,
		a,
		b,
	)?)
}
pub(super) fn sqrt(a: &Binary) -> OfflineResult<Binary> {
	Ok(nearest_sqrt(
		u32::try_from(a.precision()).map_err(|_| OfflineError::Budget("native precision"))?,
		a,
	)?)
}
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Native represented dyadic negation is exact and cannot overflow"
)]
pub(super) fn neg(a: &Binary) -> Binary {
	-a
}
#[cfg(test)]
mod tests {
	use super::*;
	use crate::precision::{nearest_cos, nearest_sin};
	use googletest::prelude::*;
	#[gtest]
	fn complex_exponential_matches_separate_correctly_rounded_primitives() -> googletest::Result<()>
	{
		for precision in [65, 128, 256] {
			for value in [
				crate::Complex64::new(0.0, -0.0),
				crate::Complex64::new(-1.0, 0.25),
				crate::Complex64::new(0.5, -0.25),
				crate::Complex64::new(0.0, f64::from_bits(1)),
				crate::Complex64::new(1.0, 1e20),
			] {
				let point = Number::exact(value, precision)?;
				let actual = point.exp(&mut ConstCache::default())?;
				let mut scalar_cache = ConstCache::default();
				let radius = nearest_exp(precision, &point.re, &mut scalar_cache)?;
				let re = mul(
					&radius,
					&nearest_cos(precision, &point.im, &mut scalar_cache)?,
				)?;
				let im = mul(
					&radius,
					&nearest_sin(precision, &point.im, &mut scalar_cache)?,
				)?;
				expect_that!(&actual.re, eq(&re));
				expect_that!(&actual.im, eq(&im));
				expect_eq!(actual.im.repr().is_neg_zero(), im.repr().is_neg_zero());
			}
		}
		let invalid = Number {
			re: zero(128),
			im: Binary::INFINITY,
		};
		expect_true!(matches!(
			invalid.exp(&mut ConstCache::default()),
			Err(OfflineError::Precision(PrecisionError::Arithmetic(
				dashu_float::FpError::InfiniteInput
			)))
		));
		Ok(())
	}

	#[gtest]
	fn exact_subnormal_components_and_zero_arithmetic_keep_configured_precision()
	-> googletest::Result<()> {
		let value = Number::exact(
			crate::Complex64::new(f64::from_bits(1), -f64::from_bits(1)),
			256,
		)?;
		let result = value.add(&Number::zero(256))?;
		expect_that!(result.re.precision(), eq(256));
		let exported = result.binary64()?;
		expect_that!(exported.re.to_bits(), eq(1));
		expect_that!(exported.im.to_bits(), eq((-f64::from_bits(1)).to_bits()));
		Ok(())
	}
	#[gtest]
	fn invalid_arbitrary_arithmetic_reaches_typed_boundary_errors() {
		expect_true!(matches!(
			Number::one(128).div(&Number::zero(128)),
			Err(OfflineError::Precision(_))
		));
		expect_true!(matches!(
			sqrt(&integer(128, -1)),
			Err(OfflineError::Precision(PrecisionError::Arithmetic(
				dashu_float::FpError::OutOfDomain
			)))
		));
		expect_true!(matches!(
			validate(&Binary::INFINITY),
			Err(OfflineError::Precision(PrecisionError::Nonfinite))
		));
	}
}
