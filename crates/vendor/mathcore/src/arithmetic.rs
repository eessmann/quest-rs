//! Backend-neutral contracts. Numerical enclosure payloads remain with their backends.
use std::cmp::Ordering;
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArithmeticError {
	#[error("nonfinite arithmetic")]
	Nonfinite,
	#[error("invalid arithmetic input: {0}")]
	Interchange(&'static str),
	#[error("outside domain of {0}")]
	Domain(&'static str),
	#[error("arithmetic resource limit: {0}")]
	Budget(&'static str),
}
pub type ArithmeticResult<T> = Result<T, ArithmeticError>;
#[derive(Clone, Debug, PartialEq)]
pub enum ExactConstant {
	/// Exact symbolic pi; no floating approximation is stored.
	Pi,
	Binary64(f64),
	Integer(i64),
	Rational(i64, u64),
	Decimal(String),
	Ratio {
		numerator: String,
		denominator: String,
	},
}
pub trait Backend {
	/// Admit a stored scalar without rounding or changing its value. Custom
	/// backends whose scalar type does not enforce validity must override this
	/// hook; the default trusts their scalar admission contract.
	/// # Errors
	/// Rejects invalid values or values outside the backend's resource policy.
	fn validate(&self, value: &Self::Scalar) -> Result<(), Self::Error> {
		let _ = value;
		Ok(())
	}

	/// Account inline bytes and allocations owned by a scalar.
	/// # Errors
	/// Rejects invalid scalars or accounting overflow.
	fn storage_bytes(&self, value: &Self::Scalar) -> Result<usize, Self::Error> {
		let _ = value;
		Ok(std::mem::size_of::<Self::Scalar>())
	}
	/// Upper bound for an output at this backend's selected precision.
	fn working_scalar_bytes(&self) -> usize {
		std::mem::size_of::<Self::Scalar>()
	}

	type Scalar: Clone;
	type Error: From<ArithmeticError>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn visit(&mut self) -> Result<(), Self::Error> {
		Ok(())
	}
	/// Reserve modeled work for an opaque kernel. The default preserves each
	/// visit's effects and stops at its first error; wrappers may reserve the
	/// whole batch conservatively before forwarding it to an inner backend.
	/// # Errors
	/// Propagates visit failures, including exhausted resource limits.
	fn charge(&mut self, work: usize) -> Result<(), Self::Error> {
		for _ in 0..work {
			self.visit()?;
		}
		Ok(())
	}
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn constant(&mut self, value: &ExactConstant) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn point(&mut self, value: f64) -> Result<Self::Scalar, Self::Error> {
		self.constant(&ExactConstant::Binary64(value))
	}
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn add(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn sub(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn mul(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn div(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn neg(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn exp(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn ln(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn sin(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn cos(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn sqrt(&mut self, a: Self::Scalar) -> Result<Self::Scalar, Self::Error>;
}
/// Arithmetic on finite stored scalar values at a selected precision.
///
/// Implementations must validate finite inputs and implement the declared
/// mathematical arithmetic and functions at their selected precision and rounding
/// policy. Successful operations on the same stored operands denote the same
/// mathematical operation; mutable caches and work counters may affect cost or
/// resource errors, but must not change that meaning. `compare` returns `Equal`
/// exactly when the stored scalars have the same mathematical value (including
/// signed zeros). Polynomial support and numerical algorithms rely on these laws.
pub trait PointBackend: Backend {
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn compare(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<Ordering, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn to_f64(&self, a: &Self::Scalar) -> Result<f64, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn pi(&mut self) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn epsilon(&mut self) -> Result<Self::Scalar, Self::Error>;
	fn precision_bits(&self) -> usize;
}
pub trait EnclosureBackend: Backend {
	type Endpoint: Clone;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn singleton(&mut self, value: &Self::Endpoint) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn lower_endpoint(&self, value: &Self::Scalar) -> Result<Self::Endpoint, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn upper_endpoint(&self, value: &Self::Scalar) -> Result<Self::Endpoint, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn lower(&mut self, value: &Self::Scalar) -> Result<Self::Scalar, Self::Error> {
		let x = self.lower_endpoint(value)?;
		self.singleton(&x)
	}
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn upper(&mut self, value: &Self::Scalar) -> Result<Self::Scalar, Self::Error> {
		let x = self.upper_endpoint(value)?;
		self.singleton(&x)
	}
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn hull(&mut self, a: &Self::Scalar, b: &Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn intersection(
		&mut self,
		a: &Self::Scalar,
		b: &Self::Scalar,
	) -> Result<Option<Self::Scalar>, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn contains_zero(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn is_zero(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn strict_subset(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<bool, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn same(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<bool, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn midpoint(&mut self, a: &Self::Scalar) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	#[allow(
		clippy::type_complexity,
		reason = "Backend returns the two halves of its scalar enclosure"
	)]
	fn bisect(
		&mut self,
		a: &Self::Scalar,
	) -> Result<Option<(Self::Scalar, Self::Scalar)>, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn magnitude_lt_one(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn nonnegative(&self, a: &Self::Scalar) -> Result<bool, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn pi(&mut self) -> Result<Self::Scalar, Self::Error>;
	/// # Errors
	/// Rejects invalid domains, backend failures, or exhausted resource limits.
	fn width_le(
		&mut self,
		a: &Self::Scalar,
		tolerance: &Self::Endpoint,
	) -> Result<bool, Self::Error>;
}

// Shared interchange grammar; backends retain their rounding and precision policy.
#[must_use]
pub fn valid_integer(s: &str) -> bool {
	let s = s
		.strip_prefix('-')
		.or_else(|| s.strip_prefix('+'))
		.unwrap_or(s);
	!s.is_empty() && s.bytes().all(|x| x.is_ascii_digit())
}
#[must_use]
pub fn valid_positive_integer(s: &str) -> bool {
	valid_integer(s) && !s.starts_with('-') && s.bytes().any(|x| matches!(x, b'1'..=b'9'))
}
#[must_use]
pub fn valid_decimal(s: &str) -> bool {
	let s = s
		.strip_prefix('-')
		.or_else(|| s.strip_prefix('+'))
		.unwrap_or(s);
	let mut parts = s.split(['e', 'E']);
	let m = parts.next().unwrap_or("");
	let exp = parts.next();
	if parts.next().is_some() || exp.is_some_and(|s| !valid_integer(s)) {
		return false;
	}
	let mut dots = 0usize;
	let mut digits = 0usize;
	for c in m.bytes() {
		if c == b'.' {
			dots = dots.saturating_add(1);
		} else if c.is_ascii_digit() {
			digits = digits.saturating_add(1);
		} else {
			return false;
		}
	}
	dots <= 1 && digits > 0
}
