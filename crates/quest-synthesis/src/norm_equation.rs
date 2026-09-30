//! Exact norm-equation candidates, following Ross--Selinger Appendix C.
//! Returned roots are always checked algebraically. Failure of this bounded
//! factor search is unresolved; it is never a proof of mathematical impossibility.
use crate::{Budget, Result, SynthesisError};
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use std::ops::{Add, Mul, Neg, Sub};

type Ring = [BigInt; 4];
#[derive(Debug, PartialEq, Eq)]
pub enum NormOutcome {
    Solution(Ring),
    NoSolution,
    Unresolved,
}

fn check(value: &BigInt, budget: &mut Budget) -> Result<()> {
    let bits = value.bits();
    if bits > budget.options.limits.coefficient_bits {
        return Err(SynthesisError::Budget {
            resource: "norm coefficient bits",
        });
    }
    let limbs = usize::try_from(bits.div_ceil(64))
        .map_err(|_| SynthesisError::Budget {
            resource: "norm storage",
        })?
        .max(1);
    // At most 128 limb arrays, including quotient candidates and products.
    if limbs
        .checked_mul(1024)
        .is_none_or(|bytes| bytes > budget.options.limits.bytes)
    {
        return Err(SynthesisError::Budget {
            resource: "norm storage",
        });
    }
    budget.charge(limbs)
}
fn checked_ring(value: Ring, budget: &mut Budget) -> Result<Ring> {
    for part in &value {
        check(part, budget)?;
    }
    Ok(value)
}
#[expect(
    clippy::many_single_char_names,
    reason = "Names are coefficients in the fixed four-dimensional cyclotomic basis"
)]
fn product(x: &Ring, y: &Ring, budget: &mut Budget) -> Result<Ring> {
    let [a, b, c, d] = x;
    let [e, f, g, h] = y;
    checked_ring(
        [
            a.mul(e).sub(b.mul(h)).sub(c.mul(g)).sub(d.mul(f)),
            a.mul(f).add(b.mul(e)).sub(c.mul(h)).sub(d.mul(g)),
            a.mul(g).add(b.mul(f)).add(c.mul(e)).sub(d.mul(h)),
            a.mul(h).add(b.mul(g)).add(c.mul(f)).add(d.mul(e)),
        ],
        budget,
    )
}
fn conjugate([a, b, c, d]: &Ring) -> Ring {
    [a.clone(), d.neg(), c.neg(), b.neg()]
}
fn bullet([a, b, c, d]: &Ring) -> Ring {
    [a.clone(), b.neg(), c.clone(), d.neg()]
}
#[expect(
    clippy::many_single_char_names,
    reason = "Names are coefficients in the fixed four-dimensional cyclotomic basis"
)]
fn relative_norm([a, b, c, d]: &Ring, budget: &mut Budget) -> Result<(BigInt, BigInt)> {
    let n = a.mul(a).add(b.mul(b)).add(c.mul(c)).add(d.mul(d));
    let m = a.mul(b.sub(d)).add(c.mul(b.add(d)));
    check(&n, budget)?;
    check(&m, budget)?;
    Ok((n, m))
}
fn field_norm(x: &Ring, budget: &mut Budget) -> Result<BigInt> {
    let (n, m) = relative_norm(x, budget)?;
    let result = (&n).mul(&n).sub((&m).mul(&m).mul(2i32));
    check(&result, budget)?;
    Ok(result)
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Arbitrary integers do not overflow; every divisor is checked positive and exponent shifts are bounded"
)]
fn floor(n: &BigInt, d: &BigInt) -> BigInt {
    let q = n / d;
    if (n % d).is_negative() {
        (&q).sub(1i32)
    } else {
        q
    }
}
fn remainder(a: &Ring, b: &Ring, budget: &mut Budget) -> Result<Option<Ring>> {
    // a/b = a b† b• b†• / N(b). All arithmetic before quotient rounding is exact.
    let conjugated = conjugate(b);
    let adjugate = product(
        &conjugated,
        &product(&bullet(b), &bullet(&conjugated), budget)?,
        budget,
    )?;
    let numerator = product(a, &adjugate, budget)?;
    let denominator = field_norm(b, budget)?;
    if denominator.is_zero() {
        return Err(SynthesisError::Invalid("zero Euclidean divisor"));
    }
    let lower = numerator.map(|value| floor(&value, &denominator));
    let mut best: Option<(BigInt, Ring)> = None;
    // All 16 corners surrounding the exact quotient; strict norm descent is checked.
    for mask in 0u8..16 {
        budget.charge(1)?;
        let q = std::array::from_fn(|index| {
            let value = lower.get(index).cloned().unwrap_or_default();
            if mask
                & (1u8
                    .checked_shl(u32::try_from(index).unwrap_or(u32::MAX))
                    .unwrap_or(0))
                == 0
            {
                value
            } else {
                value.add(1i32)
            }
        });
        let multiple = product(b, &q, budget)?;
        let mut r = std::array::from_fn(|_| BigInt::zero());
        for ((out, left), right) in r.iter_mut().zip(a).zip(multiple) {
            *out = left.sub(right);
        }
        let norm = field_norm(&r, budget)?;
        if best.as_ref().is_none_or(|(previous, _)| norm < *previous) {
            best = Some((norm, r));
        }
    }
    Ok(best.and_then(|(norm, value)| (norm < denominator).then_some(value)))
}
fn gcd(mut a: Ring, mut b: Ring, budget: &mut Budget) -> Result<Option<Ring>> {
    while b.iter().any(|x| !x.is_zero()) {
        budget.charge(1)?;
        let Some(r) = remainder(&a, &b, budget)? else {
            return Ok(None);
        };
        a = b;
        b = r;
    }
    Ok(Some(a))
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Arbitrary integers do not overflow; every divisor is checked positive and exponent shifts are bounded"
)]
fn modular_power(
    mut base: BigInt,
    mut exponent: BigInt,
    modulus: &BigInt,
    budget: &mut Budget,
) -> Result<BigInt> {
    let mut out = BigInt::one();
    base %= modulus;
    while !exponent.is_zero() {
        budget.charge(1)?;
        if (&exponent & BigInt::one()).is_one() {
            out = (&out).mul(&base) % modulus;
            check(&out, budget)?;
        }
        exponent >>= 1;
        if !exponent.is_zero() {
            base = (&base).mul(&base) % modulus;
            check(&base, budget)?;
        }
    }
    Ok(out)
}
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Arbitrary integers do not overflow; every divisor is checked positive and exponent shifts are bounded"
)]
fn sqrt_minus_one(modulus: &BigInt, seed: u64, budget: &mut Budget) -> Result<Option<BigInt>> {
    if modulus <= &BigInt::one() || (modulus % 4i32) != BigInt::one() {
        return Ok(None);
    }
    let exponent = modulus.sub(1i32) / 4i32;
    let start = BigInt::from(seed).add(2i32);
    for offset in 0u32..32 {
        let root = modular_power(
            (&start).add(offset) % modulus,
            exponent.clone(),
            modulus,
            budget,
        )?;
        if (&root).mul(&root) % modulus == modulus.sub(1i32) {
            return Ok(Some(root));
        }
    }
    Ok(None)
}
#[expect(
    clippy::many_single_char_names,
    reason = "Names are coefficients in the fixed four-dimensional cyclotomic basis"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Arbitrary integers do not overflow; every divisor is checked positive and exponent shifts are bounded"
)]
fn correct_unit(
    mut root: Ring,
    n: &BigInt,
    m: &BigInt,
    budget: &mut Budget,
) -> Result<Option<Ring>> {
    let (a, b) = relative_norm(&root, budget)?;
    let denominator = (&a).mul(&a).sub((&b).mul(&b).mul(2i32));
    if denominator.is_zero() {
        return Ok(None);
    }
    let un = n.mul(&a).sub(m.mul(&b).mul(2i32));
    let vn = m.mul(&a).sub(n.mul(&b));
    if !(&un % &denominator).is_zero() || !(&vn % &denominator).is_zero() {
        return Ok(None);
    }
    let mut u = un / &denominator;
    let mut v = vn / &denominator;
    if u < BigInt::one() || (&u).mul(&u).sub((&v).mul(&v).mul(2i32)) != BigInt::one() {
        return Ok(None);
    }
    let inverse = v.is_negative();
    let unit = [
        BigInt::from(if inverse { -1 } else { 1 }),
        BigInt::one(),
        BigInt::zero(),
        BigInt::from(-1),
    ];
    while !v.is_zero() {
        budget.charge(1)?;
        let next_u = (&u).mul(3i32).sub(v.abs().mul(4i32));
        let next_v = if inverse {
            (&v).mul(3i32).add((&u).mul(2i32))
        } else {
            (&v).mul(3i32).sub((&u).mul(2i32))
        };
        if next_u < BigInt::one() || next_u >= u {
            return Ok(None);
        }
        root = product(&root, &unit, budget)?;
        u = next_u;
        v = next_v;
    }
    let actual = relative_norm(&root, budget)?;
    Ok((actual.0 == *n && actual.1 == *m).then_some(root))
}

