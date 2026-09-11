#![cfg(feature = "certification")]

use astro_float::{BigFloat, EXPONENT_MAX, EXPONENT_MIN, Sign, Word};
use googletest::prelude::*;
use quest_qsp::precision::{BinaryRounding, PrecisionError, checked, exact_from_f64, to_f64};

const SIGN: u64 = 1_u64 << 63;
const MAX: u64 = 0x7fef_ffff_ffff_ffff;
const WORD_BITS: usize = size_of::<Word>().saturating_mul(8);

// Independent integer fixture: numerator * 2^scale. Normalization and packing
// depend only on integer arithmetic, never on either interchange function.
fn dyadic(numerator: u128, scale: i32, negative: bool, width: usize) -> Result<BigFloat> {
    let bits = usize::try_from(128_u32.saturating_sub(numerator.leading_zeros()))?;
    let shift = width
        .checked_sub(bits)
        .ok_or_else(|| std::io::Error::other("fixture width"))?;
    let mut words = vec![Word::from(0_u8); width / WORD_BITS];
    for source in 0..bits {
        if numerator & (1_u128 << source) != 0 {
            let destination = shift
                .checked_add(source)
                .ok_or_else(|| std::io::Error::other("fixture index"))?;
            *words
                .get_mut(destination / WORD_BITS)
                .ok_or_else(|| std::io::Error::other("fixture word"))? |=
                Word::from(1_u8) << (destination % WORD_BITS);
        }
    }
    let exponent = i32::try_from(bits)?
        .checked_add(scale)
        .ok_or_else(|| std::io::Error::other("fixture exponent"))?;
    let value = BigFloat::from_raw_parts(
        &words,
        if numerator == 0 { 0 } else { width },
        if negative { Sign::Neg } else { Sign::Pos },
        exponent,
        false,
    );
    assert_that!(value.err(), none());
    Ok(value)
}

fn components(bits: u64) -> Result<(u128, i32)> {
    let exponent = i32::try_from((bits >> 52) & 0x7ff)?;
    let fraction = u128::from(bits & 0x000f_ffff_ffff_ffff);
    Ok(if exponent == 0 {
        (fraction, -1074)
    } else {
        (
            fraction | (1_u128 << 52),
            exponent
                .checked_sub(1075)
                .ok_or_else(|| std::io::Error::other("scale"))?,
        )
    })
}

fn seeded_bits() -> impl Iterator<Item = u64> {
    let mut state = 0x41c6_4e6d_c0ff_ee01_u64;
    (0..2048).map(move |_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    })
}

#[gtest]
fn seeded_imports_equal_independent_integer_dyadics_at_multiple_precisions() -> Result<()> {
    for bits in seeded_bits().filter(|bits| (bits >> 52) & 0x7ff != 0x7ff) {
        let (numerator, scale) = components(bits)?;
        let expected = dyadic(numerator, scale, bits & SIGN != 0, 256)?;
        for precision in [53, 54, 63, 64, 65, 127, 128, 256] {
            let actual = exact_from_f64(f64::from_bits(bits), precision).map_err(|error| {
                std::io::Error::other(format!("bits={bits:#018x} precision={precision}: {error}"))
            })?;
            assert_that!(actual.cmp(&expected), some(eq(0)));
            for rounding in [
                BinaryRounding::Down,
                BinaryRounding::Nearest,
                BinaryRounding::Up,
            ] {
                assert_that!(to_f64(&actual, rounding)?.to_bits(), eq(bits));
                assert_that!(to_f64(&expected, rounding)?.to_bits(), eq(bits));
            }
        }
    }
    Ok(())
}

#[gtest]
fn midpoint_neighbors_round_with_sign_parity_and_gradual_underflow() -> Result<()> {
    let boundaries = [
        0,
        1,
        2,
        3,
        0x000f_ffff_ffff_fffe,
        0x000f_ffff_ffff_ffff,
        0x0010_0000_0000_0000,
        0x3fef_ffff_ffff_ffff,
        0x3ff0_0000_0000_0000,
        0x3ff0_0000_0000_0001,
        MAX.checked_sub(1)
            .ok_or_else(|| std::io::Error::other("max predecessor"))?,
        MAX,
    ];
    for low in boundaries
        .into_iter()
        .chain(seeded_bits().take(1024).map(|bits| bits % MAX))
    {
        let high = low
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("successor"))?;
        let (numerator, scale) = components(low)?;
        let midpoint = numerator
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_shl(60))
            .ok_or_else(|| std::io::Error::other("midpoint numerator"))?;
        let scale = scale
            .checked_sub(61)
            .ok_or_else(|| std::io::Error::other("midpoint scale"))?;
        for side in [-1_i32, 0, 1] {
            let numerator = match side {
                -1 => midpoint.checked_sub(1),
                0 => Some(midpoint),
                _ => midpoint.checked_add(1),
            }
            .ok_or_else(|| std::io::Error::other("midpoint perturbation"))?;
            let nearest = if side < 0 || (side == 0 && low & 1 == 0) {
                low
            } else {
                high
            };
            for negative in [false, true] {
                let sign = if negative { SIGN } else { 0 };
                let value = dyadic(numerator, scale, negative, 128)?;
                let down = if negative { high } else { low };
                let up = if negative { low } else { high };
                assert_that!(
                    to_f64(&value, BinaryRounding::Down)?.to_bits(),
                    eq(down | sign)
                );
                assert_that!(to_f64(&value, BinaryRounding::Up)?.to_bits(), eq(up | sign));
                assert_that!(
                    to_f64(&value, BinaryRounding::Nearest)?.to_bits(),
                    eq(nearest | sign)
                );
            }
        }
    }
    Ok(())
}

