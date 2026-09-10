use googletest::prelude::*;
use quest_language::classical::{FloatWidth, ScalarType, ScalarValue, Width};
use quest_language::syntax::{BinaryOperator as B, UnaryOperator as U};

#[gtest]
fn standard_builtins_keep_widths_and_signed_rotation_distances() -> Result<()> {
    let bits = ScalarValue::bitstring("10001111")?;
    let two = ScalarValue::parse_number("2")?;
    expect_eq!(
        ScalarValue::function("rotl", &[bits, two])?.raw_bits()?,
        0b0011_1110
    );
    let negative = ScalarValue::parse_number("-2")?;
    expect_eq!(
        ScalarValue::function("rotr", &[bits, negative])?,
        ScalarValue::function("rotl", &[bits, two])?
    );
    expect_eq!(
        ScalarValue::function("popcount", &[bits])?.ty(),
        ScalarType::Uint(Width::new(64)?)
    );
    expect_eq!(
        ScalarValue::function("ceiling", &[ScalarValue::parse_number("1.25")?])?.to_f64()?,
        2.
    );
    expect_eq!(
        ScalarValue::function("log", &[ScalarValue::parse_number("1.")?])?.to_f64()?,
        0.
    );
    expect_eq!(
        ScalarValue::function("mod", &[ScalarValue::parse_number("-7")?, two])?.to_i128()?,
        -1
    );
    expect_eq!(
        ScalarValue::function("pow", &[two, negative])?.to_f64()?,
        0.25
    );
    expect_true!(
        ScalarValue::function("log", &[ScalarValue::angle_bits(Width::new(4)?, 1)?]).is_err()
    );
    Ok(())
}

#[gtest]
fn integer_division_and_checked_signed_overflow() -> Result<()> {
    let one = ScalarValue::parse_number("1")?;
    let two = ScalarValue::parse_number("2")?;
    expect_eq!(one.binary(B::Divide, &two)?.to_i128()?, 0);
    let width = Width::new(4)?;
    expect_true!(
        ScalarValue::signed(width, 7)?
            .binary(B::Add, &ScalarValue::signed(width, 1)?)
            .is_err()
    );
    expect_eq!(
        ScalarValue::unsigned(width, 15)?
            .binary(B::Add, &ScalarValue::unsigned(width, 1)?)?
            .to_i128()?,
        0
    );
    expect_true!(
        one.binary(B::Divide, &ScalarValue::parse_number("0")?)
            .is_err()
    );
    Ok(())
}

#[gtest]
fn angle_casts_are_modular_and_ties_even() -> Result<()> {
    let width = Width::new(8)?;
    let target = ScalarType::Angle(width);
    let value = ScalarValue::floating(FloatWidth::F64, std::f64::consts::TAU * (127.0 / 512.0))?
        .cast(target)?;
    expect_eq!(value.cast(ScalarType::Bit(width))?.raw_bits()?, 64);
    let negative =
        ScalarValue::floating(FloatWidth::F64, -std::f64::consts::FRAC_PI_2)?.cast(target)?;
    expect_eq!(negative.raw_bits()?, 192);
    expect_eq!(
        negative
            .binary(B::Add, &negative.unary(U::Negate)?)?
            .raw_bits()?,
        0
    );
    Ok(())
}

#[gtest]
fn casts_preserve_bits_and_reject_invalid_widths_and_nonfinite_values() -> Result<()> {
    expect_true!(Width::new(0).is_err());
    expect_true!(Width::new(65).is_err());
    let width = Width::new(64)?;
    let value = ScalarValue::signed(width, -1)?;
    expect_eq!(value.cast(ScalarType::Uint(width))?.raw_bits()?, u64::MAX);
    expect_eq!(
        value
            .cast(ScalarType::Uint(width))?
            .cast(ScalarType::Int(width))?
            .to_i128()?,
        -1
    );
    expect_true!(ScalarValue::floating(FloatWidth::F64, f64::NAN).is_err());
    expect_true!(ScalarValue::parse_number("1.0im").is_err());
    Ok(())
}

#[gtest]
fn angle_products_preserve_units_and_reject_mixed_addition() -> Result<()> {
    let width = Width::new(4)?;
    let angle = ScalarValue::angle_bits(width, 10)?;
    let two = ScalarValue::unsigned(width, 2)?;
    for result in [
        angle.binary(B::Multiply, &two)?,
        two.binary(B::Multiply, &angle)?,
    ] {
        expect_eq!(result.ty(), ScalarType::Angle(width));
        expect_eq!(result.raw_bits()?, 4);
    }
    expect_true!(angle.binary(B::Add, &two).is_err());
    expect_true!(two.binary(B::Divide, &angle).is_err());
    expect_true!(angle.binary(B::Multiply, &angle).is_err());
    Ok(())
}

#[gtest]
fn explicit_cast_categories_follow_the_openqasm_allowed_cast_table() -> Result<()> {
    let width = Width::new(8)?;
    let values = [
        ScalarValue::boolean(true),
        ScalarValue::signed(width, 1)?,
        ScalarValue::unsigned(width, 1)?,
        ScalarValue::floating(FloatWidth::F64, 1.0)?,
        ScalarValue::angle_bits(width, 1)?,
        ScalarValue::bitstring("00000001")?,
    ];
    // Rows and columns: bool, int8, uint8, float64, angle8, bit8.
    let allowed = [
        [true, true, true, true, false, false],
        [true, true, true, true, false, true],
        [true, true, true, true, false, true],
        [true, true, true, true, true, false],
        [true, false, false, false, true, true],
        [true, true, true, false, true, true],
    ];
    for (value, row) in values.iter().zip(allowed) {
        for (target, expected) in values.iter().zip(row) {
            verify_eq!(value.ty().can_explicitly_cast_to(target.ty()), expected)?;
            verify_eq!(value.cast(target.ty()).is_ok(), expected)?;
        }
    }
    verify_that!(
        ScalarValue::boolean(true)
            .cast(ScalarType::Bit(Width::new(1)?))
            .is_ok(),
        eq(true)
    )?;
    Ok(())
}