/// Fast exact candidates for prime rational norms, plus ramification and units.
/// Modular exponentiation is only a search heuristic: no primality claim is made,
/// and neither a failed root search nor an incomplete gcd proves nonexistence.
#[expect(
    clippy::many_single_char_names,
    reason = "Names are coefficients in the fixed four-dimensional cyclotomic basis"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Arbitrary integers do not overflow; every divisor is checked positive and exponent shifts are bounded"
)]
pub fn solve(n: &BigInt, m: &BigInt, budget: &mut Budget, seed: u64) -> Result<NormOutcome> {
    budget.charge(1)?;
    check(n, budget)?;
    check(m, budget)?;
    let rational_norm = n.mul(n).sub(m.mul(m).mul(2i32));
    check(&rational_norm, budget)?;
    if n.is_negative() || rational_norm.is_negative() {
        return Ok(NormOutcome::NoSolution);
    }
    if n.is_zero() {
        return Ok(NormOutcome::Solution(std::array::from_fn(|_| {
            BigInt::zero()
        })));
    }
    let mut a = n.clone();
    let mut b = m.clone();
    let mut factor = [
        BigInt::one(),
        BigInt::zero(),
        BigInt::zero(),
        BigInt::zero(),
    ];
    while (&a % 2i32).is_zero() {
        budget.charge(1)?;
        if (&b % 2i32).is_zero() {
            a /= 2;
            b /= 2;
            factor = product(
                &factor,
                &[BigInt::one(), BigInt::zero(), BigInt::one(), BigInt::zero()],
                budget,
            )?;
        } else {
            let next_a = (&a).sub(&b);
            let next_b = (&b).sub(&a / 2i32);
            a = next_a;
            b = next_b;
            factor = product(
                &factor,
                &[BigInt::one(), BigInt::one(), BigInt::zero(), BigInt::zero()],
                budget,
            )?;
        }
    }
    // Modulo 2, n=A+B and m=AB, A=a+c, B=b+d. Odd n forces even m.
    if !(&b % 2i32).is_zero() {
        return Ok(NormOutcome::NoSolution);
    }
    let p = (&a).mul(&a).sub((&b).mul(&b).mul(2i32));
    check(&p, budget)?;
    let root = if p.is_one() {
        correct_unit(
            [
                BigInt::one(),
                BigInt::zero(),
                BigInt::zero(),
                BigInt::zero(),
            ],
            &a,
            &b,
            budget,
        )?
    } else {
        let modulus = if b.is_zero() { &a } else { &p };
        let Some(h) = sqrt_minus_one(modulus, seed, budget)? else {
            return Ok(NormOutcome::Unresolved);
        };
        let Some(g) = gcd(
            [a.clone(), b.clone(), BigInt::zero(), (&b).neg()],
            [h, BigInt::zero(), BigInt::one(), BigInt::zero()],
            budget,
        )?
        else {
            return Ok(NormOutcome::Unresolved);
        };
        correct_unit(g, &a, &b, budget)?
    };
    let Some(root) = root else {
        return Ok(NormOutcome::Unresolved);
    };
    let root = product(&factor, &root, budget)?;
    let actual = relative_norm(&root, budget)?;
    if actual.0 != *n || actual.1 != *m {
        return Err(SynthesisError::Invalid(
            "norm solution failed exact reconstruction",
        ));
    }
    Ok(NormOutcome::Solution(root))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::SynthesisOptions;
    #[expect(
        clippy::many_single_char_names,
        reason = "Published norm n+m√2 in the algebraic basis a+bω+cω²+dω³"
    )]
    fn check(n: i64, m: i64) -> Result<()> {
        let mut budget = Budget {
            options: SynthesisOptions::default(),
            used: 0,
        };
        let NormOutcome::Solution([a, b, c, d]) = solve(&n.into(), &m.into(), &mut budget, 13)?
        else {
            return Err(crate::SynthesisError::Invalid("norm fixture unresolved"));
        };
        if (&a)
            .mul(&a)
            .add((&b).mul(&b))
            .add((&c).mul(&c))
            .add((&d).mul(&d))
            != BigInt::from(n)
        {
            return Err(crate::SynthesisError::Invalid(
                "rational norm component differs",
            ));
        }
        if (&a).mul((&b).sub(&d)).add((&c).mul((&b).add(&d))) != BigInt::from(m) {
            return Err(crate::SynthesisError::Invalid(
                "quadratic norm component differs",
            ));
        }
        Ok(())
    }
    #[test]
    fn prime_norms_and_unit_associates() -> Result<()> {
        for (n, m) in [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 2),
            (3, -2),
            (5, 2),
            (5, -2),
            (7, 2),
            (7, -2),
            (29, 20),
            (29, -20),
            (10, 4),
            (20, -8),
        ] {
            check(n, m)?;
        }
        Ok(())
    }
    #[test]
    fn unresolved_is_not_impossible_and_work_is_bounded() -> Result<()> {
        let mut budget = Budget {
            options: SynthesisOptions::default(),
            used: 0,
        };
        if solve(&1.into(), &1.into(), &mut budget, 1)? != NormOutcome::NoSolution
            || solve(&15.into(), &2.into(), &mut budget, 1)? != NormOutcome::Unresolved
        {
            return Err(crate::SynthesisError::Invalid(
                "incorrect norm failure classification",
            ));
        }
        budget.options.max_work = 0;
        if solve(&5.into(), &2.into(), &mut budget, 1).is_ok() {
            return Err(crate::SynthesisError::Invalid("norm work bound ignored"));
        }
        Ok(())
    }
}
