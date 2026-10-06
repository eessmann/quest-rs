use super::{Data, FloatWidth, ScalarType, ScalarValue, ValueError, Width};
use crate::syntax::{BinaryOperator as B, UnaryOperator as U};

pub(super) fn unary(value: ScalarValue, operator: U) -> Result<ScalarValue, ValueError> {
	match operator {
		U::Positive if !matches!(value.ty, ScalarType::Bool | ScalarType::Bit(_)) => Ok(value),
		U::Not => Ok(ScalarValue::boolean(!value.to_bool()?)),
		U::Complement => {
			let width = value.ty.width().ok_or(ValueError::Type)?;
			Ok(ScalarValue {
				data: Data::Bits(!value.raw_bits()? & width.mask()),
				..value
			})
		}
		U::Negate => match value.ty {
			ScalarType::Float(width) => ScalarValue::floating(width, -value.to_f64()?),
			ScalarType::Int(width) => ScalarValue::signed(
				width,
				value.to_i128()?.checked_neg().ok_or(ValueError::Overflow)?,
			),
			ScalarType::Uint(width) | ScalarType::Angle(width) => Ok(ScalarValue {
				data: Data::Bits(value.raw_bits()?.wrapping_neg() & width.mask()),
				..value
			}),
			_ => Err(ValueError::Type),
		},
		U::Positive => Err(ValueError::Type),
	}
}
pub(super) fn binary(
	lhs: ScalarValue,
	operator: B,
	rhs: ScalarValue,
) -> Result<ScalarValue, ValueError> {
	if matches!(operator, B::And | B::Or) {
		return Ok(ScalarValue::boolean(if operator == B::And {
			lhs.to_bool()? && rhs.to_bool()?
		} else {
			lhs.to_bool()? || rhs.to_bool()?
		}));
	}
	if matches!(operator, B::ShiftLeft | B::ShiftRight) {
		return shift(lhs, operator, rhs);
	}
	if let Some(result) = mixed_angle(lhs, operator, rhs) {
		return result;
	}
	let (lhs, rhs) = promote(lhs, rhs)?;
	if matches!(
		operator,
		B::Equal | B::NotEqual | B::Less | B::LessEqual | B::Greater | B::GreaterEqual
	) {
		return compare(lhs, operator, rhs);
	}
	if let ScalarType::Float(width) = lhs.ty {
		let left = lhs.to_f64()?;
		let right = rhs.to_f64()?;
		let operation = match operator {
			B::Add => mathcore::scalar::BinaryOperation::Add,
			B::Subtract => mathcore::scalar::BinaryOperation::Subtract,
			B::Multiply => mathcore::scalar::BinaryOperation::Multiply,
			B::Divide => mathcore::scalar::BinaryOperation::Divide,
			B::Power => mathcore::scalar::BinaryOperation::Power,
			_ => return Err(ValueError::Type),
		};
		let value =
			mathcore::scalar::binary(left, operation, right).map_err(|error| match error {
				mathcore::arithmetic::ArithmeticError::Domain("division") => {
					ValueError::DivisionByZero
				}
				_ => ValueError::NonFinite,
			})?;
		return ScalarValue::floating(width, value);
	}
	if let ScalarType::Int(width) = lhs.ty {
		return signed(lhs, operator, rhs, width);
	}
	let width = lhs.ty.width().ok_or(ValueError::Type)?;
	let left = lhs.raw_bits()?;
	let right = rhs.raw_bits()?;
	let value = match operator {
		B::BitAnd => left & right,
		B::BitOr => left | right,
		B::BitXor => left ^ right,
		B::Add if !matches!(lhs.ty, ScalarType::Bit(_)) => left.wrapping_add(right),
		B::Subtract if !matches!(lhs.ty, ScalarType::Bit(_)) => left.wrapping_sub(right),
		B::Multiply if !matches!(lhs.ty, ScalarType::Bit(_)) => left.wrapping_mul(right),
		B::Divide if !matches!(lhs.ty, ScalarType::Bit(_)) => {
			left.checked_div(right).ok_or(ValueError::DivisionByZero)?
		}
		B::Remainder if matches!(lhs.ty, ScalarType::Uint(_)) => {
			left.checked_rem(right).ok_or(ValueError::DivisionByZero)?
		}
		B::Power if matches!(lhs.ty, ScalarType::Uint(_)) => {
			left.wrapping_pow(u32::try_from(right).map_err(|_| ValueError::Overflow)?)
		}
		_ => return Err(ValueError::Type),
	} & width.mask();
	let ty = if matches!(lhs.ty, ScalarType::Angle(_))
		&& matches!(rhs.ty, ScalarType::Angle(_))
		&& operator == B::Divide
	{
		ScalarType::Uint(width)
	} else {
		lhs.ty
	};
	if matches!(lhs.ty, ScalarType::Angle(_))
		&& matches!(rhs.ty, ScalarType::Angle(_))
		&& operator == B::Multiply
	{
		return Err(ValueError::Type);
	}
	Ok(ScalarValue {
		ty,
		data: Data::Bits(value),
	})
}
fn signed(
	lhs: ScalarValue,
	operator: B,
	rhs: ScalarValue,
	width: Width,
) -> Result<ScalarValue, ValueError> {
	let left = lhs.to_i128()?;
	let right = rhs.to_i128()?;
	let result = match operator {
		B::Add => left.checked_add(right),
		B::Subtract => left.checked_sub(right),
		B::Multiply => left.checked_mul(right),
		B::Divide => {
			if right == 0 {
				return Err(ValueError::DivisionByZero);
			}
			left.checked_div(right)
		}
		B::Remainder => {
			if right == 0 {
				return Err(ValueError::DivisionByZero);
			}
			left.checked_rem(right)
		}
		B::Power => left.checked_pow(u32::try_from(right).map_err(|_| ValueError::Type)?),
		_ => return Err(ValueError::Type),
	}
	.ok_or(ValueError::Overflow)?;
	ScalarValue::signed(width, result)
}
fn shift(lhs: ScalarValue, operator: B, rhs: ScalarValue) -> Result<ScalarValue, ValueError> {
	if !matches!(
		lhs.ty,
		ScalarType::Bit(_) | ScalarType::Uint(_) | ScalarType::Angle(_)
	) {
		return Err(ValueError::Type);
	}
	let count = u32::try_from(rhs.to_i128()?).map_err(|_| ValueError::Type)?;
	let width = lhs.ty.width().ok_or(ValueError::Type)?;
	let bits = if count >= u32::from(width.value()) {
		0
	} else if operator == B::ShiftLeft {
		lhs.raw_bits()?
			.checked_shl(count)
			.ok_or(ValueError::Overflow)?
			& width.mask()
	} else {
		lhs.raw_bits()?
			.checked_shr(count)
			.ok_or(ValueError::Overflow)?
	};
	Ok(ScalarValue {
		data: Data::Bits(bits),
		..lhs
	})
}
fn promote(lhs: ScalarValue, rhs: ScalarValue) -> Result<(ScalarValue, ScalarValue), ValueError> {
	let target = super::typing::common(lhs.ty, rhs.ty)?;
	Ok((lhs.cast(target)?, rhs.cast(target)?))
}

