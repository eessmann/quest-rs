use super::super::{ErrorKind, SemanticError};
use crate::{
	classical::{FloatWidth, ScalarType, Width},
	syntax::{self, Expression, ExpressionKind, ScalarKind},
};
mod sealed {
	pub trait Sealed {}
}
/// Sealed classical expression categories; widths are checked when constructing handles.
pub trait Classical: sealed::Sealed + Clone {
	/// # Errors
	/// Rejects widths outside the supported scalar profile.
	fn scalar_type() -> Result<ScalarType, SemanticError>;
	/// # Errors
	/// Rejects widths outside the supported scalar profile.
	fn syntax_type() -> Result<syntax::Type, SemanticError>;
}
/// Types supporting same-type addition, subtraction, and ordering.
pub trait Numeric: Classical {}
/// Types supporting same-type multiplication and division.
pub trait Arithmetic: Numeric {}
#[derive(Debug, Clone)]
pub struct Bool;
impl sealed::Sealed for Bool {}
impl Classical for Bool {
	fn scalar_type() -> Result<ScalarType, SemanticError> {
		Ok(ScalarType::Bool)
	}
	fn syntax_type() -> Result<syntax::Type, SemanticError> {
		Ok(syntax::Type::Scalar(ScalarKind::Bool, None))
	}
}
fn width(value: u8) -> Result<Width, SemanticError> {
	Width::new(value).map_err(SemanticError::from)
}
fn sized(kind: ScalarKind, value: u8) -> syntax::Type {
	syntax::Type::Scalar(
		kind,
		Some(Box::new(Expression {
			kind: ExpressionKind::Number(value.to_string()),
			span: None,
		})),
	)
}
macro_rules! integer_type {
	($name:ident,$kind:ident) => {
		#[derive(Debug, Clone)]
		pub struct $name<const WIDTH: u8>;
		impl<const WIDTH: u8> sealed::Sealed for $name<WIDTH> {}
		impl<const WIDTH: u8> Classical for $name<WIDTH> {
			fn scalar_type() -> Result<ScalarType, SemanticError> {
				Ok(ScalarType::$kind(width(WIDTH)?))
			}
			fn syntax_type() -> Result<syntax::Type, SemanticError> {
				width(WIDTH)?;
				Ok(sized(ScalarKind::$kind, WIDTH))
			}
		}
	};
}
integer_type!(Int, Int);
integer_type!(Uint, Uint);
integer_type!(Bit, Bit);
integer_type!(Angle, Angle);
#[derive(Debug, Clone)]
pub struct Float<const WIDTH: u8>;
impl<const WIDTH: u8> sealed::Sealed for Float<WIDTH> {}
impl<const WIDTH: u8> Classical for Float<WIDTH> {
	fn scalar_type() -> Result<ScalarType, SemanticError> {
		Ok(ScalarType::Float(float_width(WIDTH)?))
	}
	fn syntax_type() -> Result<syntax::Type, SemanticError> {
		float_width(WIDTH)?;
		Ok(sized(ScalarKind::Float, WIDTH))
	}
}
pub(super) fn float_width(width: u8) -> Result<FloatWidth, SemanticError> {
	match width {
		32 => Ok(FloatWidth::F32),
		64 => Ok(FloatWidth::F64),
		_ => Err(SemanticError::new(
			ErrorKind::Type,
			"float width must be 32 or 64",
		)),
	}
}
impl<const W: u8> Numeric for Int<W> {}
impl<const W: u8> Numeric for Uint<W> {}
impl<const W: u8> Numeric for Angle<W> {}
impl<const W: u8> Numeric for Float<W> {}
impl<const W: u8> Arithmetic for Int<W> {}
impl<const W: u8> Arithmetic for Uint<W> {}
impl<const W: u8> Arithmetic for Float<W> {}
