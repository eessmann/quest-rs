//! Ordered binary64 operations shared by typed-language and numerical adapters.
use crate::arithmetic::{ArithmeticError, ArithmeticResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperation {
	Add,
	Subtract,
	Multiply,
	Divide,
	Power,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperation {
	Negate,
	Exp,
	Ln,
	Sin,
	Cos,
	Sqrt,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOperation {
	And,
	Or,
}

/// # Errors
/// Rejects invalid input, unsupported operations, or exhausted resource limits.
pub fn boolean<E>(
	left: bool,
	operation: BooleanOperation,
	right: impl FnOnce() -> Result<bool, E>,
) -> Result<bool, E> {
	match operation {
		BooleanOperation::And if !left => Ok(false),
		BooleanOperation::Or if left => Ok(true),
		_ => right(),
	}
}
/// # Errors
/// Rejects invalid input, unsupported operations, or exhausted resource limits.
pub const fn finite(value: f64) -> ArithmeticResult<f64> {
	if value.is_finite() {
		Ok(value)
	} else {
		Err(ArithmeticError::Nonfinite)
	}
}
/// # Errors
/// Rejects invalid input, unsupported operations, or exhausted resource limits.
pub fn binary(left: f64, operation: BinaryOperation, right: f64) -> ArithmeticResult<f64> {
	finite(left)?;
	finite(right)?;
	finite(match operation {
		BinaryOperation::Add => left + right,
		BinaryOperation::Subtract => left - right,
		BinaryOperation::Multiply => left * right,
		BinaryOperation::Divide if right == 0.0 => return Err(ArithmeticError::Domain("division")),
		BinaryOperation::Divide => left / right,
		BinaryOperation::Power => left.powf(right),
	})
}
/// # Errors
/// Rejects invalid input, unsupported operations, or exhausted resource limits.
pub fn unary(value: f64, operation: UnaryOperation) -> ArithmeticResult<f64> {
	finite(value)?;
	finite(match operation {
		UnaryOperation::Negate => -value,
		UnaryOperation::Exp => value.exp(),
		UnaryOperation::Ln if value <= 0.0 => return Err(ArithmeticError::Domain("ln")),
		UnaryOperation::Ln => value.ln(),
		UnaryOperation::Sin => value.sin(),
		UnaryOperation::Cos => value.cos(),
		UnaryOperation::Sqrt if value < 0.0 => return Err(ArithmeticError::Domain("sqrt")),
		UnaryOperation::Sqrt => value.sqrt(),
	})
}