fn compare(lhs: ScalarValue, operator: B, rhs: ScalarValue) -> Result<ScalarValue, ValueError> {
	if lhs.ty != rhs.ty {
		return Err(ValueError::Type);
	}
	let order = match (lhs.data, rhs.data) {
		(Data::Float(left), Data::Float(right)) => {
			left.partial_cmp(&right).ok_or(ValueError::NonFinite)?
		}
		(Data::Bool(left), Data::Bool(right)) => left.cmp(&right),
		(Data::Bits(left), Data::Bits(right)) if !matches!(lhs.ty, ScalarType::Int(_)) => {
			left.cmp(&right)
		}
		_ => lhs.to_i128()?.cmp(&rhs.to_i128()?),
	};
	Ok(ScalarValue::boolean(match operator {
		B::Equal => order.is_eq(),
		B::NotEqual => !order.is_eq(),
		B::Less => order.is_lt(),
		B::LessEqual => !order.is_gt(),
		B::Greater => order.is_gt(),
		B::GreaterEqual => !order.is_lt(),
		_ => return Err(ValueError::Type),
	}))
}
pub(super) fn function(name: &str, arguments: &[ScalarValue]) -> Result<ScalarValue, ValueError> {
	if let [left, right] = arguments {
		return binary_function(name, *left, *right);
	}
	let [argument] = arguments else {
		return Err(ValueError::Function(name.into()));
	};
	let result_type = ScalarType::function_result(name, &[argument.ty()])?;
	if name == "popcount" {
		return ScalarValue::unsigned(
			Width::new(64)?,
			u64::from(argument.raw_bits()?.count_ones()),
		);
	}
	if name == "abs" {
		if let ScalarType::Int(width) = result_type {
			return ScalarValue::signed(
				width,
				argument
					.to_i128()?
					.checked_abs()
					.ok_or(ValueError::Overflow)?,
			);
		}
		if matches!(result_type, ScalarType::Uint(_)) {
			return Ok(*argument);
		}
	}
	let value = argument.to_f64()?;
	let shared = match name {
		"sin" => Some(mathcore::scalar::UnaryOperation::Sin),
		"cos" => Some(mathcore::scalar::UnaryOperation::Cos),
		"exp" => Some(mathcore::scalar::UnaryOperation::Exp),
		"ln" | "log" => Some(mathcore::scalar::UnaryOperation::Ln),
		"sqrt" => Some(mathcore::scalar::UnaryOperation::Sqrt),
		_ => None,
	};
	let result = if let Some(operation) = shared {
		mathcore::scalar::unary(value, operation).map_err(|_| ValueError::NonFinite)?
	} else {
		match name {
			"tan" => value.tan(),
			"arcsin" => value.asin(),
			"arccos" => value.acos(),
			"arctan" => value.atan(),
			"floor" => value.floor(),
			"ceil" | "ceiling" => value.ceil(),
			"round" => value.round_ties_even(),
			"abs" => value.abs(),
			_ => return Err(ValueError::Function(name.into())),
		}
	};
	ScalarValue::floating(
		if let ScalarType::Float(width) = argument.ty {
			width
		} else {
			FloatWidth::F64
		},
		result,
	)
}

