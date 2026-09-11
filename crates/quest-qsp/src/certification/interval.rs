//! Outward arbitrary-precision scalar and complex rectangles for cold verification.
use super::{CertificationError as Error, CertificationResult as Result};
use crate::precision::{checked, exact_from_f64};
use astro_float::{BigFloat, Consts, RoundingMode as Round};

/// Closed real interval with finite dyadic endpoints and a fixed working precision.
#[derive(Clone, Debug)]
pub struct MpInterval {
    lower: BigFloat,
    upper: BigFloat,
    precision: u32,
}
impl MpInterval {
    pub(crate) fn bounds(lower: BigFloat, upper: BigFloat, precision: u32) -> Result<Self> {
        let lower = checked(lower)?;
        let upper = checked(upper)?;
        if lower > upper || precision < 64 {
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
        let lower = BigFloat::from_i32(value, usize::try_from(precision).unwrap_or(usize::MAX));
        Self {
            upper: lower.clone(),
            lower,
            precision,
        }
    }
    /// Exact represented lower dyadic endpoint.
    #[must_use]
    pub const fn lower(&self) -> &BigFloat {
        &self.lower
    }
    /// Exact represented upper dyadic endpoint.
    #[must_use]
    pub const fn upper(&self) -> &BigFloat {
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
        self.lower.is_zero() && self.upper.is_zero()
    }
    // Infallible clones and integer construction can retain a backend error
    // sentinel. Validate before comparisons or exact-zero shortcuts erase it.
    fn validate(&self) -> Result<()> {
        for endpoint in [&self.lower, &self.upper] {
            if let Some(error) = endpoint.err() {
                return Err(crate::precision::PrecisionError::from(error).into());
            }
            if endpoint.is_nan() || endpoint.is_inf() {
                return Err(crate::precision::PrecisionError::Nonfinite.into());
            }
        }
        Ok(())
    }
    pub(crate) fn add(&self, rhs: &Self) -> Result<Self> {
        self.validate()?;
        rhs.validate()?;
        let p = usize::try_from(self.precision()).unwrap_or(usize::MAX);
        Self::bounds(
            self.lower.add(&rhs.lower, p, Round::Down),
            self.upper.add(&rhs.upper, p, Round::Up),
            self.precision,
        )
    }
    pub(crate) fn sub(&self, rhs: &Self) -> Result<Self> {
        self.validate()?;
        rhs.validate()?;
        let p = usize::try_from(self.precision()).unwrap_or(usize::MAX);
        Self::bounds(
            self.lower.sub(&rhs.upper, p, Round::Down),
            self.upper.sub(&rhs.lower, p, Round::Up),
            self.precision,
        )
    }
    pub(crate) fn neg(&self) -> Self {
        Self {
            lower: self.upper.neg(),
            upper: self.lower.neg(),
            precision: self.precision,
        }
    }
    pub(crate) fn mul(&self, rhs: &Self) -> Result<Self> {
        self.validate()?;
        rhs.validate()?;
        if self.is_zero() || rhs.is_zero() {
            return Ok(Self::integer(0, self.precision));
        }
        let p = usize::try_from(self.precision).unwrap_or(usize::MAX);
        let pairs = [
            (&self.lower, &rhs.lower),
            (&self.lower, &rhs.upper),
            (&self.upper, &rhs.lower),
            (&self.upper, &rhs.upper),
        ];
        let mut lower = checked(self.lower.mul(&rhs.lower, p, Round::Down))?;
        let mut upper = checked(self.lower.mul(&rhs.lower, p, Round::Up))?;
        for (left, right) in pairs.into_iter().skip(1) {
            let down = checked(left.mul(right, p, Round::Down))?;
            let up = checked(left.mul(right, p, Round::Up))?;
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
        let p = usize::try_from(self.precision).unwrap_or(usize::MAX);
        let divisor = checked(BigFloat::from_u64(
            u64::try_from(rhs).map_err(|_| Error::Budget("integer divisor"))?,
            p,
        ))?;
        Self::bounds(
            self.lower.div(&divisor, p, Round::Down),
            self.upper.div(&divisor, p, Round::Up),
            self.precision,
        )
    }
    pub(crate) fn abs_bounds(&self) -> Result<Self> {
        self.validate()?;
        let left = checked(self.lower.abs())?;
        let right = checked(self.upper.abs())?;
        let lower = if self.contains_f64(0.0) {
            BigFloat::from_u64(0, usize::try_from(self.precision).unwrap_or(usize::MAX))
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
        let p = usize::try_from(self.precision).unwrap_or(usize::MAX);
        Self::bounds(
            a.lower.mul(&a.lower, p, Round::Down),
            a.upper.mul(&a.upper, p, Round::Up),
            self.precision,
        )
    }
    pub(crate) fn sqrt(&self) -> Result<Self> {
        self.validate()?;
        if self.lower < BigFloat::from_u64(0, usize::try_from(self.precision).unwrap_or(usize::MAX))
        {
            return Err(Error::Arithmetic("negative square root"));
        }
        let p = usize::try_from(self.precision).unwrap_or(usize::MAX);
        Self::bounds(
            self.lower.sqrt(p, Round::Down),
            self.upper.sqrt(p, Round::Up),
            self.precision,
        )
    }
    pub(crate) fn sin_cos_exact(
        value: f64,
        precision: u32,
        cache: &mut Consts,
    ) -> Result<(Self, Self)> {
        let x = exact_from_f64(value, precision)?;
        let p = usize::try_from(precision).unwrap_or(usize::MAX);
        Ok((
            Self::bounds(
                x.sin(p, Round::Down, cache),
                x.sin(p, Round::Up, cache),
                precision,
            )?,
            Self::bounds(
                x.cos(p, Round::Down, cache),
                x.cos(p, Round::Up, cache),
                precision,
            )?,
        ))
    }
    pub(crate) fn twiddle(
        index: usize,
        length: usize,
        precision: u32,
        cache: &mut Consts,
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
        let p = usize::try_from(precision).unwrap_or(usize::MAX);
        let pi = Self::bounds(cache.pi(p, Round::Down), cache.pi(p, Round::Up), precision)?;
        let angle = t.mul(&pi)?;
        // The reflected angle is in [0,pi/4]. Verify its entire numerical
        // enclosure is in [0,1], where sine increases and cosine decreases.
        let zero = BigFloat::from_u64(0, p);
        let one = BigFloat::from_u64(1, p);
        if angle.lower < zero || angle.upper > one {
            return Err(Error::Arithmetic("FFT monotonic enclosure"));
        }
        let mut sin = Self::bounds(
            angle.lower.sin(p, Round::Down, cache),
            angle.upper.sin(p, Round::Up, cache),
            precision,
        )?;
        let mut cos = Self::bounds(
            angle.upper.cos(p, Round::Down, cache),
            angle.lower.cos(p, Round::Up, cache),
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
    fn backend_error_sentinel_is_not_erased_by_zero_or_absolute_shortcuts() {
        let failed = MpInterval {
            lower: BigFloat::nan(Some(astro_float::Error::MemoryAllocation)),
            upper: BigFloat::from_u64(0, 64),
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
                lower: BigFloat::nan(Some(astro_float::Error::MemoryAllocation)),
                upper: BigFloat::from_u64(0, 64),
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
