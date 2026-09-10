use crate::{Error, Limits, Rational, Result};
use num_bigint::BigInt;
use num_traits::{Signed, Zero};

/// Internal closed dyadic interval, in units of the associated Grid's 2^-bits.
#[derive(Debug, Clone)]
pub struct Interval {
    pub lower: BigInt,
    pub upper: BigInt,
}
impl Interval {
    pub fn point(value: BigInt) -> Self {
        Self {
            lower: value.clone(),
            upper: value,
        }
    }
    pub fn add(&self, rhs: &Self) -> Self {
        Self {
            lower: std::ops::Add::add(&self.lower, &rhs.lower),
            upper: std::ops::Add::add(&self.upper, &rhs.upper),
        }
    }
    pub fn negated(&self) -> Self {
        Self {
            lower: std::ops::Neg::neg(&self.upper),
            upper: std::ops::Neg::neg(&self.lower),
        }
    }
    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.negated())
    }
    pub fn scaled(&self, factor: &BigInt) -> Self {
        if factor.is_negative() {
            Self {
                lower: std::ops::Mul::mul(&self.upper, factor),
                upper: std::ops::Mul::mul(&self.lower, factor),
            }
        } else {
            Self {
                lower: std::ops::Mul::mul(&self.lower, factor),
                upper: std::ops::Mul::mul(&self.upper, factor),
            }
        }
    }
    pub fn divided(&self, positive: &BigInt) -> Result<Self> {
        Ok(Self {
            lower: floor_div(&self.lower, positive)?,
            upper: ceil_div(&self.upper, positive)?,
        })
    }
    pub fn magnitude(&self) -> BigInt {
        self.lower.abs().max(self.upper.abs())
    }
}
pub struct Grid {
    pub bits: usize,
    pub scale: BigInt,
    pub limits: Limits,
}
impl Grid {
    pub fn new(bits: usize, limits: Limits) -> Result<Self> {
        if bits == 0 || bits > limits.precision_bits.min(4096) {
            return Err(crate::types::budget(
                "precision bits",
                crate::types::size(bits)?,
                crate::types::size(limits.precision_bits.min(4096))?,
            ));
        }
        let scratch = crate::types::size(bits)?
            .checked_mul(4)
            .and_then(|n| n.checked_add(limits.coefficient_bits.checked_mul(2)?))
            .ok_or_else(|| Error::Resource("interval scratch width".into()))?;
        crate::types::allocation(scratch, 128, limits)?;
        Ok(Self {
            bits,
            scale: std::ops::Shl::shl(BigInt::from(1), bits),
            limits,
        })
    }
    pub fn integer(&self, value: i32) -> Interval {
        Interval::point(std::ops::Mul::mul(&self.scale, value))
    }
    pub fn rational(&self, value: &Rational) -> Result<Interval> {
        let numerator = std::ops::Shl::shl(value.numer(), self.bits);
        Ok(Interval {
            lower: floor_div(&numerator, value.denom())?,
            upper: ceil_div(&numerator, value.denom())?,
        })
    }
    pub fn mul(&self, lhs: &Interval, rhs: &Interval) -> Result<Interval> {
        let products = [
            std::ops::Mul::mul(&lhs.lower, &rhs.lower),
            std::ops::Mul::mul(&lhs.lower, &rhs.upper),
            std::ops::Mul::mul(&lhs.upper, &rhs.lower),
            std::ops::Mul::mul(&lhs.upper, &rhs.upper),
        ];
        let lower = products
            .iter()
            .min()
            .ok_or_else(|| Error::Resource("empty interval product".into()))?;
        let upper = products
            .iter()
            .max()
            .ok_or_else(|| Error::Resource("empty interval product".into()))?;
        Ok(Interval {
            lower: floor_div(lower, &self.scale)?,
            upper: ceil_div(upper, &self.scale)?,
        })
    }
    pub fn square(&self, value: &Interval) -> Result<Interval> {
        let maximum = value.magnitude();
        let upper = ceil_div(&std::ops::Mul::mul(&maximum, &maximum), &self.scale)?;
        let minimum = if value.lower <= BigInt::zero() && value.upper >= BigInt::zero() {
            BigInt::zero()
        } else {
            value.lower.abs().min(value.upper.abs())
        };
        Ok(Interval {
            lower: floor_div(&std::ops::Mul::mul(&minimum, &minimum), &self.scale)?,
            upper,
        })
    }
    pub fn pi(&self) -> Result<Interval> {
        static BOUNDS: [std::sync::OnceLock<Result<Interval>>; 7] =
            [const { std::sync::OnceLock::new() }; 7];
        for (bits, cache) in [64, 128, 256, 512, 1024, 2048, 4096]
            .into_iter()
            .zip(&BOUNDS)
        {
            if self.bits == bits {
                return cache.get_or_init(|| pi_units(bits)).clone();
            }
        }
        pi_units(self.bits)
    }
    pub fn root_two(&self) -> Result<Interval> {
        let shift = self
            .bits
            .checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| Error::Resource("sqrt two scale".into()))?;
        let value = std::ops::Shl::shl(BigInt::from(1), shift);
        let mut root = std::ops::Shl::shl(
            BigInt::from(1),
            self.bits
                .checked_add(1)
                .ok_or_else(|| Error::Resource("sqrt two seed".into()))?,
        );
        loop {
            let next = std::ops::Shr::shr(
                std::ops::Add::add(&root, std::ops::Div::div(&value, &root)),
                1usize,
            );
            if next >= root {
                return Ok(Interval {
                    lower: root.clone(),
                    upper: std::ops::Add::add(root, 1),
                });
            }
            root = next;
        }
    }
    pub fn sin_cos(&self, value: &Interval) -> Result<(Interval, Interval)> {
        if value.magnitude() > std::ops::Mul::mul(&self.scale, 4) {
            return Err(Error::NotCertified);
        }
        Ok((self.taylor(value, true)?, self.taylor(value, false)?))
    }
    fn taylor(&self, value: &Interval, sine: bool) -> Result<Interval> {
        let square = self.square(value)?;
        let mut term = if sine { value.clone() } else { self.integer(1) };
        let mut sum = term.clone();
        let threshold = BigInt::from(65_536);
        for index in 0..self.limits.taylor_terms {
            let degree = index
                .checked_mul(2)
                .and_then(|n| n.checked_add(if sine { 2 } else { 1 }))
                .ok_or_else(|| Error::Resource("Taylor degree".into()))?;
            let denominator = degree
                .checked_mul(
                    degree
                        .checked_add(1)
                        .ok_or_else(|| Error::Resource("Taylor degree".into()))?,
                )
                .ok_or_else(|| Error::Resource("Taylor denominator".into()))?;
            let next = self
                .mul(&term, &square)?
                .negated()
                .divided(&BigInt::from(denominator))?;
            // For |x|<=4, terms strictly decrease once index>=2. The
            // alternating-series remainder is bounded by the first omitted term.
            let remainder = next.magnitude();
            if index >= 2 && remainder <= threshold {
                return Ok(Interval {
                    lower: std::ops::Sub::sub(&sum.lower, &remainder),
                    upper: std::ops::Add::add(&sum.upper, remainder),
                });
            }
            sum = sum.add(&next);
            term = next;
        }
        Err(Error::NotCertified)
    }
}
pub fn floor_div(numerator: &BigInt, denominator: &BigInt) -> Result<BigInt> {
    if denominator <= &BigInt::zero() {
        return Err(Error::Invalid(
            "interval denominator must be positive".into(),
        ));
    }
    let quotient = std::ops::Div::div(numerator, denominator);
    let remainder = std::ops::Rem::rem(numerator, denominator);
    Ok(if remainder.is_negative() {
        std::ops::Sub::sub(quotient, 1)
    } else {
        quotient
    })
}
fn ceil_div(numerator: &BigInt, denominator: &BigInt) -> Result<BigInt> {
    Ok(std::ops::Neg::neg(floor_div(
        &std::ops::Neg::neg(numerator),
        denominator,
    )?))
}
fn pi_units(bits: usize) -> Result<Interval> {
    let five = atan_units(5, bits)?;
    let large = atan_units(239, bits)?;
    Ok(five
        .scaled(&BigInt::from(16))
        .sub(&large.scaled(&BigInt::from(4))))
}
fn atan_units(reciprocal: u16, bits: usize) -> Result<Interval> {
    let scale = std::ops::Shl::shl(BigInt::from(1), bits);
    let square = std::ops::Mul::mul(BigInt::from(reciprocal), BigInt::from(reciprocal));
    let mut power = BigInt::from(reciprocal);
    let mut sum = BigInt::zero();
    for index in 0..bits {
        let odd = index
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| Error::Resource("arctangent degree".into()))?;
        let term = std::ops::Div::div(&scale, std::ops::Mul::mul(&power, odd));
        if term.is_zero() {
            let error = BigInt::from(
                index
                    .checked_add(1)
                    .ok_or_else(|| Error::Resource("arctangent remainder".into()))?,
            );
            return Ok(Interval {
                lower: std::ops::Sub::sub(&sum, &error),
                upper: std::ops::Add::add(sum, error),
            });
        }
        if index.is_multiple_of(2) {
            std::ops::AddAssign::add_assign(&mut sum, term);
        } else {
            std::ops::SubAssign::sub_assign(&mut sum, term);
        }
        std::ops::MulAssign::mul_assign(&mut power, &square);
    }
    Err(Error::NotCertified)
}