fn binary_function(
	name: &str,
	left: ScalarValue,
	right: ScalarValue,
) -> Result<ScalarValue, ValueError> {
	let ty = ScalarType::function_result(name, &[left.ty, right.ty])?;
	if name == "rotl" || name == "rotr" {
		let width = ty.width().ok_or(ValueError::Type)?;
		let distance = right.to_i128()?;
		let distance = if name == "rotr" {
			distance.checked_neg().ok_or(ValueError::Overflow)?
		} else {
			distance
		};
		let amount = u32::try_from(distance.rem_euclid(i128::from(width.value())))
			.map_err(|_| ValueError::Overflow)?;
		let other = u32::from(width.value())
			.checked_sub(amount)
			.ok_or(ValueError::Overflow)?;
		let bits = left.raw_bits()?;
		let rotated = if amount == 0 {
			bits
		} else {
			bits.checked_shl(amount).ok_or(ValueError::Overflow)?
				| bits.checked_shr(other).ok_or(ValueError::Overflow)?
		} & width.mask();
		return Ok(ScalarValue {
			ty,
			data: Data::Bits(rotated),
		});
	}
	let left = left.cast(ty)?;
	let right = right.cast(ty)?;
	if let ScalarType::Float(width) = ty {
		let left = left.to_f64()?;
		let right = right.to_f64()?;
		return ScalarValue::floating(
			width,
			match name {
				"mod" if right != 0. => left % right,
				"mod" => return Err(ValueError::DivisionByZero),
				"pow" => left.powf(right),
				_ => return Err(ValueError::Function(name.into())),
			},
		);
	}
	left.binary(
		if name == "pow" {
			B::Power
		} else {
			B::Remainder
		},
		&right,
	)
}

fn mixed_angle(
	lhs: ScalarValue,
	operator: B,
	rhs: ScalarValue,
) -> Option<Result<ScalarValue, ValueError>> {
	let (angle, integer, angle_first) = match (lhs.ty, rhs.ty) {
		(ScalarType::Angle(_), ScalarType::Uint(_)) => (lhs, rhs, true),
		(ScalarType::Uint(_), ScalarType::Angle(_)) => (rhs, lhs, false),
		_ => return None,
	};
	Some((|| {
		if angle.ty.width() != integer.ty.width() {
			return Err(ValueError::Type);
		}
		let bits = match operator {
			B::Multiply => angle.raw_bits()?.wrapping_mul(integer.raw_bits()?),
			B::Divide if angle_first => angle
				.raw_bits()?
				.checked_div(integer.raw_bits()?)
				.ok_or(ValueError::DivisionByZero)?,
			_ => return Err(ValueError::Type),
		};
		let width = angle.ty.width().ok_or(ValueError::Type)?;
		ScalarValue::angle_bits(width, bits & width.mask())
	})())
}
