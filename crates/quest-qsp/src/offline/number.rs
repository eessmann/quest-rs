use super::{OfflineError, OfflineResult};
use crate::precision::{BinaryRounding, checked, exact_from_f64, to_f64};
use astro_float::{BigFloat, Consts, RoundingMode};
#[derive(Clone, Debug)]
pub(super) struct Number {
    pub re: BigFloat,
    pub im: BigFloat,
}
impl Number {
    pub fn zero(p: u32) -> Self {
        Self {
            re: BigFloat::new(crate::offline::number::precision_bits(p)),
            im: BigFloat::new(crate::offline::number::precision_bits(p)),
        }
    }
    pub fn one(p: u32) -> Self {
        Self {
            re: BigFloat::from_u64(1, crate::offline::number::precision_bits(p)),
            im: BigFloat::new(crate::offline::number::precision_bits(p)),
        }
    }
    pub fn exact(value: crate::Complex64, p: u32) -> OfflineResult<Self> {
        Ok(Self {
            re: exact_from_f64(value.re, p)?,
            im: exact_from_f64(value.im, p)?,
        })
    }
    pub fn real(re: BigFloat) -> Self {
        Self {
            im: BigFloat::new(bits(&re)),
            re,
        }
    }
    pub fn is_zero(&self) -> bool {
        self.re.is_zero() && self.im.is_zero()
    }
    pub fn is_finite(&self) -> bool {
        finite(&self.re) && finite(&self.im)
    }
    pub fn add(&self, rhs: &Self) -> Self {
        Self {
            re: add(&self.re, &rhs.re),
            im: add(&self.im, &rhs.im),
        }
    }
    pub fn sub(&self, rhs: &Self) -> Self {
        Self {
            re: sub(&self.re, &rhs.re),
            im: sub(&self.im, &rhs.im),
        }
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
    pub fn mul(&self, rhs: &Self) -> Self {
        Self {
            re: sub(&mul(&self.re, &rhs.re), &mul(&self.im, &rhs.im)),
            im: add(&mul(&self.re, &rhs.im), &mul(&self.im, &rhs.re)),
        }
    }
    pub fn scale(&self, scalar: &BigFloat) -> Self {
        Self {
            re: mul(&self.re, scalar),
            im: mul(&self.im, scalar),
        }
    }
    pub fn div(&self, rhs: &Self) -> Self {
        let denominator = add(&mul(&rhs.re, &rhs.re), &mul(&rhs.im, &rhs.im));
        let numerator = self.mul(&rhs.conj());
        Self {
            re: div(&numerator.re, &denominator),
            im: div(&numerator.im, &denominator),
        }
    }
    pub fn abs(&self) -> BigFloat {
        sqrt(&add(&mul(&self.re, &self.re), &mul(&self.im, &self.im)))
    }
    pub fn exp(&self, constants: &mut Consts) -> OfflineResult<Self> {
        let radius = checked(self.re.exp(bits(&self.re), RoundingMode::ToEven, constants))?;
        let cos = checked(self.im.cos(bits(&self.im), RoundingMode::ToEven, constants))?;
        let sin = checked(self.im.sin(bits(&self.im), RoundingMode::ToEven, constants))?;
        let result = Self {
            re: mul(&radius, &cos),
            im: mul(&radius, &sin),
        };
        result.validate()?;
        Ok(result)
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
// Each represented offline value is an exact dyadic operand for the next
// rounded operation. NaN/backend errors propagate until the checked boundary.
pub(super) fn represented(mut value: BigFloat) -> BigFloat {
    value.set_inexact(false);
    value
}
pub(super) fn bits(value: &BigFloat) -> usize {
    value.mantissa_max_bit_len().unwrap_or(64).max(64)
}
pub(super) fn finite(value: &BigFloat) -> bool {
    !value.is_nan() && !value.is_inf()
}
pub(super) fn validate(value: &BigFloat) -> OfflineResult<()> {
    if finite(value) {
        Ok(())
    } else {
        checked(value.clone())
            .map(|_| ())
            .map_err(OfflineError::from)
    }
}
pub(super) fn add(a: &BigFloat, b: &BigFloat) -> BigFloat {
    represented(a.add(b, bits(a).max(bits(b)), RoundingMode::ToEven))
}
pub(super) fn sub(a: &BigFloat, b: &BigFloat) -> BigFloat {
    represented(a.sub(b, bits(a).max(bits(b)), RoundingMode::ToEven))
}
pub(super) fn mul(a: &BigFloat, b: &BigFloat) -> BigFloat {
    represented(a.mul(b, bits(a).max(bits(b)), RoundingMode::ToEven))
}
pub(super) fn div(a: &BigFloat, b: &BigFloat) -> BigFloat {
    represented(a.div(b, bits(a).max(bits(b)), RoundingMode::ToEven))
}
pub(super) fn sqrt(a: &BigFloat) -> BigFloat {
    represented(a.sqrt(bits(a), RoundingMode::ToEven))
}
pub(super) fn neg(a: &BigFloat) -> BigFloat {
    represented(a.neg())
}

pub(super) fn negative(value: &BigFloat) -> bool {
    value.is_negative() && !value.is_zero()
}
pub(super) fn positive(value: &BigFloat) -> bool {
    value.is_positive() && !value.is_zero()
}

#[expect(
    clippy::as_conversions,
    reason = "Context admission checks that configured u32 precision fits usize before any numerical kernel is constructed"
)]
pub(super) const fn precision_bits(p: u32) -> usize {
    p as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn exact_subnormal_components_and_zero_arithmetic_keep_configured_precision()
    -> googletest::Result<()> {
        let value = Number::exact(
            crate::Complex64::new(f64::from_bits(1), -f64::from_bits(1)),
            256,
        )?;
        let result = value.add(&Number::zero(256));
        expect_that!(bits(&result.re), eq(256));
        let exported = result.binary64()?;
        expect_that!(exported.re.to_bits(), eq(1));
        expect_that!(exported.im.to_bits(), eq((-f64::from_bits(1)).to_bits()));
        Ok(())
    }

    #[gtest]
    fn invalid_arbitrary_arithmetic_reaches_typed_boundary_errors() {
        let invalid = Number::one(128).div(&Number::zero(128));
        expect_true!(matches!(
            invalid.validate(),
            Err(OfflineError::Precision(_))
        ));
        let invalid_root = sqrt(&BigFloat::from_i64(-1, 128));
        expect_true!(matches!(
            validate(&invalid_root),
            Err(OfflineError::Precision(_))
        ));
        let backend_failure = BigFloat::nan(Some(astro_float::Error::InvalidArgument));
        expect_true!(matches!(
            validate(&backend_failure),
            Err(OfflineError::Precision(
                crate::precision::PrecisionError::Backend(_)
            ))
        ));
    }
}
