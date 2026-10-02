//! Outward arbitrary-precision scalar and complex rectangles for cold verification.
#![expect(
    clippy::arithmetic_side_effects,
    reason = "Directed native binary arithmetic operates on validated finite endpoints with admitted precision"
)]
use super::{CertificationError as Error, CertificationResult as Result};
use crate::precision::{
    Binary, abs, checked, down_add, down_cos, down_div, down_mul, down_pi, down_sin, down_sin_cos,
    down_sqrt, down_sub, exact_from_f64, integer, native_precision, up_add, up_cos, up_div, up_mul,
    up_pi, up_sin, up_sin_cos, up_sqrt, up_sub, zero,
};
use dashu_float::ConstCache;

/// Closed real interval with finite dyadic endpoints and a fixed working precision.
#[derive(Clone, Debug)]
pub struct MpInterval {
    lower: Binary,
    upper: Binary,
    precision: u32,
}
impl MpInterval {
    pub(crate) fn bounds(lower: Binary, upper: Binary, precision: u32) -> Result<Self> {
        let lower = checked(lower)?;
        let upper = checked(upper)?;
        let admitted_digits = native_precision(precision)
            .checked_add(1)
            .ok_or(Error::Budget("interval guard-bit storage"))?;
        if lower.digits() > admitted_digits || upper.digits() > admitted_digits {
            return Err(Error::Budget("interval guard-bit storage"));
        }
        if lower > upper || !(64..=crate::precision::MAX_PRECISION).contains(&precision) {
            return Err(Error::Arithmetic("invalid multiprecision interval"));
        }
        Ok(Self {
            lower,
            upper,
            precision,
        })
    }
    pub(crate) fn exact(value: f64, precision: u32) -> Result<Self> {
        let lower = exact_from_f64(value, precision)?;
        Self::bounds(lower.clone(), lower, precision)
    }
    pub(crate) fn integer(value: i32, precision: u32) -> Self {
        let lower = integer(precision, value);
        Self {
            upper: lower.clone(),
            lower,
            precision,
        }
    }
    /// Exact represented lower dyadic endpoint.
    #[must_use]
    pub const fn lower(&self) -> &Binary {
        &self.lower
    }
    /// Exact represented upper dyadic endpoint.
    #[must_use]
    pub const fn upper(&self) -> &Binary {
        &self.upper
    }
    /// Whether the exact binary64 value is enclosed; nonfinite inputs return false.
    #[must_use]
    pub fn contains_f64(&self, value: f64) -> bool {
        exact_from_f64(value, self.precision)
            .is_ok_and(|point| self.lower <= point && self.upper >= point)
    }
    pub(crate) const fn precision(&self) -> u32 {
        self.precision
    }
    pub(crate) fn is_zero(&self) -> bool {
        self.lower == Binary::ZERO && self.upper == Binary::ZERO
    }
    // Validate before comparisons or zero shortcuts can erase nonfinite input.
    fn validate(&self) -> Result<()> {
        if self.lower.repr().is_infinite() || self.upper.repr().is_infinite() {
            return Err(crate::precision::PrecisionError::Nonfinite.into());
        }
        Ok(())
    }
    pub(crate) fn add(&self, rhs: &Self) -> Result<Self> {
        self.validate()?;
        rhs.validate()?;
        let p = self.precision();
        Self::bounds(
            down_add(p, &self.lower, &rhs.lower)?,
            up_add(p, &self.upper, &rhs.upper)?,
            self.precision,
        )
    }
    pub(crate) fn sub(&self, rhs: &Self) -> Result<Self> {
        self.validate()?;
        rhs.validate()?;
        let p = self.precision();
        Self::bounds(
            down_sub(p, &self.lower, &rhs.upper)?,
            up_sub(p, &self.upper, &rhs.lower)?,
            self.precision,
        )
    }
    pub(crate) fn neg(&self) -> Self {
        Self {
            lower: -&self.upper,
            upper: -&self.lower,
            precision: self.precision,
        }
    }
    pub(crate) fn mul(&self, rhs: &Self) -> Result<Self> {
        self.validate()?;
        rhs.validate()?;
        if self.is_zero() || rhs.is_zero() {
            return Ok(Self::integer(0, self.precision));
        }
        let p = self.precision;
        let pairs = [
            (&self.lower, &rhs.lower),
            (&self.lower, &rhs.upper),
            (&self.upper, &rhs.lower),
            (&self.upper, &rhs.upper),
        ];
        let mut lower = checked(down_mul(p, &self.lower, &rhs.lower)?)?;
        let mut upper = checked(up_mul(p, &self.lower, &rhs.lower)?)?;
        for (left, right) in pairs.into_iter().skip(1) {
            let down = checked(down_mul(p, left, right)?)?;
            let up = checked(up_mul(p, left, right)?)?;
            if down < lower {
                lower = down;
            }
            if up > upper {
                upper = up;
            }
        }
        Self::bounds(lower, upper, self.precision)
    }
    pub(crate) fn divide_usize(&self, rhs: usize) -> Result<Self> {
        self.validate()?;
        if rhs == 0 {
            return Err(Error::Arithmetic("zero interval denominator"));
        }
        let p = self.precision;
        let divisor = checked(integer(
            p,
            u64::try_from(rhs).map_err(|_| Error::Budget("integer divisor"))?,
        ))?;
        Self::bounds(
            down_div(p, &self.lower, &divisor)?,
            up_div(p, &self.upper, &divisor)?,
            self.precision,
        )
    }
    pub(crate) fn abs_bounds(&self) -> Result<Self> {
        self.validate()?;
        let left = checked(abs(&self.lower))?;
        let right = checked(abs(&self.upper))?;
        let lower = if self.contains_f64(0.0) {
            zero(self.precision)
        } else if left < right {
            left.clone()
        } else {
            right.clone()
        };
        let upper = if left > right { left } else { right };
        Self::bounds(lower, upper, self.precision)
    }
    pub(crate) fn square(&self) -> Result<Self> {
        let a = self.abs_bounds()?;
        let p = self.precision;
        Self::bounds(
            down_mul(p, &a.lower, &a.lower)?,
            up_mul(p, &a.upper, &a.upper)?,
            self.precision,
        )
    }
    pub(crate) fn sqrt(&self) -> Result<Self> {
        self.validate()?;
        if self.lower < zero(self.precision) {
            return Err(Error::Arithmetic("negative square root"));
        }
        let p = self.precision;
        Self::bounds(
            down_sqrt(p, &self.lower)?,
            up_sqrt(p, &self.upper)?,
            self.precision,
        )
    }
    pub(crate) fn sin_cos_exact(
        value: f64,
        precision: u32,
        cache: &mut ConstCache,
    ) -> Result<(Self, Self)> {
        let x = exact_from_f64(value, precision)?;
        let p = precision;
        let (sin_lower, cos_lower) = down_sin_cos(p, &x, cache)?;
        let (sin_upper, cos_upper) = up_sin_cos(p, &x, cache)?;
        Ok((
            Self::bounds(sin_lower, sin_upper, precision)?,
            Self::bounds(cos_lower, cos_upper, precision)?,
        ))
    }
    pub(crate) fn twiddle(
        index: usize,
        length: usize,
        precision: u32,
        cache: &mut ConstCache,
    ) -> Result<(Self, Self)> {
        if !length.is_power_of_two() || u32::try_from(length).is_err() {
            return Err(Error::Arithmetic("non-dyadic FFT angle"));
        }
        let k = index
            .checked_rem(length)
            .ok_or(Error::Arithmetic("zero FFT length"))?;
        // Integer reduction preserves exact roots on axes, including N=1,2,4.
        let scaled = k.checked_mul(4).ok_or(Error::Budget("FFT quadrant"))?;
        let quadrant = scaled
            .checked_div(length)
            .ok_or(Error::Arithmetic("zero FFT length"))?;
        if scaled.is_multiple_of(length) {
            let (s, c) = match quadrant {
                0 => (0, 1),
                1 => (1, 0),
                2 => (0, -1),
                _ => (-1, 0),
            };
            return Ok((Self::integer(s, precision), Self::integer(c, precision)));
        }
        let quarter = length / 4;
        let residual = k
            .checked_rem(quarter)
            .ok_or(Error::Arithmetic("zero FFT quarter"))?;
        let reflected = residual > length / 8;
        let r = if reflected {
            quarter
                .checked_sub(residual)
                .ok_or(Error::Arithmetic("FFT residual"))?
        } else {
            residual
        };
        let t = Self::integer(
            i32::try_from(r.checked_mul(2).ok_or(Error::Budget("FFT residual"))?)
                .map_err(|_| Error::Budget("FFT residual"))?,
            precision,
        )
        .divide_usize(length)?;
        let p = precision;
        let pi = Self::bounds(down_pi(p, cache)?, up_pi(p, cache)?, precision)?;
        let angle = t.mul(&pi)?;
        // The reflected angle is in [0,pi/4]. Verify its entire numerical
        // enclosure is in [0,1], where sine increases and cosine decreases.
        let zero = zero(p);
        let one = integer(p, 1);
        if angle.lower < zero || angle.upper > one {
            return Err(Error::Arithmetic("FFT monotonic enclosure"));
        }
        let mut sin = Self::bounds(
            down_sin(p, &angle.lower, cache)?,
            up_sin(p, &angle.upper, cache)?,
            precision,
        )?;
        let mut cos = Self::bounds(
            down_cos(p, &angle.upper, cache)?,
            up_cos(p, &angle.lower, cache)?,
            precision,
        )?;
        if reflected {
            std::mem::swap(&mut sin, &mut cos);
        }
        Ok(match quadrant {
            0 => (sin, cos),
            1 => (cos, sin.neg()),
            2 => (sin.neg(), cos.neg()),
            _ => (cos.neg(), sin),
        })
    }
}
/// Rectangular enclosure of one complex number.
#[derive(Clone, Debug)]
pub struct MpComplex {
    real: MpInterval,
    imaginary: MpInterval,
}
impl MpComplex {
    pub(crate) fn validate(&self) -> Result<()> {
        self.real.validate()?;
        self.imaginary.validate()
    }
    pub(crate) const fn new(real: MpInterval, imaginary: MpInterval) -> Self {
        Self { real, imaginary }
    }
    pub(crate) fn exact(value: crate::Complex64, precision: u32) -> Result<Self> {
        Ok(Self {
            real: MpInterval::exact(value.re, precision)?,
            imaginary: MpInterval::exact(value.im, precision)?,
        })
    }
    pub(crate) fn zero(precision: u32) -> Self {
        Self {
            real: MpInterval::integer(0, precision),
            imaginary: MpInterval::integer(0, precision),
        }
    }
    pub(crate) fn one(precision: u32) -> Self {
        Self {
            real: MpInterval::integer(1, precision),
            imaginary: MpInterval::integer(0, precision),
        }
    }
    /// Enclosure of the real component.
    #[must_use]
    pub const fn real(&self) -> &MpInterval {
        &self.real
    }
    /// Enclosure of the imaginary component.
    #[must_use]
    pub const fn imaginary(&self) -> &MpInterval {
        &self.imaginary
    }
    pub(crate) fn is_zero(&self) -> bool {
        self.real.is_zero() && self.imaginary.is_zero()
    }
    pub(crate) fn add(&self, rhs: &Self) -> Result<Self> {
        Ok(Self::new(
            self.real.add(&rhs.real)?,
            self.imaginary.add(&rhs.imaginary)?,
        ))
    }
    pub(crate) fn sub(&self, rhs: &Self) -> Result<Self> {
        Ok(Self::new(
            self.real.sub(&rhs.real)?,
            self.imaginary.sub(&rhs.imaginary)?,
        ))
    }
    pub(crate) fn neg(&self) -> Self {
        Self::new(self.real.neg(), self.imaginary.neg())
    }
    pub(crate) fn conj(&self) -> Self {
        Self::new(self.real.clone(), self.imaginary.neg())
    }
    pub(crate) fn mul(&self, rhs: &Self) -> Result<Self> {
        Ok(Self::new(
            self.real
                .mul(&rhs.real)?
                .sub(&self.imaginary.mul(&rhs.imaginary)?)?,
            self.real
                .mul(&rhs.imaginary)?
                .add(&self.imaginary.mul(&rhs.real)?)?,
        ))
    }
    pub(crate) fn divide_usize(&self, rhs: usize) -> Result<Self> {
        Ok(Self::new(
            self.real.divide_usize(rhs)?,
            self.imaginary.divide_usize(rhs)?,
        ))
    }
    pub(crate) fn magnitude(&self) -> Result<MpInterval> {
        self.real.square()?.add(&self.imaginary.square()?)?.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn exact_trigonometric_pairs_match_separate_directed_primitives() -> googletest::Result<()> {
        for precision in [65, 128, 256] {
            for phase in [
                0.0,
                -0.0,
                0.25,
                -0.25,
                f64::from_bits(1),
                std::f64::consts::FRAC_PI_2.next_down(),
                std::f64::consts::FRAC_PI_2.next_up(),
                1e20,
                -1e20,
                f64::MAX,
            ] {
                let point = exact_from_f64(phase, precision)?;
                let mut pair_cache = ConstCache::default();
                let (sin, cos) = MpInterval::sin_cos_exact(phase, precision, &mut pair_cache)?;
                let mut scalar_cache = ConstCache::default();
                let references = [
                    down_sin(precision, &point, &mut scalar_cache)?,
                    up_sin(precision, &point, &mut scalar_cache)?,
                    down_cos(precision, &point, &mut scalar_cache)?,
                    up_cos(precision, &point, &mut scalar_cache)?,
                ];
                for (actual, expected) in [sin.lower(), sin.upper(), cos.lower(), cos.upper()]
                    .into_iter()
                    .zip(&references)
                {
                    expect_that!(actual, eq(expected));
                    expect_eq!(actual.repr().is_neg_zero(), expected.repr().is_neg_zero());
                }
            }
        }
        Ok(())
    }

    #[gtest]
    fn stored_guard_digit_is_admitted_without_changing_its_dyadic_value() -> googletest::Result<()>
    {
        let significand = (dashu_int::IBig::ONE << 64) + dashu_int::IBig::ONE;
        let point = Binary::from_repr(
            dashu_float::Repr::new(significand, -64),
            dashu_float::Context::new(64),
        );
        let enclosure = MpInterval::bounds(point.clone(), point.clone(), 64)?;
        expect_that!(enclosure.lower(), eq(&point));
        expect_that!(enclosure.upper(), eq(&point));
        expect_that!(enclosure.lower().digits(), eq(65));
        expect_true!(enclosure.lower() > &integer(64, 1));
        Ok(())
    }

    #[gtest]
    fn nonfinite_input_is_not_erased_by_zero_or_absolute_shortcuts() {
        let failed = MpInterval {
            lower: Binary::INFINITY,
            upper: zero(64),
            precision: 64,
        };
        let zero = MpInterval::integer(0, 64);
        expect_true!(matches!(failed.mul(&zero), Err(Error::Precision(_))));
        expect_true!(matches!(zero.mul(&failed), Err(Error::Precision(_))));
        expect_true!(matches!(failed.abs_bounds(), Err(Error::Precision(_))));
        expect_true!(matches!(failed.square(), Err(Error::Precision(_))));
        expect_true!(matches!(failed.add(&zero), Err(Error::Precision(_))));
        expect_true!(matches!(failed.divide_usize(1), Err(Error::Precision(_))));
    }

    #[gtest]
    fn zero_polynomial_does_not_hide_invalid_other_convolution_operand() -> googletest::Result<()> {
        use crate::certification::{CertificationPolicy, Context, ConvolutionMethod, product};
        let invalid = MpComplex::new(
            MpInterval {
                lower: Binary::INFINITY,
                upper: zero(64),
                precision: 64,
            },
            MpInterval::integer(0, 64),
        );
        let zero = vec![MpComplex::zero(64); 17];
        for method in [ConvolutionMethod::Direct, ConvolutionMethod::IntervalFft] {
            let mut context = Context::new(
                17,
                64,
                CertificationPolicy {
                    method,
                    ..CertificationPolicy::default()
                },
            )?;
            expect_true!(matches!(
                product::convolve(&zero, std::slice::from_ref(&invalid), &mut context),
                Err(Error::Precision(_))
            ));
            expect_true!(matches!(
                product::convolve(std::slice::from_ref(&invalid), &zero, &mut context),
                Err(Error::Precision(_))
            ));
        }
        Ok(())
    }
}
