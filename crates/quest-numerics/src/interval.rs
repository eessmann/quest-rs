use crate::{Error, Result};

/// Nonempty bounded real interval with checked function domains.
///
/// Endpoints enclose the mathematical result under maryada 0.2.1's directed
/// binary64 arithmetic contract. Basic arithmetic and square root are tight;
/// exp/log are accurate enclosures and trigonometric results are valid but can
/// widen to [-1, 1] for difficult range reduction. No global rounding mode is
/// changed and no system-libm-nextafter assumption is introduced here.
#[derive(Clone, Copy, Debug)]
pub struct Interval(maryada::Interval);

impl Interval {
    /// Construct finite ordered endpoints.
    ///
    /// # Errors
    /// Rejects NaN, infinities, and reversed bounds.
    pub fn new(lower: f64, upper: f64) -> Result<Self> {
        if !lower.is_finite() || !upper.is_finite() || lower > upper {
            return Err(Error::Interval);
        }
        Ok(Self(maryada::Interval::nums_to_interval(
            lower,
            upper,
            &mut (),
        )))
    }

    /// Enclose one exact binary64 value.
    ///
    /// # Errors
    /// Rejects nonfinite values.
    pub fn point(value: f64) -> Result<Self> {
        Self::new(value, value)
    }

    /// Lower endpoint.
    #[must_use]
    pub fn lower(self) -> f64 {
        maryada::inf(self.0)
    }

    /// Upper endpoint.
    #[must_use]
    pub fn upper(self) -> f64 {
        maryada::sup(self.0)
    }

    /// Whether the value belongs to this closed interval.
    #[must_use]
    pub fn contains(self, value: f64) -> bool {
        self.lower() <= value && value <= self.upper()
    }

    fn checked(value: maryada::Interval) -> Result<Self> {
        Self::new(maryada::inf(value), maryada::sup(value))
    }

    /// Enclose a sum.
    ///
    /// # Errors
    /// Rejects a result with unbounded endpoints.
    pub fn checked_add(self, rhs: Self) -> Result<Self> {
        Self::checked(maryada::add(self.0, rhs.0))
    }

    /// Enclose a difference.
    ///
    /// # Errors
    /// Rejects a result with unbounded endpoints.
    pub fn checked_sub(self, rhs: Self) -> Result<Self> {
        Self::checked(maryada::sub(self.0, rhs.0))
    }

    /// Enclose a product.
    ///
    /// # Errors
    /// Rejects a result with unbounded endpoints.
    pub fn checked_mul(self, rhs: Self) -> Result<Self> {
        Self::checked(maryada::mul(self.0, rhs.0))
    }

    /// Enclose a quotient.
    ///
    /// # Errors
    /// Rejects a denominator containing zero, or an unbounded result.
    pub fn checked_div(self, rhs: Self) -> Result<Self> {
        if rhs.contains(0.0) {
            return Err(Error::Domain("division"));
        }
        Self::checked(maryada::div(self.0, rhs.0))
    }

    /// Reverse endpoint signs.
    ///
    /// # Errors
    /// The finite interval invariant ensures negation cannot fail.
    pub fn checked_neg(self) -> Result<Self> {
        Self::checked(maryada::neg(self.0))
    }

    /// Enclose the square, including an exact zero lower bound across zero.
    ///
    /// # Errors
    /// Rejects a result with unbounded endpoints.
    pub fn square(self) -> Result<Self> {
        Self::checked(maryada::sqr(self.0))
    }

    /// Enclose square roots throughout a nonnegative interval.
    ///
    /// # Errors
    /// Rejects any negative input endpoint.
    pub fn sqrt(self) -> Result<Self> {
        if self.lower() < 0.0 {
            return Err(Error::Domain("sqrt"));
        }
        Self::checked(maryada::sqrt(self.0))
    }

    /// Enclose the exponential.
    ///
    /// # Errors
    /// Rejects an unbounded result.
    pub fn exp(self) -> Result<Self> {
        Self::checked(maryada::exp(self.0))
    }

    /// Enclose the natural logarithm throughout a positive interval.
    ///
    /// # Errors
    /// Rejects any nonpositive endpoint.
    pub fn ln(self) -> Result<Self> {
        if self.lower() <= 0.0 {
            return Err(Error::Domain("ln"));
        }
        Self::checked(maryada::log(self.0))
    }

    /// Enclose sine, including any interior extrema.
    ///
    /// # Errors
    /// Rejects an invalid backend result.
    pub fn sin(self) -> Result<Self> {
        Self::checked(maryada::sin(self.0))
    }

    /// Enclose cosine, including any interior extrema.
    ///
    /// # Errors
    /// Rejects an invalid backend result.
    pub fn cos(self) -> Result<Self> {
        Self::checked(maryada::cos(self.0))
    }
}

impl std::ops::Add for Interval {
    type Output = Result<Self>;
    fn add(self, rhs: Self) -> Self::Output {
        Self::checked_add(self, rhs)
    }
}

impl std::ops::Sub for Interval {
    type Output = Result<Self>;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::checked_sub(self, rhs)
    }
}

impl std::ops::Mul for Interval {
    type Output = Result<Self>;
    fn mul(self, rhs: Self) -> Self::Output {
        Self::checked_mul(self, rhs)
    }
}

impl std::ops::Div for Interval {
    type Output = Result<Self>;
    fn div(self, rhs: Self) -> Self::Output {
        Self::checked_div(self, rhs)
    }
}

impl std::ops::Neg for Interval {
    type Output = Result<Self>;
    fn neg(self) -> Self::Output {
        Self::checked_neg(self)
    }
}
