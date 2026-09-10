use googletest::{Result, prelude::*};
use num_bigint::BigInt;
use quest_math::{Cyclotomic, Limits};
#[gtest]
fn eighth_roots_and_sqrt_two_retain_exact_phase() -> Result<()> {
    let limits = Limits::default();
    let w = Cyclotomic::omega(1);
    expect_eq!(w.checked_mul(&w, limits)?, Cyclotomic::omega(2));
    expect_eq!(
        Cyclotomic::omega(3).checked_mul(&w, limits)?,
        Cyclotomic::omega(4)
    );
    expect_eq!(
        Cyclotomic::omega(7).checked_mul(&w, limits)?,
        Cyclotomic::one()
    );
    let root_half = Cyclotomic::new([0, 1, 0, -1].map(BigInt::from), 1, limits)?;
    expect_eq!(
        root_half.checked_mul(&root_half, limits)?,
        Cyclotomic::new([1, 0, 0, 0].map(BigInt::from), 1, limits)?
    );
    expect_eq!(
        Cyclotomic::new([2, 0, 0, 0].map(BigInt::from), 1, limits)?,
        Cyclotomic::one()
    );
    expect_eq!(
        Cyclotomic::omega(1).checked_add(&Cyclotomic::omega(5), limits)?,
        Cyclotomic::zero()
    );
    Ok(())
}

#[gtest]
fn conjugation_and_admission_respect_exact_resource_bounds() -> Result<()> {
    let limits = Limits::default();
    for power in 0..8u8 {
        let value = Cyclotomic::omega(power);
        expect_eq!(
            value.checked_mul(&value.conjugated(limits)?, limits)?,
            Cyclotomic::one()
        );
    }
    expect_true!(
        Cyclotomic::new(
            [256, 0, 0, 0].map(BigInt::from),
            0,
            Limits {
                coefficient_bits: 8,
                ..limits
            }
        )
        .is_err()
    );
    expect_true!(
        Cyclotomic::new(
            [1, 0, 0, 0].map(BigInt::from),
            9,
            Limits {
                coefficient_bits: 8,
                ..limits
            }
        )
        .is_err()
    );
    expect_true!(
        Cyclotomic::one()
            .checked_mul(&Cyclotomic::one(), Limits { bytes: 1, ..limits })
            .is_err()
    );
    Ok(())
}
