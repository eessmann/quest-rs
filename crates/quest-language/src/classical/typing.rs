use super::{FloatWidth, ScalarType, ValueError};
use crate::syntax::{BinaryOperator as B, UnaryOperator as U};
impl ScalarType {
	/// Whether source cast syntax permits this category and width conversion.
	///
	/// Follows the `OpenQASM` 3.1 allowed-casts table; permitted casts may still
	/// fail for values outside the target's representable range.
	#[must_use]
	pub const fn can_explicitly_cast_to(self, target: Self) -> bool {
		match (self, target) {
			(_, Self::Bool)
			| (
				Self::Bool | Self::Int(_) | Self::Uint(_) | Self::Float(_),
				Self::Int(_) | Self::Uint(_) | Self::Float(_),
			)
			| (Self::Float(_) | Self::Angle(_), Self::Angle(_)) => true,
			(Self::Bool, Self::Bit(width)) => width.value() == 1,
			(
				Self::Int(source) | Self::Uint(source) | Self::Angle(source) | Self::Bit(source),
				Self::Bit(target),
			)
			| (Self::Bit(source), Self::Int(target) | Self::Uint(target) | Self::Angle(target)) => {
				source.value() == target.value()
			}
			_ => false,
		}
	}

	/// Whether assignment may perform this conversion without explicit cast syntax.
	#[must_use]
	pub const fn can_implicitly_cast_to(self, target: Self) -> bool {
		if let (Self::Bit(source), Self::Bit(target)) = (self, target) {
			return source.value() == target.value();
		}
		matches!(
			(self, target),
			(
				Self::Bool | Self::Int(_) | Self::Uint(_) | Self::Float(_),
				Self::Bool | Self::Int(_) | Self::Uint(_) | Self::Float(_)
			) | (Self::Bit(_), Self::Bit(_))
				| (Self::Angle(_) | Self::Float(_), Self::Angle(_))
		)
	}

	/// Determine a unary instruction's type without evaluating a dummy value.
	///
	/// # Errors
	/// Rejects operator/type combinations outside the simulator profile.
	pub const fn unary_result(self, operator: U) -> Result<Self, ValueError> {
		match (operator, self) {
			(U::Not, Self::Bool)
			| (U::Complement, Self::Bit(_) | Self::Uint(_) | Self::Angle(_))
			| (
				U::Negate | U::Positive,
				Self::Int(_) | Self::Uint(_) | Self::Angle(_) | Self::Float(_),
			) => Ok(self),
			_ => Err(ValueError::Type),
		}
	}
	/// Determine a binary result, including integer promotion and angle units.
	///
	/// # Errors
	/// Rejects unsupported operators and incompatible scalar types.
	pub fn binary_result(self, operator: B, rhs: Self) -> Result<Self, ValueError> {
		if matches!(operator, B::And | B::Or) {
			return if self == Self::Bool && rhs == Self::Bool {
				Ok(Self::Bool)
			} else {
				Err(ValueError::Type)
			};
		}
		if matches!(operator, B::ShiftLeft | B::ShiftRight) {
			return if matches!(self, Self::Bit(_) | Self::Uint(_) | Self::Angle(_))
				&& matches!(rhs, Self::Int(_) | Self::Uint(_))
			{
				Ok(self)
			} else {
				Err(ValueError::Type)
			};
		}
		if let Some(result) = mixed_angle(self, operator, rhs) {
			return result;
		}
		let common = common(self, rhs)?;
		if matches!(operator, B::Equal | B::NotEqual) {
			return Ok(Self::Bool);
		}
		if matches!(
			operator,
			B::Less | B::LessEqual | B::Greater | B::GreaterEqual
		) {
			return if common == Self::Bool {
				Err(ValueError::Type)
			} else {
				Ok(Self::Bool)
			};
		}
		match common {
			Self::Float(_)
				if matches!(
					operator,
					B::Add | B::Subtract | B::Multiply | B::Divide | B::Power
				) =>
			{
				Ok(common)
			}
			Self::Int(_) | Self::Uint(_)
				if matches!(
					operator,
					B::Add | B::Subtract | B::Multiply | B::Divide | B::Power | B::Remainder
				) =>
			{
				Ok(common)
			}
			Self::Bit(_) | Self::Uint(_) | Self::Angle(_)
				if matches!(operator, B::BitAnd | B::BitOr | B::BitXor) =>
			{
				Ok(common)
			}
			Self::Angle(width) if operator == B::Divide => Ok(Self::Uint(width)),
			Self::Angle(_) if matches!(operator, B::Add | B::Subtract) => Ok(common),
			_ => Err(ValueError::Type),
		}
	}
}
pub(super) fn common(left: ScalarType, right: ScalarType) -> Result<ScalarType, ValueError> {
	use ScalarType as T;
	if left == right {
		return Ok(left);
	}
	Ok(match (left, right) {
		(T::Float(a), T::Float(b)) => T::Float(if a == FloatWidth::F64 || b == FloatWidth::F64 {
			FloatWidth::F64
		} else {
			FloatWidth::F32
		}),
		(T::Float(w), T::Int(_) | T::Uint(_)) | (T::Int(_) | T::Uint(_), T::Float(w)) => {
			T::Float(w)
		}
		(T::Int(a), T::Int(b)) => T::Int(a.max(b)),
		(T::Uint(a), T::Uint(b)) => T::Uint(a.max(b)),
		(T::Angle(a), T::Angle(b)) => T::Angle(a.max(b)),
		(T::Int(a), T::Uint(b)) | (T::Uint(b), T::Int(a)) => {
			if a > b {
				T::Int(a)
			} else {
				T::Uint(b)
			}
		}
		_ => return Err(ValueError::Type),
	})
}
fn mixed_angle(
	left: ScalarType,
	operator: B,
	right: ScalarType,
) -> Option<Result<ScalarType, ValueError>> {
	use ScalarType as T;
	match (left, right) {
		(T::Angle(a), T::Uint(b)) => {
			Some(if a == b && matches!(operator, B::Multiply | B::Divide) {
				Ok(left)
			} else {
				Err(ValueError::Type)
			})
		}
		(T::Uint(a), T::Angle(b)) => Some(if a == b && operator == B::Multiply {
			Ok(right)
		} else {
			Err(ValueError::Type)
		}),
		_ => None,
	}
}

