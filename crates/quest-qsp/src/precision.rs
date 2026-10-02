//! Checked native binary arithmetic and exact binary64 interchange for cold stages.
use dashu_base::Abs;
use dashu_float::{
    ConstCache, Context, FBig, FpError,
    round::mode::{Down, HalfEven, Up},
};
/// Native binary value; endpoints remain exact represented dyadics.
pub type Binary = FBig<HalfEven, 2>;
/// Maximum admitted interchange precision.
pub const MAX_PRECISION: u32 = 1_048_576;
/// Checked arithmetic and interchange failure.
#[derive(Debug, thiserror::Error)]
pub enum PrecisionError {
    /// Nonfinite input or output.
    #[error("arbitrary-precision arithmetic produced a nonfinite value")]
    Nonfinite,
    /// Invalid interchange configuration.
    #[error("invalid arbitrary-precision interchange: {0}")]
    Interchange(&'static str),
    /// Domain, exponent range, or correct-rounding failure.
    #[error("native binary arithmetic: {0}")]
    Arithmetic(#[from] FpError),
    /// Shared exact interchange failure.
    #[error(transparent)]
    Boundary(#[from] quest_numerics::arithmetic::ArithmeticError),
}
/// Exact dyadic export rounding direction.
pub use quest_numerics::arithmetic::BinaryRounding;
type Result<T> = std::result::Result<T, PrecisionError>;
/// Admit a finite represented dyadic.
/// # Errors
/// Rejects infinity.
pub fn checked(value: Binary) -> Result<Binary> {
    if value.repr().is_infinite() {
        Err(PrecisionError::Nonfinite)
    } else {
        Ok(value)
    }
}
/// Import the exact binary64 value.
/// # Errors
/// Rejects nonfinite input and invalid precision.
pub fn exact_from_f64(value: f64, precision: u32) -> Result<Binary> {
    if !value.is_finite() {
        return Err(PrecisionError::Nonfinite);
    }
    if !(53..=MAX_PRECISION).contains(&precision) {
        return Err(PrecisionError::Interchange(
            "binary64 import requires 53..=1048576 bits",
        ));
    }
    Ok(quest_numerics::arithmetic::exact_from_f64(
        value, precision,
    )?)
}
/// Export an exact represented dyadic with gradual underflow.
/// # Errors
/// Rejects infinity.
pub fn to_f64(value: &Binary, rounding: BinaryRounding) -> Result<f64> {
    if value.repr().is_infinite() {
        return Err(PrecisionError::Nonfinite);
    }
    Ok(quest_numerics::arithmetic::to_f64(value, rounding)?)
}
// QSP supports the native 32/64-bit targets used by the workspace.
const _: () = assert!(usize::BITS >= u32::BITS);
#[expect(
    clippy::as_conversions,
    reason = "u32 precision widens on supported 32/64-bit targets"
)]
pub(crate) const fn native_precision(p: u32) -> usize {
    p as usize
}
pub(crate) fn integer<T: Into<dashu_int::IBig>>(p: u32, value: T) -> Binary {
    Binary::from_parts(value.into(), 0)
        .with_precision(native_precision(p))
        .value()
}
pub(crate) fn zero(p: u32) -> Binary {
    integer(p, 0)
}
pub(crate) fn abs(value: &Binary) -> Binary {
    value.clone().abs()
}
macro_rules! binary_op {
    ($name:ident, $op:ident, $round:ty) => {
        pub(crate) fn $name(p: u32, a: &Binary, b: &Binary) -> Result<Binary> {
            checked(
                Context::<$round>::new(native_precision(p))
                    .$op(a.repr(), b.repr())?
                    .value()
                    .with_rounding::<HalfEven>(),
            )
        }
    };
}
#[cfg(feature = "offline-synthesis")]
binary_op!(nearest_add, add, HalfEven);
#[cfg(feature = "offline-synthesis")]
binary_op!(nearest_sub, sub, HalfEven);
#[cfg(any(feature = "offline-synthesis", test))]
binary_op!(nearest_mul, mul, HalfEven);
#[cfg(any(feature = "offline-synthesis", test))]
binary_op!(nearest_div, div, HalfEven);
binary_op!(down_add, add, Down);
binary_op!(down_sub, sub, Down);
binary_op!(down_mul, mul, Down);
binary_op!(down_div, div, Down);
binary_op!(up_add, add, Up);
binary_op!(up_sub, sub, Up);
binary_op!(up_mul, mul, Up);
binary_op!(up_div, div, Up);
macro_rules! root_op {
    ($name:ident, $round:ty) => {
        pub(crate) fn $name(p: u32, a: &Binary) -> Result<Binary> {
            checked(
                Context::<$round>::new(native_precision(p))
                    .sqrt(a.repr())?
                    .value()
                    .with_rounding::<HalfEven>(),
            )
        }
    };
}
#[cfg(any(feature = "offline-synthesis", test))]
root_op!(nearest_sqrt, HalfEven);
root_op!(down_sqrt, Down);
root_op!(up_sqrt, Up);
macro_rules! transcendental {
    ($name:ident, $op:ident, $round:ty) => {
        pub(crate) fn $name(p: u32, a: &Binary, cache: &mut ConstCache) -> Result<Binary> {
            checked(
                Context::<$round>::new(native_precision(p))
                    .$op(a.repr(), Some(cache))?
                    .value()
                    .with_rounding::<HalfEven>(),
            )
        }
    };
}
#[cfg(all(test, feature = "offline-synthesis"))]
transcendental!(nearest_sin, sin, HalfEven);
#[cfg(all(test, feature = "offline-synthesis"))]
transcendental!(nearest_cos, cos, HalfEven);
#[cfg(feature = "offline-synthesis")]
transcendental!(nearest_ln, ln, HalfEven);
#[cfg(feature = "offline-synthesis")]
transcendental!(nearest_exp, exp, HalfEven);
#[cfg(feature = "offline-synthesis")]
transcendental!(nearest_atan, atan, HalfEven);
transcendental!(down_sin, sin, Down);
transcendental!(up_sin, sin, Up);
transcendental!(down_cos, cos, Down);
transcendental!(up_cos, cos, Up);
// Keep both native correctly rounded values; retagging changes no represented dyadic.
macro_rules! trigonometric_pair {
    ($name:ident, $round:ty) => {
        pub(crate) fn $name(
            p: u32,
            a: &Binary,
            cache: &mut ConstCache,
        ) -> Result<(Binary, Binary)> {
            let (sin, cos) =
                Context::<$round>::new(native_precision(p)).sin_cos(a.repr(), Some(cache));
            Ok((
                checked(sin?.value().with_rounding::<HalfEven>())?,
                checked(cos?.value().with_rounding::<HalfEven>())?,
            ))
        }
    };
}
#[cfg(feature = "offline-synthesis")]
trigonometric_pair!(nearest_sin_cos, HalfEven);
trigonometric_pair!(down_sin_cos, Down);
trigonometric_pair!(up_sin_cos, Up);
macro_rules! constant {
    ($name:ident, $round:ty) => {
        pub(crate) fn $name(p: u32, cache: &mut ConstCache) -> Result<Binary> {
            checked(
                Context::<$round>::new(native_precision(p))
                    .pi::<2>(Some(cache))
                    .value()
                    .with_rounding::<HalfEven>(),
            )
        }
    };
}
#[cfg(feature = "offline-synthesis")]
constant!(nearest_pi, HalfEven);
constant!(down_pi, Down);
constant!(up_pi, Up);

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;
    #[test]
    fn native_domain_and_exponent_failures_are_typed() {
        assert!(matches!(
            nearest_sqrt(64, &integer(64, -1)),
            Err(PrecisionError::Arithmetic(FpError::OutOfDomain))
        ));
        let huge = Binary::from_parts(dashu_int::IBig::ONE, isize::MAX.saturating_sub(2));
        assert!(matches!(
            nearest_mul(64, &huge, &integer(64, 8)),
            Err(PrecisionError::Arithmetic(FpError::Overflow(_)))
        ));
        let tiny = Binary::from_parts(dashu_int::IBig::ONE, isize::MIN.saturating_add(2));
        assert!(matches!(
            nearest_div(64, &tiny, &integer(64, 8)),
            Err(PrecisionError::Arithmetic(FpError::Underflow(_)))
        ));
    }
    #[gtest]
    fn finite_binary64_import_roundtrips_signed_zero_and_subnormal_at_65_bits()
    -> googletest::Result<()> {
        for bits in [
            0,
            1,
            2,
            3,
            0x000f_ffff_ffff_ffff,
            0x0010_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
        ] {
            for sign in [0, 1_u64 << 63] {
                let point = exact_from_f64(f64::from_bits(bits | sign), 65)?;
                for mode in [
                    BinaryRounding::Down,
                    BinaryRounding::Nearest,
                    BinaryRounding::Up,
                ] {
                    expect_eq!(to_f64(&point, mode)?.to_bits(), bits | sign);
                }
            }
        }
        Ok(())
    }
}
