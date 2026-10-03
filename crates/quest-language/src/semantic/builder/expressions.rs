use super::{
	Angle, Arithmetic, Bit, Bool, Builder, Classical, E, Expr, Float, Int, Numeric, SemanticError,
	SharedExpression, Uint, expression, foreign, types,
};
use crate::{
	classical::{ScalarValue, Width},
	syntax::BinaryOperator as B,
};
use std::marker::PhantomData;
impl Builder {
	/// # Errors
	/// Rejects expression construction beyond the configured resource limits.
	pub fn boolean(&self, value: bool) -> Result<Expr<Bool>, SemanticError> {
		Ok(self.wrap(SharedExpression::leaf(
			expression(E::Bool(value)),
			self.expression_limits,
		)?))
	}
	/// # Errors
	/// Rejects invalid widths or an out-of-range signed integer.
	pub fn integer<const W: u8>(&self, value: i128) -> Result<Expr<Int<W>>, SemanticError> {
		ScalarValue::signed(Width::new(W)?, value)?;
		self.literal::<Int<W>>(E::Number(value.to_string()))
	}
	/// # Errors
	/// Rejects invalid widths or an out-of-range unsigned integer.
	pub fn unsigned<const W: u8>(&self, value: u64) -> Result<Expr<Uint<W>>, SemanticError> {
		ScalarValue::unsigned(Width::new(W)?, value)?;
		self.literal::<Uint<W>>(E::BitString(format!("{value:064b}")))
	}
	/// # Errors
	/// Rejects unsupported widths or nonfinite floating values.
	pub fn floating<const W: u8>(&self, value: f64) -> Result<Expr<Float<W>>, SemanticError> {
		ScalarValue::floating(types::float_width(W)?, value)?;
		self.literal::<Float<W>>(E::Number(format!("{value:e}")))
	}
	/// # Errors
	/// Rejects invalid widths or bitstrings whose length differs from the marker width.
	pub fn bitstring<const W: u8>(&self, bits: &str) -> Result<Expr<Bit<W>>, SemanticError> {
		let value = ScalarValue::bitstring(bits)?;
		if value.ty() != Bit::<W>::scalar_type()? {
			return Err(SemanticError::new(
				super::super::ErrorKind::Type,
				"bitstring width differs from typed marker",
			));
		}
		self.literal::<Bit<W>>(E::BitString(bits.into()))
	}
	/// # Errors
	/// Rejects widths outside 1..64.
	pub fn angle_bits<const W: u8>(&self, bits: u64) -> Result<Expr<Angle<W>>, SemanticError> {
		ScalarValue::angle_bits(Width::new(W)?, bits)?;
		self.literal::<Angle<W>>(E::BitString(format!(
			"{bits:0width$b}",
			width = usize::from(W)
		)))
	}
	fn literal<T: Classical>(&self, value: E) -> Result<Expr<T>, SemanticError> {
		Ok(self.wrap(
			SharedExpression::leaf(expression(value), self.expression_limits)?
				.cast(T::syntax_type()?, self.expression_limits)?,
		))
	}
}
impl<T: Classical> Expr<T> {
	fn binary<R: Classical>(&self, rhs: &Self, operator: B) -> Result<Expr<R>, SemanticError> {
		if self.owner != rhs.owner {
			return Err(foreign());
		}
		let result = T::scalar_type()?.binary_result(operator, T::scalar_type()?)?;
		if result != R::scalar_type()? {
			return Err(SemanticError::new(
				super::super::ErrorKind::Type,
				"operator result differs from typed marker",
			));
		}
		Ok(Expr {
			owner: self.owner,
			expression: self
				.expression
				.binary(&rhs.expression, operator, self.limits)?,
			limits: self.limits,
			marker: PhantomData,
		})
	}
	/// # Errors
	/// Rejects foreign handles or unsupported equality operand types.
	pub fn equal(&self, rhs: &Self) -> Result<Expr<Bool>, SemanticError> {
		self.binary(rhs, B::Equal)
	}
	/// An explicit checked cast; conversion failures remain observable at execution.
	/// # Errors
	/// Rejects invalid target widths.
	pub fn cast<R: Classical>(&self) -> Result<Expr<R>, SemanticError> {
		Ok(Expr {
			owner: self.owner,
			expression: self.expression.cast(R::syntax_type()?, self.limits)?,
			limits: self.limits,
			marker: PhantomData,
		})
	}
}
impl<T: Numeric> Expr<T> {
	/// # Errors
	/// Rejects foreign handles or unsupported typed addition.
	pub fn add(&self, rhs: &Self) -> Result<Self, SemanticError> {
		self.binary(rhs, B::Add)
	}
	/// # Errors
	/// Rejects foreign handles or unsupported typed subtraction.
	pub fn subtract(&self, rhs: &Self) -> Result<Self, SemanticError> {
		self.binary(rhs, B::Subtract)
	}
	/// # Errors
	/// Rejects foreign handles or unsupported typed comparison.
	pub fn less(&self, rhs: &Self) -> Result<Expr<Bool>, SemanticError> {
		self.binary(rhs, B::Less)
	}
}
impl<T: Arithmetic> Expr<T> {
	/// # Errors
	/// Rejects foreign handles or unsupported typed multiplication.
	pub fn multiply(&self, rhs: &Self) -> Result<Self, SemanticError> {
		self.binary(rhs, B::Multiply)
	}
	/// # Errors
	/// Rejects foreign handles or unsupported typed division.
	pub fn divide(&self, rhs: &Self) -> Result<Self, SemanticError> {
		self.binary(rhs, B::Divide)
	}
}
impl Expr<Bool> {
	/// # Errors
	/// Rejects foreign handles. The admitted program evaluates the right side only when needed.
	pub fn and(&self, rhs: &Self) -> Result<Self, SemanticError> {
		self.binary(rhs, B::And)
	}
	/// # Errors
	/// Rejects foreign handles. The admitted program evaluates the right side only when needed.
	pub fn or(&self, rhs: &Self) -> Result<Self, SemanticError> {
		self.binary(rhs, B::Or)
	}
}