impl ScalarType {
	/// Determine a built-in function result independently of its input values.
	///
	/// # Errors
	/// Rejects unknown functions, wrong arities and unsupported parameter types.
	pub fn function_result(name: &str, arguments: &[Self]) -> Result<Self, ValueError> {
		if let [left, right] = arguments {
			return binary_function(name, *left, *right);
		}
		let [argument] = arguments else {
			return Err(ValueError::Function(name.into()));
		};
		match name {
			"popcount" if matches!(argument, Self::Bit(_) | Self::Uint(_) | Self::Angle(_)) => {
				Ok(Self::Uint(super::Width::new(64)?))
			}
			"abs" if matches!(argument, Self::Int(_) | Self::Uint(_) | Self::Float(_)) => {
				Ok(*argument)
			}
			"sin" | "cos" | "tan" if matches!(argument, Self::Angle(_)) => {
				Ok(Self::Float(FloatWidth::F64))
			}
			"sin" | "cos" | "tan" | "arcsin" | "arccos" | "arctan" | "exp" | "ln" | "log"
			| "sqrt" | "floor" | "ceil" | "ceiling" | "round"
				if matches!(argument, Self::Int(_) | Self::Uint(_) | Self::Float(_)) =>
			{
				Ok(if let Self::Float(width) = argument {
					Self::Float(*width)
				} else {
					Self::Float(FloatWidth::F64)
				})
			}
			_ => Err(ValueError::Function(name.into())),
		}
	}
}
fn binary_function(
	name: &str,
	left: ScalarType,
	right: ScalarType,
) -> Result<ScalarType, ValueError> {
	use ScalarType as T;
	match name {
		"rotl" | "rotr"
			if matches!(left, T::Bit(_) | T::Uint(_) | T::Angle(_))
				&& matches!(right, T::Int(_) | T::Uint(_)) =>
		{
			Ok(left)
		}
		"pow" if matches!(left, T::Int(_) | T::Uint(_)) && matches!(right, T::Uint(_)) => {
			Ok(T::Int(super::Width::new(64)?))
		}
		"mod"
			if matches!(left, T::Int(_) | T::Uint(_))
				&& matches!(right, T::Int(_) | T::Uint(_)) =>
		{
			common(left, right)
		}
		"mod" | "pow"
			if matches!(left, T::Int(_) | T::Uint(_) | T::Float(_))
				&& matches!(right, T::Int(_) | T::Uint(_) | T::Float(_)) =>
		{
			let common = common(left, right)?;
			Ok(if matches!(common, T::Float(_)) {
				common
			} else {
				T::Float(FloatWidth::F64)
			})
		}
		_ => Err(ValueError::Function(name.into())),
	}
}
