#![allow(
	clippy::needless_pass_by_value,
	reason = "AD owns derivative seeds and follows backend ownership"
)]
//! Structural automatic differentiation selected at compile time.
use crate::arithmetic::{Backend, ExactConstant};
#[derive(Clone, Debug)]
pub struct Jet<T> {
	pub value: T,
	pub first: T,
	pub second: T,
}
#[derive(Clone, Debug)]
pub struct First<T> {
	pub value: T,
	pub first: T,
}
#[derive(Clone, Debug)]
pub struct Gradient<T, const N: usize> {
	pub value: T,
	pub gradient: crate::shapes::Matrix<T, 1, N>,
}
/// Lift any arithmetic backend to value, first derivative and second derivative.
/// Differentiation is structural; the backend still controls rounding and precision.
#[derive(Debug)]
pub struct JetBackend<'a, B: Backend>(pub &'a mut B);
impl<B: Backend> JetBackend<'_, B> {
	/// # Errors
	/// Rejects invalid seeds and propagates backend constant failures.
	pub fn variable(&mut self, value: B::Scalar) -> std::result::Result<Jet<B::Scalar>, B::Error> {
		self.0.validate(&value)?;
		Ok(Jet {
			value,
			first: self.0.point(1.0)?,
			second: self.0.point(0.0)?,
		})
	}
	fn chain(
		&mut self,
		a: Jet<B::Scalar>,
		value: B::Scalar,
		first: B::Scalar,
		second: B::Scalar,
	) -> std::result::Result<Jet<B::Scalar>, B::Error> {
		let d = self.0.mul(first.clone(), a.first.clone())?;
		let d2 = self.0.mul(second, a.first.clone())?;
		let d2 = self.0.mul(d2, a.first)?;
		let term = self.0.mul(first, a.second)?;
		Ok(Jet {
			value,
			first: d,
			second: self.0.add(d2, term)?,
		})
	}
	pub(crate) fn recip(
		&mut self,
		a: Jet<B::Scalar>,
	) -> std::result::Result<Jet<B::Scalar>, B::Error> {
		let one = self.0.point(1.0)?;
		let v = self.0.div(one, a.value.clone())?;
		let v2 = self.0.mul(v.clone(), v.clone())?;
		let first = self.0.neg(v2.clone())?;
		let second = self.0.mul(v2, v.clone())?;
		let two = self.0.point(2.0)?;
		let second = self.0.mul(two, second)?;
		self.chain(a, v, first, second)
	}
}
impl<B: Backend> Backend for JetBackend<'_, B> {
	fn validate(&self, value: &Self::Scalar) -> Result<(), Self::Error> {
		self.0.validate(&value.value)?;
		self.0.validate(&value.first)?;
		self.0.validate(&value.second)
	}

	fn storage_bytes(&self, x: &Self::Scalar) -> Result<usize, B::Error> {
		let mut n = 0_usize;
		for value in [&x.value, &x.first, &x.second] {
			n = n
				.checked_add(self.0.storage_bytes(value)?)
				.ok_or(mathcore::arithmetic::ArithmeticError::Budget("jet storage"))?;
		}
		Ok(n)
	}
	fn working_scalar_bytes(&self) -> usize {
		self.0.working_scalar_bytes().saturating_mul(3)
	}

	type Scalar = Jet<B::Scalar>;
	type Error = B::Error;
	fn visit(&mut self) -> std::result::Result<(), Self::Error> {
		self.0.visit()
	}
	fn charge(&mut self, work: usize) -> Result<(), Self::Error> {
		self.0.charge(work)
	}
	fn constant(&mut self, x: &ExactConstant) -> std::result::Result<Self::Scalar, Self::Error> {
		Ok(Jet {
			value: self.0.constant(x)?,
			first: self.0.point(0.0)?,
			second: self.0.point(0.0)?,
		})
	}
	#[inline]
	fn add(
		&mut self,
		a: Self::Scalar,
		b: Self::Scalar,
	) -> std::result::Result<Self::Scalar, Self::Error> {
		Ok(Jet {
			value: self.0.add(a.value, b.value)?,
			first: self.0.add(a.first, b.first)?,
			second: self.0.add(a.second, b.second)?,
		})
	}
	fn sub(
		&mut self,
		a: Self::Scalar,
		b: Self::Scalar,
	) -> std::result::Result<Self::Scalar, Self::Error> {
		let b = self.neg(b)?;
		self.add(a, b)
	}
	fn neg(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
		Ok(Jet {
			value: self.0.neg(a.value)?,
			first: self.0.neg(a.first)?,
			second: self.0.neg(a.second)?,
		})
	}
	#[expect(
		clippy::many_single_char_names,
		reason = "Short local names follow the derivative product rule"
	)]
	#[inline]
	fn mul(
		&mut self,
		a: Self::Scalar,
		b: Self::Scalar,
	) -> std::result::Result<Self::Scalar, Self::Error> {
		let value = self.0.mul(a.value.clone(), b.value.clone())?;
		let l = self.0.mul(a.first.clone(), b.value.clone())?;
		let r = self.0.mul(a.value.clone(), b.first.clone())?;
		let first = self.0.add(l, r)?;
		let l = self.0.mul(a.second, b.value)?;
		let m = self.0.mul(a.first, b.first)?;
		let two = self.0.point(2.0)?;
		let m = self.0.mul(two, m)?;
		let r = self.0.mul(a.value, b.second)?;
		let second = self.0.add(l, m)?;
		Ok(Jet {
			value,
			first,
			second: self.0.add(second, r)?,
		})
	}
	fn div(
		&mut self,
		a: Self::Scalar,
		b: Self::Scalar,
	) -> std::result::Result<Self::Scalar, Self::Error> {
		let b = self.recip(b)?;
		self.mul(a, b)
	}
	fn exp(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
		let v = self.0.exp(a.value.clone())?;
		self.chain(a, v.clone(), v.clone(), v)
	}
	fn ln(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
		let v = self.0.ln(a.value.clone())?;
		let one = self.0.point(1.0)?;
		let d = self.0.div(one, a.value.clone())?;
		let d2 = self.0.mul(d.clone(), d.clone())?;
		let d2 = self.0.neg(d2)?;
		self.chain(a, v, d, d2)
	}
	fn sin(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
		let v = self.0.sin(a.value.clone())?;
		let d = self.0.cos(a.value.clone())?;
		let d2 = self.0.neg(v.clone())?;
		self.chain(a, v, d, d2)
	}
	fn cos(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
		let v = self.0.cos(a.value.clone())?;
		let d = self.0.sin(a.value.clone())?;
		let d = self.0.neg(d)?;
		let d2 = self.0.neg(v.clone())?;
		self.chain(a, v, d, d2)
	}
	fn sqrt(&mut self, a: Self::Scalar) -> std::result::Result<Self::Scalar, Self::Error> {
		let v = self.0.sqrt(a.value.clone())?;
		let half = self.0.point(0.5)?;
		let d = self.0.div(half, v.clone())?;
		let v3 = self.0.mul(v.clone(), v.clone())?;
		let v3 = self.0.mul(v3, v.clone())?;
		let quarter = self.0.point(-0.25)?;
		let d2 = self.0.div(quarter, v3)?;
		self.chain(a, v, d, d2)
	}
}
/// One derivative per independent variable, with heap-owned static shape.
pub struct GradientBackend<'a, B: Backend, const N: usize>(pub &'a mut B);
impl<B: Backend, const N: usize> GradientBackend<'_, B, N> {
	fn zeros(&mut self) -> Result<crate::shapes::Matrix<B::Scalar, 1, N>, B::Error> {
		const { assert!(N > 0, "zero gradient dimension") };
		let zero = self.0.point(0.0)?;
		let mut data = Vec::new();
		data.try_reserve_exact(N)
			.map_err(|_| mathcore::arithmetic::ArithmeticError::Budget("gradient allocation"))?;
		data.resize(N, zero);
		Ok(crate::shapes::Matrix::from_vec(data)?)
	}
	/// # Errors
	/// Rejects invalid seed indices, allocations, or backend arithmetic.
	pub fn variable(
		&mut self,
		value: B::Scalar,
		index: usize,
	) -> Result<Gradient<B::Scalar, N>, B::Error> {
		if index >= N {
			return Err(mathcore::arithmetic::ArithmeticError::Domain("gradient index").into());
		}
		self.0.validate(&value)?;
		let mut gradient = self.zeros()?;
		*gradient.get_mut(0, index)? = self.0.point(1.0)?;
		Ok(Gradient { value, gradient })
	}
	fn chain(
		&mut self,
		a: Gradient<B::Scalar, N>,
		value: B::Scalar,
		d: B::Scalar,
	) -> Result<Gradient<B::Scalar, N>, B::Error> {
		let mut gradient = a.gradient;
		for x in gradient.as_mut_slice() {
			*x = self.0.mul(d.clone(), x.clone())?;
		}
		Ok(Gradient { value, gradient })
	}
}
impl<B: Backend, const N: usize> Backend for GradientBackend<'_, B, N> {
	fn validate(&self, value: &Self::Scalar) -> Result<(), Self::Error> {
		self.0.validate(&value.value)?;
		for derivative in value.gradient.as_slice() {
			self.0.validate(derivative)?;
		}
		Ok(())
	}

	type Scalar = Gradient<B::Scalar, N>;
	type Error = B::Error;
	fn storage_bytes(&self, x: &Self::Scalar) -> Result<usize, B::Error> {
		let mut n = self
			.0
			.storage_bytes(&x.value)?
			.checked_add(std::mem::size_of::<Vec<B::Scalar>>())
			.ok_or(mathcore::arithmetic::ArithmeticError::Budget(
				"gradient storage",
			))?;
		for value in x.gradient.as_slice() {
			n = n.checked_add(self.0.storage_bytes(value)?).ok_or(
				mathcore::arithmetic::ArithmeticError::Budget("gradient storage"),
			)?;
		}
		Ok(n)
	}
	fn working_scalar_bytes(&self) -> usize {
		self.0
			.working_scalar_bytes()
			.saturating_mul(N.saturating_add(1))
			.saturating_add(std::mem::size_of::<Vec<B::Scalar>>())
	}
	fn visit(&mut self) -> Result<(), B::Error> {
		self.0.visit()
	}
	fn charge(&mut self, work: usize) -> Result<(), Self::Error> {
		self.0.charge(work)
	}
	fn constant(&mut self, x: &ExactConstant) -> Result<Self::Scalar, B::Error> {
		let value = self.0.constant(x)?;
		Ok(Gradient {
			value,
			gradient: self.zeros()?,
		})
	}
	fn add(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let value = self.0.add(a.value, b.value)?;
		let mut gradient = a.gradient;
		for (x, y) in gradient
			.as_mut_slice()
			.iter_mut()
			.zip(b.gradient.into_vec())
		{
			*x = self.0.add(x.clone(), y)?;
		}
		Ok(Gradient { value, gradient })
	}
	fn sub(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let b = self.neg(b)?;
		self.add(a, b)
	}
	fn neg(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let value = self.0.neg(a.value)?;
		let mut gradient = a.gradient;
		for x in gradient.as_mut_slice() {
			*x = self.0.neg(x.clone())?;
		}
		Ok(Gradient { value, gradient })
	}
	fn mul(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let value = self.0.mul(a.value.clone(), b.value.clone())?;
		let mut gradient = a.gradient;
		for (x, y) in gradient
			.as_mut_slice()
			.iter_mut()
			.zip(b.gradient.into_vec())
		{
			let l = self.0.mul(x.clone(), b.value.clone())?;
			let r = self.0.mul(a.value.clone(), y)?;
			*x = self.0.add(l, r)?;
		}
		Ok(Gradient { value, gradient })
	}
	fn div(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let one = self.0.point(1.0)?;
		let value = self.0.div(one, b.value.clone())?;
		let derivative = self.0.mul(value.clone(), value.clone())?;
		let derivative = self.0.neg(derivative)?;
		let inv = self.chain(b, value, derivative)?;
		self.mul(a, inv)
	}
	fn exp(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.exp(a.value.clone())?;
		self.chain(a, v.clone(), v)
	}
	fn ln(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.ln(a.value.clone())?;
		let one = self.0.point(1.0)?;
		let d = self.0.div(one, a.value.clone())?;
		self.chain(a, v, d)
	}
	fn sin(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.sin(a.value.clone())?;
		let d = self.0.cos(a.value.clone())?;
		self.chain(a, v, d)
	}
	fn cos(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.cos(a.value.clone())?;
		let d = self.0.sin(a.value.clone())?;
		let d = self.0.neg(d)?;
		self.chain(a, v, d)
	}
	fn sqrt(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.sqrt(a.value.clone())?;
		let half = self.0.point(0.5)?;
		let d = self.0.div(half, v.clone())?;
		self.chain(a, v, d)
	}
}
/// First order requires no heap-owned derivative vector.
pub struct FirstBackend<'a, B: Backend>(pub &'a mut B);
impl<B: Backend> FirstBackend<'_, B> {
	/// # Errors
	/// Rejects invalid seeds and propagates backend constant errors.
	pub fn variable(&mut self, value: B::Scalar) -> Result<First<B::Scalar>, B::Error> {
		self.0.validate(&value)?;
		Ok(First {
			value,
			first: self.0.point(1.0)?,
		})
	}
	fn chain(
		&mut self,
		a: First<B::Scalar>,
		value: B::Scalar,
		d: B::Scalar,
	) -> Result<First<B::Scalar>, B::Error> {
		Ok(First {
			value,
			first: self.0.mul(d, a.first)?,
		})
	}
}
impl<B: Backend> Backend for FirstBackend<'_, B> {
	fn validate(&self, value: &Self::Scalar) -> Result<(), Self::Error> {
		self.0.validate(&value.value)?;
		self.0.validate(&value.first)
	}

	type Scalar = First<B::Scalar>;
	type Error = B::Error;
	fn storage_bytes(&self, x: &Self::Scalar) -> Result<usize, B::Error> {
		self.0
			.storage_bytes(&x.value)?
			.checked_add(self.0.storage_bytes(&x.first)?)
			.ok_or_else(|| {
				mathcore::arithmetic::ArithmeticError::Budget("first derivative storage").into()
			})
	}
	fn working_scalar_bytes(&self) -> usize {
		self.0.working_scalar_bytes().saturating_mul(2)
	}
	fn visit(&mut self) -> Result<(), B::Error> {
		self.0.visit()
	}
	fn charge(&mut self, work: usize) -> Result<(), Self::Error> {
		self.0.charge(work)
	}
	fn constant(&mut self, x: &ExactConstant) -> Result<Self::Scalar, B::Error> {
		Ok(First {
			value: self.0.constant(x)?,
			first: self.0.point(0.0)?,
		})
	}
	fn add(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		Ok(First {
			value: self.0.add(a.value, b.value)?,
			first: self.0.add(a.first, b.first)?,
		})
	}
	fn sub(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		Ok(First {
			value: self.0.sub(a.value, b.value)?,
			first: self.0.sub(a.first, b.first)?,
		})
	}
	fn neg(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		Ok(First {
			value: self.0.neg(a.value)?,
			first: self.0.neg(a.first)?,
		})
	}
	fn mul(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let value = self.0.mul(a.value.clone(), b.value.clone())?;
		let left = self.0.mul(a.first, b.value)?;
		let right = self.0.mul(a.value, b.first)?;
		Ok(First {
			value,
			first: self.0.add(left, right)?,
		})
	}
	fn div(&mut self, a: Self::Scalar, b: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let one = self.0.point(1.0)?;
		let value = self.0.div(one, b.value.clone())?;
		let d = self.0.mul(value.clone(), value.clone())?;
		let d = self.0.neg(d)?;
		let inverse = self.chain(b, value, d)?;
		self.mul(a, inverse)
	}
	fn exp(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.exp(a.value.clone())?;
		self.chain(a, v.clone(), v)
	}
	fn ln(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.ln(a.value.clone())?;
		let one = self.0.point(1.0)?;
		let d = self.0.div(one, a.value.clone())?;
		self.chain(a, v, d)
	}
	fn sin(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.sin(a.value.clone())?;
		let d = self.0.cos(a.value.clone())?;
		self.chain(a, v, d)
	}
	fn cos(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.cos(a.value.clone())?;
		let d = self.0.sin(a.value.clone())?;
		let d = self.0.neg(d)?;
		self.chain(a, v, d)
	}
	fn sqrt(&mut self, a: Self::Scalar) -> Result<Self::Scalar, B::Error> {
		let v = self.0.sqrt(a.value.clone())?;
		let half = self.0.point(0.5)?;
		let d = self.0.div(half, v.clone())?;
		self.chain(a, v, d)
	}
}
/// Rectangular static Jacobian from one structural evaluation.
#[derive(Clone, Debug)]
pub struct Jacobian<T, const M: usize, const N: usize> {
	pub value: [T; M],
	pub derivative: crate::shapes::Matrix<T, M, N>,
}
/// Admission for heap-owned seeds, derivative rows and output matrix.
#[derive(Clone, Copy, Debug)]
pub struct JacobianLimits {
	pub max_inputs: usize,
	pub max_outputs: usize,
	pub max_bytes: usize,
}
impl Default for JacobianLimits {
	fn default() -> Self {
		Self {
			max_inputs: 4096,
			max_outputs: 4096,
			max_bytes: 64 * 1024 * 1024,
		}
	}
}
/// Seed N independent variables and evaluate M outputs using first-order AD.
///
/// Seeds, gradient rows and rectangular derivative data live on the heap.
/// Input/output value arrays remain O(N)/O(M) logical static I/O. Admission
/// includes scalar-owned limbs and matrix headers; callback expression
/// temporaries and opaque backend workspaces remain caller-owned.
/// # Errors
/// Rejects shape, dimension, storage or callback-output mismatch, allocation
/// failures, and backend/callback failures.
pub fn jacobian<B: Backend, F, const N: usize, const M: usize>(
	backend: &mut B,
	input: [B::Scalar; N],
	limits: JacobianLimits,
	function: F,
) -> Result<Jacobian<B::Scalar, M, N>, B::Error>
where
	F: FnOnce(
		&mut GradientBackend<'_, B, N>,
		&[Gradient<B::Scalar, N>],
	) -> Result<Vec<Gradient<B::Scalar, N>>, B::Error>,
{
	const { assert!(N > 0 && M > 0, "zero Jacobian dimension") };
	if N > limits.max_inputs || M > limits.max_outputs {
		return Err(mathcore::arithmetic::ArithmeticError::Budget("Jacobian dimension").into());
	}
	let mut scalar_bytes = backend.working_scalar_bytes();
	for value in &input {
		backend.validate(value)?;
		scalar_bytes = scalar_bytes.max(backend.storage_bytes(value)?);
	}
	let count = N
		.checked_mul(M)
		.ok_or(mathcore::arithmetic::ArithmeticError::Budget(
			"Jacobian shape",
		))?;
	let cells = N
		.checked_mul(N)
		.and_then(|n| n.checked_add(count.checked_mul(2)?))
		.and_then(|n| n.checked_add(N.checked_add(M)?.checked_mul(3)?))
		.ok_or(mathcore::arithmetic::ArithmeticError::Budget(
			"Jacobian shape",
		))?;
	let bytes = cells
		.checked_mul(scalar_bytes)
		.and_then(|n| {
			n.checked_add(
				N.checked_add(M)?
					.checked_mul(std::mem::size_of::<Vec<B::Scalar>>())?,
			)
		})
		.ok_or(mathcore::arithmetic::ArithmeticError::Budget(
			"Jacobian storage",
		))?;
	if bytes > limits.max_bytes {
		return Err(mathcore::arithmetic::ArithmeticError::Budget("Jacobian storage").into());
	}
	let mut seeds = Vec::new();
	seeds
		.try_reserve_exact(N)
		.map_err(|_| mathcore::arithmetic::ArithmeticError::Budget("Jacobian seeds"))?;
	let mut ad = GradientBackend(backend);
	for (index, value) in input.into_iter().enumerate() {
		seeds.push(ad.variable(value, index)?);
	}
	let outputs = function(&mut ad, &seeds)?;
	if outputs.len() != M {
		return Err(mathcore::arithmetic::ArithmeticError::Domain("Jacobian output shape").into());
	}
	let mut actual = 0_usize;
	for output in &outputs {
		ad.validate(output)?;
		actual = actual.checked_add(ad.storage_bytes(output)?).ok_or(
			mathcore::arithmetic::ArithmeticError::Budget("Jacobian output storage"),
		)?;
	}
	// Higher-precision values returned by an open callback must also be admitted.
	if actual.saturating_mul(2).saturating_add(bytes) > limits.max_bytes {
		return Err(
			mathcore::arithmetic::ArithmeticError::Budget("Jacobian output storage").into(),
		);
	}
	let mut data = Vec::new();
	data.try_reserve_exact(count)
		.map_err(|_| mathcore::arithmetic::ArithmeticError::Budget("Jacobian allocation"))?;
	let mut values = Vec::new();
	values
		.try_reserve_exact(M)
		.map_err(|_| mathcore::arithmetic::ArithmeticError::Budget("Jacobian values"))?;
	for output in outputs {
		data.extend(output.gradient.into_vec());
		values.push(output.value);
	}
	let value = values
		.try_into()
		.map_err(|_| mathcore::arithmetic::ArithmeticError::Domain("Jacobian output shape"))?;
	Ok(Jacobian {
		value,
		derivative: crate::shapes::Matrix::from_vec(data)?,
	})
}
