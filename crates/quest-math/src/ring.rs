use crate::{Error, Limits, Result};
use dashu_base::BitTest;
use dashu_int::IBig;
/// Exponent of a power of two in the canonical coefficient representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PowerOfTwoExponent(pub u32);
/// Least nonnegative exponent of sqrt(2) clearing all ring denominators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sqrt2Exponent(pub u32);
/// An admitted eighth root of unity, with canonical exponent in 0..8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EighthRootPhase(u8);
impl EighthRootPhase {
    /// # Errors
    /// Rejects a noncanonical phase exponent.
    pub fn new(power: u8) -> Result<Self> {
        if power >= 8 {
            return Err(Error::Invalid("noncanonical eighth-root phase".into()));
        }
        Ok(Self(power))
    }
    #[must_use]
    pub const fn power(self) -> u8 {
        self.0
    }
}
/// The quotient `Z[omega]/(2)`; low bit is the constant coefficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmegaResidue(u8);
impl OmegaResidue {
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 15)
    }
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
    #[must_use]
    pub const fn reducible(self) -> bool {
        matches!(self.0, 0 | 5 | 10 | 15)
    }
    #[must_use]
    pub const fn times_omega(self) -> Self {
        Self(((self.0 << 1) | (self.0 >> 3)) & 15)
    }
    #[must_use]
    pub const fn conjugated(self) -> Self {
        Self((self.0 & 5) | ((self.0 & 2) << 2) | ((self.0 & 8) >> 2))
    }
    #[must_use]
    pub fn product(self, rhs: Self) -> Self {
        let mut out = 0;
        let mut shifted = rhs;
        for i in 0..4 {
            if self.0 & (1 << i) != 0 {
                out ^= shifted.0;
            }
            shifted = shifted.times_omega();
        }
        Self(out)
    }
    #[must_use]
    pub fn norm(self) -> Self {
        self.conjugated().product(self)
    }
}

