use crate::{Error, Limits, Result};
use num_bigint::BigInt;
use num_traits::Zero;
/// Canonical elements (a+bω+cω²+dω³)/2^k, with ω^4=-1.
/// Coefficients and denominator are private; deserialization cannot bypass admission.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Cyclotomic {
    coefficients: [BigInt; 4],
    denominator_exponent: u32,
}
impl Cyclotomic {
    #[must_use]
    pub fn zero() -> Self {
        Self {
            coefficients: std::array::from_fn(|_| BigInt::zero()),
            denominator_exponent: 0,
        }
    }
    #[must_use]
    pub fn one() -> Self {
        Self::omega(0)
    }
    #[must_use]
    pub fn omega(power: u8) -> Self {
        let slot = usize::from(power & 3);
        let sign = if power & 4 == 0 { 1 } else { -1 };
        Self {
            coefficients: std::array::from_fn(|index| {
                BigInt::from(if index == slot { sign } else { 0 })
            }),
            denominator_exponent: 0,
        }
    }
    #[must_use]
    pub const fn coefficients(&self) -> &[BigInt; 4] {
        &self.coefficients
    }
    #[must_use]
    pub const fn denominator_exponent(&self) -> u32 {
        self.denominator_exponent
    }
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.coefficients.iter().all(Zero::is_zero)
    }
    /// Admit exact coefficients and normalize powers of two.
    /// # Errors
    /// Rejects coefficient, denominator and allocation resource excesses.
    pub fn new(
        mut coefficients: [BigInt; 4],
        mut denominator_exponent: u32,
        limits: Limits,
    ) -> Result<Self> {
        check_bits(u64::from(denominator_exponent), limits)?;
        for coefficient in &coefficients {
            check_bits(coefficient.bits(), limits)?;
        }
        crate::types::allocation(limits.coefficient_bits, 8, limits)?;
        if coefficients.iter().all(Zero::is_zero) {
            return Ok(Self::zero());
        }
        while denominator_exponent != 0 && coefficients.iter().all(|value| !value.bit(0)) {
            for coefficient in &mut coefficients {
                std::ops::ShrAssign::shr_assign(coefficient, 1usize);
            }
            denominator_exponent = denominator_exponent
                .checked_sub(1)
                .ok_or_else(|| Error::Resource("denominator normalization".into()))?;
        }
        Ok(Self {
            coefficients,
            denominator_exponent,
        })
    }
    fn admit(&self, limits: Limits) -> Result<()> {
        check_bits(u64::from(self.denominator_exponent), limits)?;
        for coefficient in &self.coefficients {
            check_bits(coefficient.bits(), limits)?;
        }
        Ok(())
    }
    /// Multiply in the cyclotomic ring with checked resources.
    /// # Errors
    /// Rejects coefficient and allocation resource excesses.
    pub fn checked_mul(&self, rhs: &Self, limits: Limits) -> Result<Self> {
        self.admit(limits)?;
        rhs.admit(limits)?;
        let scratch_bits = limits
            .coefficient_bits
            .checked_mul(2)
            .and_then(|bits| bits.checked_add(3))
            .ok_or_else(|| Error::Resource("coefficient scratch width".into()))?;
        crate::types::allocation(scratch_bits, 12, limits)?;
        let exponent = self
            .denominator_exponent
            .checked_add(rhs.denominator_exponent)
            .ok_or_else(|| Error::Resource("denominator exponent".into()))?;
        check_bits(u64::from(exponent), limits)?;
        let mut result = std::array::from_fn(|_| BigInt::zero());
        for (i, a) in self.coefficients.iter().enumerate() {
            for (j, b) in rhs.coefficients.iter().enumerate() {
                let degree = i
                    .checked_add(j)
                    .ok_or_else(|| Error::Resource("polynomial degree".into()))?;
                let slot = result
                    .get_mut(degree & 3)
                    .ok_or_else(|| Error::Resource("polynomial coefficient".into()))?;
                let product = std::ops::Mul::mul(a, b);
                if degree >= 4 {
                    std::ops::SubAssign::sub_assign(slot, product);
                } else {
                    std::ops::AddAssign::add_assign(slot, product);
                }
            }
        }
        Self::new(result, exponent, limits)
    }
    /// Add in the cyclotomic ring with checked resources.
    /// # Errors
    /// Rejects coefficient and allocation resource excesses.
    pub fn checked_add(&self, rhs: &Self, limits: Limits) -> Result<Self> {
        self.admit(limits)?;
        rhs.admit(limits)?;
        let scratch_bits = limits
            .coefficient_bits
            .checked_mul(2)
            .and_then(|bits| bits.checked_add(1))
            .ok_or_else(|| Error::Resource("sum scratch width".into()))?;
        crate::types::allocation(scratch_bits, 12, limits)?;
        let exponent = self.denominator_exponent.max(rhs.denominator_exponent);
        let left = exponent
            .checked_sub(self.denominator_exponent)
            .ok_or_else(|| Error::Resource("left denominator".into()))?;
        let right = exponent
            .checked_sub(rhs.denominator_exponent)
            .ok_or_else(|| Error::Resource("right denominator".into()))?;
        let mut result = std::array::from_fn(|_| BigInt::zero());
        for ((output, a), b) in result
            .iter_mut()
            .zip(&self.coefficients)
            .zip(&rhs.coefficients)
        {
            *output = std::ops::Add::add(std::ops::Shl::shl(a, left), std::ops::Shl::shl(b, right));
        }
        Self::new(result, exponent, limits)
    }
    /// Complex conjugation preserves the canonical dyadic denominator.
    /// # Errors
    /// Rejects resource excesses under the supplied limits.
    pub fn conjugated(&self, limits: Limits) -> Result<Self> {
        self.admit(limits)?;
        crate::types::allocation(limits.coefficient_bits, 8, limits)?;
        let [a, b, c, d] = &self.coefficients;
        Self::new(
            [
                a.clone(),
                std::ops::Neg::neg(d),
                std::ops::Neg::neg(c),
                std::ops::Neg::neg(b),
            ],
            self.denominator_exponent,
            limits,
        )
    }
}
fn check_bits(bits: u64, limits: Limits) -> Result<()> {
    if bits > limits.coefficient_bits {
        return Err(crate::types::budget(
            "coefficient bits",
            bits,
            limits.coefficient_bits,
        ));
    }
    Ok(())
}
