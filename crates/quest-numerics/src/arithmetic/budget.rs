#![allow(
	clippy::type_complexity,
	reason = "Statically paired backend scalar and endpoint types"
)]
//! Count backend calls and reserve modeled opaque-kernel work. This is not a
//! bound on transcendental-library internals or arbitrary callback code. A bulk
//! reservation remains charged if a later inner backend rejects the batch.
use super::{ArithmeticError, Backend, EnclosureBackend, ExactConstant, PointBackend};
use std::{cell::Cell, cmp::Ordering};
#[derive(Debug)]
pub struct Budget {
	used: Cell<usize>,
	limit: usize,
}
impl Budget {
	#[must_use]
	pub const fn new(limit: usize) -> Self {
		Self {
			used: Cell::new(0),
			limit,
		}
	}
	#[must_use]
	pub const fn used(&self) -> usize {
		self.used.get()
	}
	/// # Errors
	/// Rejects count overflow or exhaustion of the admitted limit.
	pub fn charge(&self, amount: usize) -> Result<(), ArithmeticError> {
		let used = self
			.used
			.get()
			.checked_add(amount)
			.ok_or(ArithmeticError::Budget("backend work"))?;
		if used > self.limit {
			return Err(ArithmeticError::Budget("backend work"));
		}
		self.used.set(used);
		Ok(())
	}
}
pub struct BudgetedBackend<'a, B> {
	inner: &'a mut B,
	work: &'a Budget,
}
macro_rules! binary {($($name:ident),*)=>{$(fn $name(&mut self,a:Self::Scalar,b:Self::Scalar)->Result<Self::Scalar,Self::Error>{self.work.charge(1)?;self.inner.$name(a,b)})*};}
macro_rules! unary {($($name:ident),*)=>{$(fn $name(&mut self,a:Self::Scalar)->Result<Self::Scalar,Self::Error>{self.work.charge(1)?;self.inner.$name(a)})*};}
impl<B: Backend> Backend for BudgetedBackend<'_, B> {
	fn validate(&self, value: &Self::Scalar) -> Result<(), Self::Error> {
		self.work.charge(1)?;
		self.inner.validate(value)
	}

	fn storage_bytes(&self, x: &Self::Scalar) -> Result<usize, Self::Error> {
		self.inner.storage_bytes(x)
	}
	fn working_scalar_bytes(&self) -> usize {
		self.inner.working_scalar_bytes()
	}

	type Scalar = B::Scalar;
	type Error = B::Error;
	fn visit(&mut self) -> Result<(), Self::Error> {
		self.work.charge(1)?;
		self.inner.visit()
	}
	fn charge(&mut self, work: usize) -> Result<(), Self::Error> {
		self.work.charge(work)?;
		self.inner.charge(work)
	}
	fn constant(&mut self, x: &ExactConstant) -> Result<Self::Scalar, Self::Error> {
		self.work.charge(1)?;
		self.inner.constant(x)
	}
	binary!(add, sub, mul, div);
	unary!(neg, exp, ln, sin, cos, sqrt);
}
impl<B: PointBackend> PointBackend for BudgetedBackend<'_, B> {
	fn compare(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<Ordering, Self::Error> {
		self.work.charge(1)?;
		self.inner.compare(a, b)
	}
	fn to_f64(&self, a: &Self::Scalar) -> Result<f64, Self::Error> {
		self.work.charge(1)?;
		self.inner.to_f64(a)
	}
	fn pi(&mut self) -> Result<Self::Scalar, Self::Error> {
		self.work.charge(1)?;
		self.inner.pi()
	}
	fn epsilon(&mut self) -> Result<Self::Scalar, Self::Error> {
		self.work.charge(1)?;
		self.inner.epsilon()
	}
	fn precision_bits(&self) -> usize {
		self.inner.precision_bits()
	}
}
impl<B: EnclosureBackend> EnclosureBackend for BudgetedBackend<'_, B> {
	type Endpoint = B::Endpoint;
	fn singleton(&mut self, x: &Self::Endpoint) -> Result<Self::Scalar, Self::Error> {
		self.work.charge(1)?;
		self.inner.singleton(x)
	}
	fn lower_endpoint(&self, x: &Self::Scalar) -> Result<Self::Endpoint, Self::Error> {
		self.work.charge(1)?;
		self.inner.lower_endpoint(x)
	}
	fn upper_endpoint(&self, x: &Self::Scalar) -> Result<Self::Endpoint, Self::Error> {
		self.work.charge(1)?;
		self.inner.upper_endpoint(x)
	}
	fn hull(&mut self, a: &Self::Scalar, b: &Self::Scalar) -> Result<Self::Scalar, Self::Error> {
		self.work.charge(1)?;
		self.inner.hull(a, b)
	}
	fn intersection(
		&mut self,
		a: &Self::Scalar,
		b: &Self::Scalar,
	) -> Result<Option<Self::Scalar>, Self::Error> {
		self.work.charge(1)?;
		self.inner.intersection(a, b)
	}
	fn contains_zero(&self, x: &Self::Scalar) -> Result<bool, Self::Error> {
		self.work.charge(1)?;
		self.inner.contains_zero(x)
	}
	fn is_zero(&self, x: &Self::Scalar) -> Result<bool, Self::Error> {
		self.work.charge(1)?;
		self.inner.is_zero(x)
	}
	fn strict_subset(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<bool, Self::Error> {
		self.work.charge(1)?;
		self.inner.strict_subset(a, b)
	}
	fn same(&self, a: &Self::Scalar, b: &Self::Scalar) -> Result<bool, Self::Error> {
		self.work.charge(1)?;
		self.inner.same(a, b)
	}
	fn midpoint(&mut self, x: &Self::Scalar) -> Result<Self::Scalar, Self::Error> {
		self.work.charge(1)?;
		self.inner.midpoint(x)
	}
	fn bisect(
		&mut self,
		x: &Self::Scalar,
	) -> Result<Option<(Self::Scalar, Self::Scalar)>, Self::Error> {
		self.work.charge(1)?;
		self.inner.bisect(x)
	}
	fn magnitude_lt_one(&self, x: &Self::Scalar) -> Result<bool, Self::Error> {
		self.work.charge(1)?;
		self.inner.magnitude_lt_one(x)
	}
	fn nonnegative(&self, x: &Self::Scalar) -> Result<bool, Self::Error> {
		self.work.charge(1)?;
		self.inner.nonnegative(x)
	}
	fn pi(&mut self) -> Result<Self::Scalar, Self::Error> {
		self.work.charge(1)?;
		self.inner.pi()
	}

	fn width_le(&mut self, x: &Self::Scalar, t: &Self::Endpoint) -> Result<bool, Self::Error> {
		self.work.charge(1)?;
		self.inner.width_le(x, t)
	}
}

impl<'a, B> BudgetedBackend<'a, B> {
	pub const fn new(inner: &'a mut B, budget: &'a Budget) -> Self {
		Self {
			inner,
			work: budget,
		}
	}
}
impl<B: super::CertifyingBackend> super::sealed::Sealed for BudgetedBackend<'_, B> {}
impl<B: super::CertifyingBackend> super::CertifyingBackend for BudgetedBackend<'_, B> {}