/// Admitted algebraic integer, sharing Cyclotomic's canonical representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmegaInteger(Cyclotomic);
impl OmegaInteger {
    /// # Errors
    /// Rejects nonintegral elements.
    pub fn admit(value: Cyclotomic) -> Result<Self> {
        if value.denominator_exponent != 0 {
            return Err(Error::Invalid("nonintegral omega element".into()));
        }
        Ok(Self(value))
    }
    #[must_use]
    pub const fn value(&self) -> &Cyclotomic {
        &self.0
    }
    #[must_use]
    pub fn residue(&self) -> OmegaResidue {
        let mut bits = 0;
        for (index, coefficient) in self.0.coefficients.iter().enumerate() {
            if coefficient.bit(0) {
                bits |= 1 << index;
            }
        }
        OmegaResidue(bits)
    }
}
/// Canonical elements (a+bω+cω²+dω³)/2^k, with ω^4=-1.
/// Coefficients and denominator are private; deserialization cannot bypass admission.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Cyclotomic {
    #[cfg_attr(feature = "serde", serde(with = "crate::encoding::integer_array"))]
    coefficients: [IBig; 4],
    denominator_exponent: u32,
}
impl Cyclotomic {
    #[must_use]
    pub fn phase(phase: EighthRootPhase) -> Self {
        Self::omega(phase.power())
    }
    #[must_use]
    pub fn eighth_root_phase(&self) -> Option<EighthRootPhase> {
        (0..8)
            .find(|&power| Self::omega(power) == *self)
            .map(EighthRootPhase)
    }
    #[must_use]
    pub const fn power_of_two_exponent(&self) -> PowerOfTwoExponent {
        PowerOfTwoExponent(self.denominator_exponent)
    }
    /// Multiply by sqrt(2) in the same canonical ring.
    /// # Errors
    /// Rejects arithmetic resources beyond the request limits.
    pub fn times_sqrt2(&self, limits: Limits) -> Result<Self> {
        self.admit(limits)?;
        let [a, b, c, d] = &self.coefficients;
        Self::new(
            [
                std::ops::Sub::sub(b, d),
                std::ops::Add::add(a, c),
                std::ops::Add::add(b, d),
                std::ops::Sub::sub(c, a),
            ],
            self.denominator_exponent,
            limits,
        )
    }
    /// # Errors
    /// Rejects arithmetic resources beyond the request limits.
    pub fn least_sqrt2_exponent(&self, limits: Limits) -> Result<Sqrt2Exponent> {
        self.admit(limits)?;
        if self.denominator_exponent == 0 {
            return Ok(Sqrt2Exponent(0));
        }
        let twice = self
            .denominator_exponent
            .checked_mul(2)
            .ok_or_else(|| Error::Resource("sqrt2 exponent".into()))?;
        let odd = self.times_sqrt2(limits)?.denominator_exponent < self.denominator_exponent;
        Ok(Sqrt2Exponent(
            twice
                .checked_sub(u32::from(odd))
                .ok_or_else(|| Error::Resource("sqrt2 exponent".into()))?,
        ))
    }
    /// Scale by a power of sqrt(2) and require an algebraic integer result.
    /// # Errors
    /// Rejects a non-clearing exponent or exhausted arithmetic resources.
    pub fn numerator_at(&self, exponent: Sqrt2Exponent, limits: Limits) -> Result<OmegaInteger> {
        self.admit(limits)?;
        check_bits(u64::from(exponent.0), limits)?;
        let value = if exponent.0 & 1 == 1 {
            self.times_sqrt2(limits)?
        } else {
            self.clone()
        };
        let scale = exponent.0 / 2;
        if scale < value.denominator_exponent {
            return Err(Error::Invalid(
                "sqrt2 exponent does not clear denominator".into(),
            ));
        }
        let shift = scale
            .checked_sub(value.denominator_exponent)
            .ok_or_else(|| Error::Resource("sqrt2 numerator".into()))?;
        for coefficient in &value.coefficients {
            check_bits(
                u64::try_from(coefficient.bit_len())
                    .unwrap_or(u64::MAX)
                    .saturating_add(u64::from(shift)),
                limits,
            )?;
        }
        let shift =
            usize::try_from(shift).map_err(|_| Error::Resource("sqrt2 numerator shift".into()))?;
        OmegaInteger::admit(Self::new(
            value.coefficients.map(|c| std::ops::Shl::shl(c, shift)),
            0,
            limits,
        )?)
    }
    /// # Errors
    /// Rejects an exponent which does not clear denominators or a resource excess.
    pub fn residue_at(&self, exponent: Sqrt2Exponent, limits: Limits) -> Result<OmegaResidue> {
        Ok(self.numerator_at(exponent, limits)?.residue())
    }
    #[must_use]
    pub fn zero() -> Self {
        Self {
            coefficients: std::array::from_fn(|_| IBig::ZERO),
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
                IBig::from(if index == slot { sign } else { 0 })
            }),
            denominator_exponent: 0,
        }
    }
    #[must_use]
    pub const fn coefficients(&self) -> &[IBig; 4] {
        &self.coefficients
    }
    #[must_use]
    pub const fn denominator_exponent(&self) -> u32 {
        self.denominator_exponent
    }
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.coefficients.iter().all(IBig::is_zero)
    }
    /// Admit exact coefficients and normalize powers of two.
    /// # Errors
    /// Rejects coefficient, denominator and allocation resource excesses.
    pub fn new(
        mut coefficients: [IBig; 4],
        mut denominator_exponent: u32,
        limits: Limits,
    ) -> Result<Self> {
        check_bits(u64::from(denominator_exponent), limits)?;
        for coefficient in &coefficients {
            check_bits(
                u64::try_from(coefficient.bit_len()).unwrap_or(u64::MAX),
                limits,
            )?;
        }
        crate::types::allocation(limits.coefficient_bits, 8, limits)?;
        if coefficients.iter().all(IBig::is_zero) {
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
            check_bits(
                u64::try_from(coefficient.bit_len()).unwrap_or(u64::MAX),
                limits,
            )?;
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
        let mut result = std::array::from_fn(|_| IBig::ZERO);
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
        let left =
            usize::try_from(left).map_err(|_| Error::Resource("left denominator shift".into()))?;
        let right = usize::try_from(right)
            .map_err(|_| Error::Resource("right denominator shift".into()))?;
        let mut result = std::array::from_fn(|_| IBig::ZERO);
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
