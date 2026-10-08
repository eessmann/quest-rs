#![feature(const_trait_impl, generic_const_exprs)]
#![allow(incomplete_features)]
#![forbid(unsafe_code)]
//! Typed binary64 polynomials with explicit basis, support and numerical limits.
//!
//! Coefficients are immutable and stored in increasing basis order. A Laurent
//! basis retains its signed exponent offset. Conversion is an explicit cold
//! operation; evaluation never selects a different precision backend.

pub use quest_numerics::Interval;
pub mod typed;
pub use quest_numerics::ad::{First, Gradient, Jet};
pub use typed::{Expression, ExpressionMetadata, StaticExpression};
mod exact_target;
mod function;
pub use exact_target::ExactMonomialTarget;
pub use function::{
	AdmittedFunction, Assumed, AssumedFunction, ConsistencyAssumption, Function, GenericFunction,
	Structural, System, VectorExpression,
};
mod norm;
pub use norm::{NormDomain, NormEvidence, NormOptions, NormOutcome, NormStatus};
mod analysis;
pub use analysis::{Conversion, Even, Odd, Parity, ParityPolynomial};
mod remez;
pub use remez::*;
mod basis;
pub use basis::{Basis, Chebyshev, Hermite, Jacobi, Laguerre, Laurent, Monomial};
mod polynomial;
pub use num_complex::Complex64;
pub use polynomial::{DynamicShape, Polynomial, Shape, StaticShape};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
	#[error("polynomial value or arithmetic result is not finite")]
	NonFinite,
	#[error(transparent)]
	Arithmetic(#[from] quest_numerics::arithmetic::ArithmeticError),
	#[error("coefficient shape mismatch: expected {expected}, got {actual}")]
	Shape { expected: usize, actual: usize },
	#[error(transparent)]
	Interval(#[from] quest_numerics::Error),
	#[error("requested parity is not satisfied")]
	Parity,
	#[error("conversion is unsupported for this support")]
	UnsupportedConversion,
	#[error("numerical enclosure could not be established: {0}")]
	NotEstablished(&'static str),
	#[error("basis parameters are outside their mathematical domain")]
	BasisParameters,
	#[error("polynomial operation is undefined at the requested argument")]
	Domain,
	#[error("signed polynomial support overflows")]
	SupportOverflow,
	#[error("polynomial resource budget exceeded: {0}")]
	Budget(&'static str),
	#[error("operation requires real coefficients")]
	NotReal,
}

/// Transitional name for the shared operation configuration.
pub type Limits = quest_numerics::OperationLimits;
pub use quest_numerics::{
	OperationLimits, OperationResources, ResourceLimits, ResourceReport, ShapeLimits,
};
fn check_storage(limits: Limits, count: usize, copies: usize) -> Result<()> {
	limits
		.shapes
		.coefficients(count)
		.map_err(quest_numerics::Error::from)?;
	let bytes = count
		.checked_mul(size_of::<Complex64>())
		.and_then(|n| n.checked_mul(copies))
		.ok_or(Error::Budget("coefficient storage overflow"))?;
	if bytes > limits.resources.max_peak_bytes
		|| i32::try_from(count).is_err()
		|| isize::try_from(bytes).is_err()
	{
		return Err(Error::Budget("coefficient storage"));
	}
	Ok(())
}

const fn finite(value: Complex64) -> Result<Complex64> {
	if value.re.is_finite() && value.im.is_finite() {
		Ok(value)
	} else {
		Err(Error::NonFinite)
	}
}

fn zeros(count: usize, limits: Limits) -> Result<Vec<Complex64>> {
	check_storage(limits, count, 1)?;
	let mut values = Vec::new();
	values
		.try_reserve_exact(count)
		.map_err(|_| Error::Budget("coefficient allocation"))?;
	values.resize(count, Complex64::new(0.0, 0.0));
	Ok(values)
}

impl From<mathcore::arithmetic::ArithmeticError> for Error {
	fn from(error: mathcore::arithmetic::ArithmeticError) -> Self {
		Self::Arithmetic(error.into())
	}
}