#[gtest]
fn storage_padding_and_sticky_bits_cross_multiple_word_boundaries() -> Result<()> {
    // 1 + 2^-53 + 2^-200 is strictly above the even midpoint at one.
    for width in [256_usize, 512] {
        for negative in [false, true] {
            let mut words = vec![Word::from(0_u8); width / WORD_BITS];
            for offset in [1_usize, 54, 201] {
                let bit = width
                    .checked_sub(offset)
                    .ok_or_else(|| std::io::Error::other("sticky bit"))?;
                *words
                    .get_mut(bit / WORD_BITS)
                    .ok_or_else(|| std::io::Error::other("sticky word"))? |=
                    Word::from(1_u8) << (bit % WORD_BITS);
            }
            let sign = if negative { Sign::Neg } else { Sign::Pos };
            let value = BigFloat::from_raw_parts(&words, width, sign, 1, false);
            assert_that!(value.err(), none());
            let expected = 0x3ff0_0000_0000_0001 | if negative { SIGN } else { 0 };
            assert_that!(
                to_f64(&value, BinaryRounding::Nearest)?.to_bits(),
                eq(expected)
            );
        }
    }
    Ok(())
}

#[gtest]
fn backend_exponent_extremes_and_signed_zero_keep_directional_semantics() -> Result<()> {
    for negative in [false, true] {
        let sign = if negative { Sign::Neg } else { Sign::Pos };
        let sign_bit = if negative { SIGN } else { 0 };
        let tiny = BigFloat::from_raw_parts(&[Word::from(1_u8)], 1, sign, EXPONENT_MIN, false);
        let huge = BigFloat::from_raw_parts(
            &[Word::from(1_u8) << WORD_BITS.saturating_sub(1)],
            WORD_BITS,
            sign,
            EXPONENT_MAX,
            false,
        );
        for (mode, tiny_bits, huge_bits) in [
            (BinaryRounding::Nearest, 0, f64::INFINITY.to_bits()),
            (
                BinaryRounding::Down,
                u64::from(negative),
                if negative {
                    f64::INFINITY.to_bits()
                } else {
                    MAX
                },
            ),
            (
                BinaryRounding::Up,
                u64::from(!negative),
                if negative {
                    MAX
                } else {
                    f64::INFINITY.to_bits()
                },
            ),
        ] {
            assert_that!(to_f64(&tiny, mode)?.to_bits(), eq(sign_bit | tiny_bits));
            assert_that!(to_f64(&huge, mode)?.to_bits(), eq(sign_bit | huge_bits));
            assert_that!(
                to_f64(&dyadic(0, 0, negative, 128)?, mode)?.to_bits(),
                eq(sign_bit)
            );
        }
    }
    Ok(())
}

#[gtest]
fn invalid_inputs_keep_backend_nonfinite_and_precision_errors_distinct() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        expect_true!(matches!(
            exact_from_f64(value, 256),
            Err(PrecisionError::Nonfinite)
        ));
    }
    for precision in [0, 1, 32, 52] {
        expect_true!(matches!(
            exact_from_f64(1.0, precision),
            Err(PrecisionError::Interchange(_))
        ));
    }
    for value in [astro_float::INF_POS, astro_float::INF_NEG] {
        expect_true!(matches!(
            to_f64(&value, BinaryRounding::Nearest),
            Err(PrecisionError::Nonfinite)
        ));
        expect_true!(matches!(checked(value), Err(PrecisionError::Nonfinite)));
    }
    let backend_error = BigFloat::nan(Some(astro_float::Error::InvalidArgument));
    expect_true!(matches!(
        to_f64(&backend_error, BinaryRounding::Nearest),
        Err(PrecisionError::Backend(astro_float::Error::InvalidArgument))
    ));
    expect_true!(matches!(
        checked(backend_error),
        Err(PrecisionError::Backend(astro_float::Error::InvalidArgument))
    ));
    expect_true!(matches!(
        to_f64(&BigFloat::nan(None), BinaryRounding::Nearest),
        Err(PrecisionError::Nonfinite)
    ));
    expect_true!(matches!(
        checked(BigFloat::nan(None)),
        Err(PrecisionError::Nonfinite)
    ));
}

#[gtest]
fn rounding_provenance_does_not_change_the_represented_dyadic() -> Result<()> {
    let expected = dyadic((1_u128 << 54) | 3, -54, true, 256)?;
    let mut marked = expected.clone();
    marked.set_inexact(true);
    for mode in [
        BinaryRounding::Down,
        BinaryRounding::Nearest,
        BinaryRounding::Up,
    ] {
        assert_that!(
            to_f64(&marked, mode)?.to_bits(),
            eq(to_f64(&expected, mode)?.to_bits())
        );
    }
    let admitted = checked(marked)?;
    assert_that!(admitted.cmp(&expected), some(eq(0)));
    assert_that!(admitted.inexact(), eq(false));
    Ok(())
}
