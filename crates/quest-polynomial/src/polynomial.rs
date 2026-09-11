use crate::{Basis, Chebyshev, Complex64, Error, Interval, Laurent, Limits, Result, finite, zeros};
use std::{
    ops::{Add, Div, Mul, Sub},
    sync::Arc,
};

/// Immutable basis coefficients with admitted signed support.
#[derive(Debug, Clone)]
pub struct Polynomial<B: Basis> {
    basis: B,
    coefficients: Arc<Vec<Complex64>>,
    limits: Limits,
    last_order: i32,
}
impl<B: Basis> Polynomial<B> {
    /// # Errors
    /// Rejects nonfinite coefficients, support overflow, or excessive storage.
    pub fn new(basis: B, coefficients: Vec<Complex64>, limits: Limits) -> Result<Self> {
        limits.check(coefficients.len(), 1)?;
        for value in &coefficients {
            finite(*value)?;
        }
        let last = i32::try_from(coefficients.len().saturating_sub(1))
            .map_err(|_| Error::SupportOverflow)?;
        let last_order = basis
            .offset()
            .checked_add(last)
            .ok_or(Error::SupportOverflow)?;
        Ok(Self {
            basis,
            coefficients: Arc::new(coefficients),
            limits,
            last_order,
        })
    }
    #[must_use]
    pub fn coefficients(&self) -> &[Complex64] {
        self.coefficients.as_slice()
    }
    #[must_use]
    pub const fn basis(&self) -> &B {
        &self.basis
    }
    #[must_use]
    pub fn support(&self) -> (i32, i32) {
        (self.basis.offset(), self.last_order)
    }
    #[must_use]
    pub fn degree(&self) -> usize {
        self.coefficients.len().saturating_sub(1)
    }
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.coefficients
            .iter()
            .all(|x| *x == Complex64::new(0.0, 0.0))
    }
    #[must_use]
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Evaluate by generalized Clenshaw followed by the explicit Laurent shift.
    /// # Errors
    /// Rejects nonfinite arithmetic and Laurent poles at zero.
    pub fn evaluate(&self, argument: Complex64) -> Result<Complex64> {
        finite(argument)?;
        if self.coefficients.is_empty() {
            return Ok(Complex64::new(0.0, 0.0));
        }
        if argument == Complex64::new(0.0, 0.0) && self.basis.offset() < 0 {
            return Err(Error::Domain);
        }
        if self.coefficients.len() == 1 {
            return finite(
                self.coefficients
                    .first()
                    .copied()
                    .ok_or(Error::Domain)?
                    .mul(power(argument, self.basis.offset())?),
            );
        }
        let mut following = Complex64::new(0.0, 0.0);
        let mut after_following = following;
        for (index, coefficient) in self.coefficients.iter().enumerate().rev() {
            let order = u32::try_from(index).map_err(|_| Error::SupportOverflow)?;
            let (a, b, _) = self
                .basis
                .recurrence(order.checked_add(1).ok_or(Error::SupportOverflow)?)?;
            let (_, _, c) = self
                .basis
                .recurrence(order.checked_add(2).ok_or(Error::SupportOverflow)?)?;
            let value = finite(
                coefficient
                    .add(argument.mul(a).add(b).mul(following))
                    .sub(after_following.mul(c)),
            )?;
            after_following = following;
            following = value;
        }
        finite(following.mul(power(argument, self.basis.offset())?))
    }

    /// # Errors
    /// Rejects complex coefficients or nonfinite/domain arithmetic.
    pub fn evaluate_real(&self, argument: f64) -> Result<f64> {
        if self.coefficients.iter().any(|value| value.im != 0.0) {
            return Err(Error::NotReal);
        }
        Ok(self.evaluate(Complex64::new(argument, 0.0))?.re)
    }
}

impl Polynomial<Chebyshev> {
    /// Substitute x=(z+1/z)/2 without a monomial conversion.
    /// # Errors
    /// Rejects support/allocation overflow, insufficient storage, or subnormal
    /// coefficients whose exact halving is not representable in binary64.
    pub fn on_cosine_circle(&self) -> Result<Polynomial<Laurent>> {
        let degree = self.degree();
        let count = degree
            .checked_mul(2)
            .and_then(|x| x.checked_add(1))
            .ok_or(Error::SupportOverflow)?;
        self.limits.check(count, 2)?;
        let mut coefficients = zeros(count, self.limits)?;
        for (index, value) in self.coefficients.iter().enumerate() {
            if index == 0 {
                *coefficients.get_mut(degree).ok_or(Error::SupportOverflow)? = *value;
            } else {
                for component in [value.re, value.im] {
                    let half = Interval::point(component)?.checked_mul(Interval::point(0.5)?)?;
                    if half.lower().to_bits() != half.upper().to_bits()
                        && !(half.lower() == 0.0 && half.upper() == 0.0)
                    {
                        return Err(Error::NotEstablished(
                            "cosine substitution loses a subnormal coefficient",
                        ));
                    }
                }
                let left = degree.checked_sub(index).ok_or(Error::SupportOverflow)?;
                let right = degree.checked_add(index).ok_or(Error::SupportOverflow)?;
                *coefficients.get_mut(left).ok_or(Error::SupportOverflow)? = value.mul(0.5);
                *coefficients.get_mut(right).ok_or(Error::SupportOverflow)? = value.mul(0.5);
            }
        }
        let offset = i32::try_from(degree)
            .map_err(|_| Error::SupportOverflow)?
            .checked_neg()
            .ok_or(Error::SupportOverflow)?;
        Polynomial::new(Laurent::new(offset), coefficients, self.limits)
    }
}

fn power(mut base: Complex64, exponent: i32) -> Result<Complex64> {
    let mut result = Complex64::new(1.0, 0.0);
    let mut magnitude = exponent.unsigned_abs();
    while magnitude != 0 {
        if magnitude & 1 != 0 {
            result = finite(result.mul(base))?;
        }
        magnitude >>= 1;
        if magnitude != 0 {
            base = finite(base.mul(base))?;
        }
    }
    if exponent < 0 {
        if result == Complex64::new(0.0, 0.0) {
            return Err(Error::Domain);
        }
        finite(Complex64::new(1.0, 0.0).div(result))
    } else {
        Ok(result)
    }
}
