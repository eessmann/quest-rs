use crate::{Error, Interval, Result};
use quest_numerics::arithmetic::{
	ArithmeticError, Backend, ExactConstant, F64Backend, Interval64Backend,
};

type RecurrenceResult<A> = std::result::Result<
	(
		<A as Backend>::Scalar,
		<A as Backend>::Scalar,
		<A as Backend>::Scalar,
	),
	<A as Backend>::Error,
>;

mod sealed {
	pub trait Sealed {}
}

/// A three-term basis, with signed support supplied only by Laurent.
pub trait Basis: sealed::Sealed + Clone + std::fmt::Debug {
	/// Compute the three-term recurrence in the selected arithmetic backend.
	/// Integer factors are imported exactly before division or multiplication.
	/// # Errors
	/// Rejects invalid basis orders and propagates checked backend arithmetic.
	fn recurrence_with<A: Backend>(&self, degree: u32, backend: &mut A) -> RecurrenceResult<A>;
	#[doc(hidden)]
	fn recurrence(&self, degree: u32) -> Result<(f64, f64, f64)> {
		Ok(self.recurrence_with(degree, &mut F64Backend)?)
	}
	#[doc(hidden)]
	fn interval_recurrence(&self, degree: u32) -> Result<(Interval, Interval, Interval)> {
		Ok(self.recurrence_with(degree, &mut Interval64Backend)?)
	}
	#[doc(hidden)]
	fn symmetric(&self) -> bool {
		true
	}
	fn offset(&self) -> i32 {
		0
	}
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Monomial;
#[derive(Debug, Clone, Copy, Default)]
pub struct Chebyshev;
#[derive(Debug, Clone, Copy)]
pub struct Laurent {
	offset: i32,
}
impl Laurent {
	#[must_use]
	pub const fn new(offset: i32) -> Self {
		Self { offset }
	}
}
#[derive(Debug, Clone, Copy)]
pub struct Hermite {
	physicists: bool,
}
impl Hermite {
	#[must_use]
	pub const fn physicists() -> Self {
		Self { physicists: true }
	}
	#[must_use]
	pub const fn scale(self) -> f64 {
		if self.physicists { 2.0 } else { 1.0 }
	}
	#[must_use]
	pub const fn probabilists() -> Self {
		Self { physicists: false }
	}
}
#[derive(Debug, Clone, Copy)]
pub struct Laguerre {
	alpha: f64,
}
impl Laguerre {
	/// # Errors
	/// Rejects a nonfinite parameter or alpha at or below minus one.
	pub fn new(alpha: f64) -> Result<Self> {
		if !alpha.is_finite() || alpha <= -1.0 {
			return Err(Error::BasisParameters);
		}
		Ok(Self { alpha })
	}
	#[must_use]
	pub const fn alpha(self) -> f64 {
		self.alpha
	}
}
#[derive(Debug, Clone, Copy)]
pub struct Jacobi {
	alpha: f64,
	beta: f64,
}
impl Jacobi {
	/// # Errors
	/// Rejects nonfinite parameters or parameters at or below minus one.
	pub fn new(alpha: f64, beta: f64) -> Result<Self> {
		if !alpha.is_finite() || !beta.is_finite() || alpha <= -1.0 || beta <= -1.0 {
			return Err(Error::BasisParameters);
		}
		Ok(Self { alpha, beta })
	}
	#[must_use]
	pub const fn parameters(self) -> (f64, f64) {
		(self.alpha, self.beta)
	}
}

impl sealed::Sealed for Monomial {}
impl sealed::Sealed for Chebyshev {}
impl sealed::Sealed for Laurent {}
impl sealed::Sealed for Hermite {}
impl sealed::Sealed for Laguerre {}
impl sealed::Sealed for Jacobi {}
fn integer<A: Backend>(backend: &mut A, value: i64) -> std::result::Result<A::Scalar, A::Error> {
	backend.constant(&ExactConstant::Integer(value))
}
fn monomial<A: Backend>(backend: &mut A) -> RecurrenceResult<A> {
	Ok((
		integer(backend, 1)?,
		integer(backend, 0)?,
		integer(backend, 0)?,
	))
}
impl Basis for Monomial {
	fn recurrence_with<A: Backend>(&self, _: u32, backend: &mut A) -> RecurrenceResult<A> {
		monomial(backend)
	}
}
impl Basis for Laurent {
	fn recurrence_with<A: Backend>(&self, _: u32, backend: &mut A) -> RecurrenceResult<A> {
		monomial(backend)
	}
	fn offset(&self) -> i32 {
		self.offset
	}
}
impl Basis for Chebyshev {
	fn recurrence_with<A: Backend>(&self, degree: u32, backend: &mut A) -> RecurrenceResult<A> {
		if degree <= 1 {
			monomial(backend)
		} else {
			Ok((
				integer(backend, 2)?,
				integer(backend, 0)?,
				integer(backend, 1)?,
			))
		}
	}
}
impl Basis for Hermite {
	fn recurrence_with<A: Backend>(&self, degree: u32, backend: &mut A) -> RecurrenceResult<A> {
		let scale = integer(backend, if self.physicists { 2 } else { 1 })?;
		let previous = integer(backend, i64::from(degree.saturating_sub(1)))?;
		let back = backend.mul(scale.clone(), previous)?;
		Ok((scale, integer(backend, 0)?, back))
	}
}
impl Basis for Laguerre {
	fn symmetric(&self) -> bool {
		false
	}
	fn recurrence_with<A: Backend>(&self, degree: u32, backend: &mut A) -> RecurrenceResult<A> {
		if degree == 0 {
			return Err(ArithmeticError::Domain("Laguerre recurrence order").into());
		}
		let n = integer(backend, i64::from(degree))?;
		let alpha = backend.point(self.alpha)?;
		let one = integer(backend, 1)?;
		let minus_one = integer(backend, -1)?;
		let scale = backend.div(minus_one, n.clone())?;
		let twice = integer(backend, 2)?;
		let twice = backend.mul(twice, n.clone())?;
		let shift = backend.sub(twice, one.clone())?;
		let shift = backend.add(shift, alpha.clone())?;
		let shift = backend.div(shift, n.clone())?;
		let back = if degree == 1 {
			integer(backend, 0)?
		} else {
			let previous = backend.sub(n.clone(), one)?;
			let previous = backend.add(previous, alpha)?;
			backend.div(previous, n)?
		};
		Ok((scale, shift, back))
	}
}
impl Basis for Jacobi {
	#[expect(
		clippy::float_cmp,
		reason = "Exact equality of basis parameters is required for parity"
	)]
	fn symmetric(&self) -> bool {
		self.alpha == self.beta
	}
	fn recurrence_with<A: Backend>(&self, degree: u32, backend: &mut A) -> RecurrenceResult<A> {
		let alpha = backend.point(self.alpha)?;
		let beta = backend.point(self.beta)?;
		let one = integer(backend, 1)?;
		let two = integer(backend, 2)?;
		let sum = backend.add(alpha.clone(), beta.clone())?;
		if degree <= 1 {
			let scale = backend.add(sum, two.clone())?;
			let scale = backend.div(scale, two.clone())?;
			let shift = backend.sub(alpha, beta)?;
			let shift = backend.div(shift, two)?;
			return Ok((scale, shift, integer(backend, 0)?));
		}
		let n = integer(backend, i64::from(degree))?;
		let twice = backend.mul(two.clone(), n.clone())?;
		let twice = backend.add(twice, sum.clone())?;
		let twice_minus_two = backend.sub(twice.clone(), two.clone())?;
		let n_sum = backend.add(n.clone(), sum)?;
		let denominator = backend.mul(two.clone(), n.clone())?;
		let denominator = backend.mul(denominator, n_sum)?;
		let denominator = backend.mul(denominator, twice_minus_two.clone())?;
		let twice_minus_one = backend.sub(twice.clone(), one.clone())?;
		let scale = backend.mul(twice_minus_one.clone(), twice.clone())?;
		let scale = backend.mul(scale, twice_minus_two)?;
		let scale = backend.div(scale, denominator.clone())?;
		let alpha_square = backend.mul(alpha.clone(), alpha.clone())?;
		let beta_square = backend.mul(beta.clone(), beta.clone())?;
		let difference = backend.sub(alpha_square, beta_square)?;
		let shift = backend.mul(twice_minus_one, difference)?;
		let shift = backend.div(shift, denominator.clone())?;
		let n_alpha = backend.add(n.clone(), alpha)?;
		let n_alpha = backend.sub(n_alpha, one.clone())?;
		let n_beta = backend.add(n, beta)?;
		let n_beta = backend.sub(n_beta, one)?;
		let back = backend.mul(two, n_alpha)?;
		let back = backend.mul(back, n_beta)?;
		let back = backend.mul(back, twice)?;
		let back = backend.div(back, denominator)?;
		Ok((scale, shift, back))
	}
}
