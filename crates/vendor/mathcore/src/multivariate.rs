//! Bounded sparse polynomials over exact rational coefficients and scoped symbols.
//! Canonical algebra is mathematical construction, not floating-point reassociation.
use crate::{
	RBig,
	arithmetic::{ArithmeticError as Error, ArithmeticProfile, Backend, ExactConstant},
	identity::Symbol,
	scope::ScopePlan,
};
use dashu_base::BitTest;
use dashu_int::IBig;
use std::{
	collections::BTreeMap,
	ops::{Add, Mul, Neg},
};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Copy)]
pub struct PolynomialLimits {
	pub max_variables: usize,
	pub max_terms: usize,
	pub max_degree: u32,
	pub max_coefficient_bits: usize,
	pub max_bytes: usize,
	pub max_work: usize,
}
impl Default for PolynomialLimits {
	fn default() -> Self {
		Self {
			max_variables: 4096,
			max_terms: 65536,
			max_degree: 64,
			max_coefficient_bits: 16384,
			max_bytes: 64 * 1024 * 1024,
			max_work: 16 * 1024 * 1024,
		}
	}
}
impl PolynomialLimits {
	const fn work(self, count: usize) -> Result<usize> {
		if count > self.max_work {
			Err(Error::Budget("polynomial work"))
		} else {
			Ok(count)
		}
	}
	fn coefficient(self, value: &RBig) -> Result<()> {
		if value
			.numerator()
			.bit_len()
			.max(value.denominator().bit_len())
			> self.max_coefficient_bits
		{
			Err(Error::Budget("polynomial coefficient bits"))
		} else {
			Ok(())
		}
	}
	fn width(self, count: usize) -> Result<()> {
		if count > self.max_coefficient_bits
			|| count
				.div_ceil(8)
				.checked_mul(4)
				.and_then(|b| b.checked_add(size_of::<RBig>()))
				.is_none_or(|b| b > self.max_bytes)
		{
			Err(Error::Budget("polynomial coefficient growth"))
		} else {
			Ok(())
		}
	}
}

#[derive(Debug, Clone)]
pub struct SparsePolynomial {
	symbols: Vec<Symbol>,
	terms: BTreeMap<Vec<u32>, RBig>,
	limits: PolynomialLimits,
	work: usize,
	retained: usize,
}
impl SparsePolynomial {
	pub(crate) fn admit_variables(
		symbols: &[Symbol],
		capacity: usize,
		limits: PolynomialLimits,
	) -> Result<()> {
		let live = Self::base_bytes(capacity)?
			.checked_add(
				symbols
					.len()
					.checked_mul(size_of::<u32>())
					.ok_or(Error::Budget("polynomial storage"))?,
			)
			.ok_or(Error::Budget("polynomial storage"))?;
		// Constructor-owned exponents coexist with the import uniqueness scratch.
		let plan = ScopePlan::new(
			symbols.len(),
			limits.max_variables,
			limits.max_bytes,
			limits.max_work,
			live,
		)?;
		if plan
			.work
			.checked_add(1)
			.is_none_or(|work| work > limits.max_work)
		{
			return Err(Error::Budget("polynomial work"));
		}
		Ok(())
	}
	/// Import bounded terms, reduce duplicates in input order, then remove exact zeros.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn from_terms(
		symbols: Vec<Symbol>,
		terms: impl IntoIterator<Item = (Vec<u32>, RBig)>,
		limits: PolynomialLimits,
	) -> Result<Self> {
		let retained = Self::base_bytes(symbols.capacity())?;
		let plan = ScopePlan::new(
			symbols.len(),
			limits.max_variables,
			limits.max_bytes,
			limits.max_work,
			retained,
		)?;
		drop(plan.validate(&symbols)?);

		let mut output = Self {
			retained,
			symbols,
			terms: BTreeMap::new(),
			limits,
			work: plan.work,
		};
		output.check_storage()?;
		for (powers, value) in terms {
			output.charge(1)?;
			output.insert(powers, value)?;
		}
		Ok(output)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn constant(symbols: Vec<Symbol>, value: RBig, limits: PolynomialLimits) -> Result<Self> {
		Self::admit_variables(&symbols, symbols.capacity(), limits)?;
		let powers = vec![0; symbols.len()];
		Self::from_terms(symbols, [(powers, value)], limits)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn variable(symbols: Vec<Symbol>, index: usize, limits: PolynomialLimits) -> Result<Self> {
		if index >= symbols.len() {
			return Err(Error::Domain("polynomial variable"));
		}
		Self::admit_variables(&symbols, symbols.capacity(), limits)?;
		let mut powers = vec![0; symbols.len()];
		*powers
			.get_mut(index)
			.ok_or(Error::Domain("polynomial variable"))? = 1;
		Self::from_terms(symbols, [(powers, RBig::ONE)], limits)
	}
	#[must_use]
	pub fn symbols(&self) -> &[Symbol] {
		&self.symbols
	}
	pub fn terms(&self) -> impl ExactSizeIterator<Item = (&[u32], &RBig)> {
		self.terms
			.iter()
			.map(|(powers, value)| (powers.as_slice(), value))
	}
	#[must_use]
	pub const fn logical_work(&self) -> usize {
		self.work
	}
	#[must_use]
	pub const fn limits(&self) -> PolynomialLimits {
		self.limits
	}
	#[must_use]
	pub fn degree(&self) -> u32 {
		self.terms
			.keys()
			.filter_map(|powers| powers.iter().try_fold(0_u32, |a, b| a.checked_add(*b)))
			.max()
			.unwrap_or(0)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub const fn retained_bytes(&self) -> Result<usize> {
		Ok(self.retained)
	}
	fn base_bytes(capacity: usize) -> Result<usize> {
		capacity
			.checked_mul(size_of::<Symbol>())
			.and_then(|b| b.checked_add(size_of::<Self>()))
			.ok_or(Error::Budget("polynomial storage"))
	}
	fn term_bytes(powers: &Vec<u32>, value: &RBig) -> Result<usize> {
		let limbs = value
			.numerator()
			.bit_len()
			.checked_add(value.denominator().bit_len())
			.ok_or(Error::Budget("polynomial storage"))?
			.div_ceil(8);
		powers
			.capacity()
			.checked_mul(size_of::<u32>())
			.and_then(|b| b.checked_add(limbs))
			.and_then(|b| b.checked_add(128))
			.ok_or(Error::Budget("polynomial storage"))
	}

	fn check_storage(&self) -> Result<()> {
		if self.terms.len() > self.limits.max_terms
			|| self.retained_bytes()? > self.limits.max_bytes
		{
			return Err(Error::Budget("polynomial storage"));
		}
		Ok(())
	}
	fn charge(&mut self, work: usize) -> Result<()> {
		self.work = self.limits.work(
			self.work
				.checked_add(work)
				.ok_or(Error::Budget("polynomial work"))?,
		)?;
		Ok(())
	}
	fn insert(&mut self, powers: Vec<u32>, value: RBig) -> Result<()> {
		self.insert_live(powers, value, 0)
	}
	fn insert_live(&mut self, powers: Vec<u32>, value: RBig, live: usize) -> Result<()> {
		let incoming = Self::term_bytes(&powers, &value)?;
		if self
			.retained_bytes()?
			.checked_add(live)
			.and_then(|b| b.checked_add(incoming))
			.is_none_or(|b| b > self.limits.max_bytes)
		{
			return Err(Error::Budget("polynomial temporary storage"));
		}

		if powers.len() != self.symbols.len() {
			return Err(Error::Domain("polynomial monomial shape"));
		}
		let degree = powers
			.iter()
			.try_fold(0_u32, |a, b| a.checked_add(*b))
			.ok_or(Error::Budget("polynomial degree"))?;
		if degree > self.limits.max_degree {
			return Err(Error::Budget("polynomial degree"));
		}
		self.limits.coefficient(&value)?;
		let value = if let Some(old) = self.terms.get(&powers) {
			rational_add(old, &value, self.limits)?
		} else {
			value
		};
		let previous = self
			.terms
			.get_key_value(&powers)
			.map(|(p, v)| Self::term_bytes(p, v))
			.transpose()?
			.unwrap_or(0);
		let next = if value == RBig::ZERO {
			0
		} else {
			Self::term_bytes(
				self.terms
					.get_key_value(&powers)
					.map_or(&powers, |(stored, _)| stored),
				&value,
			)?
		};
		self.retained = self
			.retained
			.checked_sub(previous)
			.and_then(|b| b.checked_add(next))
			.ok_or(Error::Budget("polynomial storage"))?;
		if value == RBig::ZERO {
			self.terms.remove(&powers);
		} else {
			self.terms.insert(powers, value);
		}

		self.check_storage()
	}
	fn pair(&self, rhs: &Self, work: usize) -> Result<Self> {
		if self.symbols != rhs.symbols {
			return Err(Error::Domain("polynomial symbol ordering"));
		}
		let count = self
			.work
			.checked_add(rhs.work)
			.and_then(|n| n.checked_add(work))
			.ok_or(Error::Budget("polynomial work"))?;
		self.limits.work(count)?;
		if self
			.retained_bytes()?
			.checked_add(rhs.retained_bytes()?)
			.is_none_or(|n| n > self.limits.max_bytes)
		{
			return Err(Error::Budget("polynomial temporary storage"));
		}
		Ok(Self {
			retained: Self::base_bytes(self.symbols.len())?,
			symbols: self.symbols.clone(),
			terms: BTreeMap::new(),
			limits: self.limits,
			work: count,
		})
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn add(&self, rhs: &Self) -> Result<Self> {
		let mut output = self.pair(
			rhs,
			self.terms
				.len()
				.checked_add(rhs.terms.len())
				.ok_or(Error::Budget("polynomial work"))?,
		)?;
		let live = self
			.retained_bytes()?
			.checked_add(rhs.retained_bytes()?)
			.ok_or(Error::Budget("polynomial temporary storage"))?;
		for (powers, value) in self.terms.iter().chain(&rhs.terms) {
			output.insert_live(powers.clone(), value.clone(), live)?;
		}
		Ok(output)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn negate(&self) -> Result<Self> {
		self.limits.work(
			self.work
				.checked_add(self.terms.len())
				.ok_or(Error::Budget("polynomial work"))?,
		)?;
		if self
			.retained_bytes()?
			.checked_mul(2)
			.is_none_or(|b| b > self.limits.max_bytes)
		{
			return Err(Error::Budget("polynomial temporary storage"));
		}
		let mut output = self.clone();
		output.charge(self.terms.len())?;
		for coefficient in output.terms.values_mut() {
			*coefficient = coefficient.clone().neg();
		}
		Ok(output)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn subtract(&self, rhs: &Self) -> Result<Self> {
		self.add(&rhs.negate()?)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn multiply(&self, rhs: &Self) -> Result<Self> {
		let products = self
			.terms
			.len()
			.checked_mul(rhs.terms.len())
			.ok_or(Error::Budget("polynomial expansion"))?;
		let mut output = self.pair(rhs, products)?;
		let live = self
			.retained_bytes()?
			.checked_add(rhs.retained_bytes()?)
			.ok_or(Error::Budget("polynomial temporary storage"))?;
		for (left, a) in &self.terms {
			for (right, b) in &rhs.terms {
				let powers = left
					.iter()
					.zip(right)
					.map(|(a, b)| {
						a.checked_add(*b)
							.ok_or(Error::Budget("polynomial exponent"))
					})
					.collect::<Result<Vec<_>>>()?;
				output.insert_live(powers, rational_mul(a, b, self.limits)?, live)?;
			}
		}
		Ok(output)
	}
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn differentiate(&self, variable: Symbol) -> Result<Self> {
		let index = self
			.symbols
			.iter()
			.position(|s| *s == variable)
			.ok_or(Error::Domain("polynomial derivative symbol"))?;
		let mut output = Self {
			retained: Self::base_bytes(self.symbols.len())?,
			symbols: self.symbols.clone(),
			terms: BTreeMap::new(),
			limits: self.limits,
			work: self.work,
		};
		output.charge(self.terms.len())?;
		for (powers, value) in &self.terms {
			let exponent = *powers
				.get(index)
				.ok_or(Error::Domain("polynomial monomial shape"))?;
			if exponent > 0 {
				let mut powers = powers.clone();
				*powers
					.get_mut(index)
					.ok_or(Error::Domain("polynomial monomial shape"))? =
					exponent
						.checked_sub(1)
						.ok_or(Error::Budget("polynomial exponent"))?;
				output.insert_live(
					powers,
					rational_mul(value, &RBig::from(exponent), self.limits)?,
					self.retained_bytes()?,
				)?;
			}
		}
		Ok(output)
	}
	/// Simultaneous substitution; replacement polynomials use the same ordered symbols.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn substitute(&self, replacements: &BTreeMap<Symbol, Self>) -> Result<Self> {
		for (symbol, value) in replacements {
			if !self.symbols.contains(symbol) || value.symbols != self.symbols {
				return Err(Error::Domain("polynomial substitution scope"));
			}
		}
		let mut output = Self::constant(self.symbols.clone(), RBig::ZERO, self.limits)?;
		output.charge(self.work)?;
		for (powers, coefficient) in &self.terms {
			let mut term = Self::constant(self.symbols.clone(), coefficient.clone(), self.limits)?;
			for (index, exponent) in powers.iter().enumerate() {
				if *exponent == 0 {
					continue;
				}
				let symbol = self
					.symbols
					.get(index)
					.ok_or(Error::Domain("polynomial variables"))?;
				let variable = Self::variable(self.symbols.clone(), index, self.limits)?;
				let replacement = replacements.get(symbol).unwrap_or(&variable);
				for _ in 0..*exponent {
					term = term.multiply(replacement)?;
				}
			}
			output = output.add(&term)?;
		}
		Ok(output)
	}
	/// Convert each exact coefficient once; no symbolic construction occurs in evaluation.
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn lower<B: Backend>(
		&self,
		backend: &mut B,
	) -> std::result::Result<PolynomialKernel<B::Scalar>, B::Error> {
		backend.charge(self.work)?;
		let evaluation_work = self
			.terms
			.keys()
			.try_fold(0_usize, |total, powers| {
				let degree = powers
					.iter()
					.try_fold(0_usize, |d, p| d.checked_add(usize::try_from(*p).ok()?))?;
				total.checked_add(degree)?.checked_add(1)
			})
			.ok_or(Error::Budget("polynomial kernel work"))?;
		self.limits.work(evaluation_work)?;
		let projected = self
			.terms
			.len()
			.checked_mul(
				self.symbols
					.len()
					.checked_mul(size_of::<u32>())
					.and_then(|b| b.checked_add(backend.working_scalar_bytes()))
					.and_then(|b| b.checked_add(size_of::<(Vec<u32>, B::Scalar)>()))
					.ok_or(Error::Budget("polynomial kernel"))?,
			)
			.ok_or(Error::Budget("polynomial kernel"))?;
		if projected
			.checked_add(self.retained_bytes()?)
			.is_none_or(|b| b > self.limits.max_bytes)
		{
			return Err(Error::Budget("polynomial kernel storage").into());
		}
		let mut terms = Vec::new();
		terms
			.try_reserve_exact(self.terms.len())
			.map_err(|_| Error::Budget("polynomial kernel"))?;
		let mut actual = self
			.retained_bytes()?
			.checked_add(
				terms
					.capacity()
					.checked_mul(size_of::<(Vec<u32>, B::Scalar)>())
					.ok_or(Error::Budget("polynomial kernel storage"))?,
			)
			.ok_or(Error::Budget("polynomial kernel storage"))?;
		for (powers, value) in &self.terms {
			let coefficient = backend.constant(&ExactConstant::Ratio {
				numerator: value.numerator().to_string(),
				denominator: value.denominator().to_string(),
			})?;
			actual = actual
				.checked_add(backend.storage_bytes(&coefficient)?)
				.and_then(|b| b.checked_add(powers.capacity().checked_mul(size_of::<u32>())?))
				.ok_or(Error::Budget("polynomial kernel storage"))?;
			if actual > self.limits.max_bytes {
				return Err(Error::Budget("polynomial kernel storage").into());
			}
			terms.push((powers.clone(), coefficient));
		}
		Ok(PolynomialKernel {
			profile: backend.profile(),
			variables: self.symbols.len(),
			terms,
			max_bytes: self.limits.max_bytes,
			retained: actual
				.checked_sub(self.retained_bytes()?)
				.ok_or(Error::Budget("polynomial kernel storage"))?,
		})
	}
}

#[derive(Debug, Clone)]
pub struct PolynomialKernel<S> {
	profile: ArithmeticProfile,
	variables: usize,
	terms: Vec<(Vec<u32>, S)>,
	max_bytes: usize,
	retained: usize,
}
impl<S> PolynomialKernel<S> {
	#[must_use]
	pub const fn profile(&self) -> ArithmeticProfile {
		self.profile
	}
	/// Complete input coordinate count, including variables absent from the terms.
	#[must_use]
	pub const fn variables(&self) -> usize {
		self.variables
	}
	/// Admitted retained payload plus the kernel wrapper. Lowering charges actual
	/// term capacity, exponent storage and backend scalar storage conservatively.
	/// This excludes evaluation inputs, transient arithmetic and allocator metadata.
	/// # Errors
	/// Rejects byte-accounting overflow.
	pub const fn retained_bytes(&self) -> Result<usize> {
		match self.retained.checked_add(size_of::<Self>()) {
			Some(bytes) => Ok(bytes),
			None => Err(Error::Budget("polynomial kernel retained bytes")),
		}
	}
}
impl<S: Clone> PolynomialKernel<S> {
	/// # Errors
	/// Rejects invalid input, unsupported operations, or exhausted resource limits.
	pub fn evaluate<B: Backend<Scalar = S>>(
		&self,
		backend: &mut B,
		inputs: &[S],
	) -> std::result::Result<S, B::Error> {
		if backend.profile() != self.profile {
			return Err(Error::Domain("arithmetic profile mismatch").into());
		}
		if inputs.len() != self.variables {
			return Err(Error::Domain("polynomial kernel input shape").into());
		}
		for input in inputs {
			backend.validate(input)?;
		}
		let mut base = self.retained;
		for input in inputs {
			base = base
				.checked_add(backend.storage_bytes(input)?)
				.ok_or(Error::Budget("polynomial evaluation storage"))?;
		}
		if base
			.checked_add(
				backend
					.working_scalar_bytes()
					.checked_mul(3)
					.ok_or(Error::Budget("polynomial evaluation storage"))?,
			)
			.is_none_or(|b| b > self.max_bytes)
		{
			return Err(Error::Budget("polynomial evaluation storage").into());
		}
		let mut output = backend.constant(&ExactConstant::Integer(0))?;
		for (powers, coefficient) in &self.terms {
			let mut term = coefficient.clone();
			backend.validate(&term)?;
			for (value, exponent) in inputs.iter().zip(powers) {
				for _ in 0..*exponent {
					backend.visit()?;
					let temporary = backend
						.storage_bytes(&term)?
						.checked_add(backend.storage_bytes(value)?)
						.and_then(|b| b.checked_add(backend.working_scalar_bytes()))
						.ok_or(Error::Budget("polynomial evaluation storage"))?;
					if base
						.checked_add(backend.storage_bytes(&output)?)
						.and_then(|b| b.checked_add(temporary))
						.is_none_or(|b| b > self.max_bytes)
					{
						return Err(Error::Budget("polynomial evaluation storage").into());
					}
					term = backend.mul(term, value.clone())?;
				}
			}
			backend.visit()?;
			let term_bytes = backend.storage_bytes(&term)?;
			let temporary = base
				.checked_add(backend.storage_bytes(&output)?)
				.and_then(|b| b.checked_add(term_bytes))
				.and_then(|b| b.checked_add(backend.working_scalar_bytes()))
				.ok_or(Error::Budget("polynomial evaluation storage"))?;
			if temporary > self.max_bytes {
				return Err(Error::Budget("polynomial evaluation storage").into());
			}
			output = backend.add(output, term)?;
		}
		Ok(output)
	}
}

pub(crate) fn rational_add(a: &RBig, b: &RBig, limits: PolynomialLimits) -> Result<RBig> {
	limits.coefficient(a)?;
	limits.coefficient(b)?;
	let left = a
		.numerator()
		.bit_len()
		.checked_add(b.denominator().bit_len())
		.ok_or(Error::Budget("rational growth"))?;
	let right = b
		.numerator()
		.bit_len()
		.checked_add(a.denominator().bit_len())
		.ok_or(Error::Budget("rational growth"))?;
	limits.width(
		left.max(right)
			.checked_add(1)
			.ok_or(Error::Budget("rational growth"))?,
	)?;
	limits.width(
		a.denominator()
			.bit_len()
			.checked_add(b.denominator().bit_len())
			.ok_or(Error::Budget("rational growth"))?,
	)?;
	let value = a.add(b);
	limits.coefficient(&value)?;
	Ok(value)
}
pub(crate) fn rational_mul(a: &RBig, b: &RBig, limits: PolynomialLimits) -> Result<RBig> {
	limits.coefficient(a)?;
	limits.coefficient(b)?;
	limits.width(
		a.numerator()
			.bit_len()
			.checked_add(b.numerator().bit_len())
			.ok_or(Error::Budget("rational growth"))?,
	)?;
	limits.width(
		a.denominator()
			.bit_len()
			.checked_add(b.denominator().bit_len())
			.ok_or(Error::Budget("rational growth"))?,
	)?;
	let value = a.mul(b);
	limits.coefficient(&value)?;
	Ok(value)
}
/// Explicit exact import; binary64 is imported as its dyadic value, never as a decimal approximation.
/// # Errors
/// Rejects invalid input, unsupported operations, or exhausted resource limits.
pub fn rational_constant(constant: &ExactConstant, limits: PolynomialLimits) -> Result<RBig> {
	let source_bytes = match constant {
		ExactConstant::Decimal(s) => s.capacity(),
		ExactConstant::Ratio {
			numerator,
			denominator,
		} => numerator
			.capacity()
			.checked_add(denominator.capacity())
			.ok_or(Error::Budget("constant storage"))?,
		_ => 0,
	};
	if source_bytes
		.checked_add(size_of::<RBig>())
		.is_none_or(|b| b > limits.max_bytes)
	{
		return Err(Error::Budget("constant storage"));
	}
	let value = match constant {
		ExactConstant::Pi => return Err(Error::Domain("pi is not rational")),
		ExactConstant::Integer(value) => RBig::from(*value),
		ExactConstant::Rational(n, d) => {
			if *d == 0 {
				return Err(Error::Domain("zero rational denominator"));
			}
			RBig::from_parts_signed(IBig::from(*n), IBig::from(*d))
		}
		ExactConstant::Ratio {
			numerator,
			denominator,
		} => {
			if numerator.len().max(denominator.len()).saturating_mul(4)
				> limits.max_coefficient_bits
			{
				return Err(Error::Budget("rational digits"));
			}
			if !crate::arithmetic::valid_integer(numerator)
				|| !crate::arithmetic::valid_positive_integer(denominator)
			{
				return Err(Error::Interchange("integer ratio"));
			}
			let n = IBig::from_str_radix(numerator, 10)
				.map_err(|_| Error::Interchange("integer ratio"))?;
			let d = IBig::from_str_radix(denominator, 10)
				.map_err(|_| Error::Interchange("integer ratio"))?;
			if d <= IBig::ZERO {
				return Err(Error::Domain("positive rational denominator"));
			}
			RBig::from_parts_signed(n, d)
		}
		ExactConstant::Decimal(text) => exact_decimal(text, limits)?,

		ExactConstant::Binary64(value) => exact_binary64(*value, limits)?,
	};
	limits.coefficient(&value)?;
	Ok(value)
}

fn exact_decimal(text: &str, limits: PolynomialLimits) -> Result<RBig> {
	if !crate::arithmetic::valid_decimal(text) {
		return Err(Error::Interchange("decimal syntax"));
	}
	if text.len().saturating_mul(4) > limits.max_coefficient_bits {
		return Err(Error::Budget("decimal digits"));
	}
	let mut parts = text.split(['e', 'E']);
	let mantissa = parts.next().unwrap_or("");
	let exponent = parts
		.next()
		.map(str::parse::<i32>)
		.transpose()
		.map_err(|_| Error::Interchange("decimal exponent"))?
		.unwrap_or(0);
	if parts.next().is_some() || mantissa.bytes().filter(|b| *b == b'.').count() > 1 {
		return Err(Error::Interchange("decimal syntax"));
	}
	let fraction = mantissa.split('.').nth(1).map_or(0, str::len);
	let scale = exponent
		.checked_sub(i32::try_from(fraction).map_err(|_| Error::Budget("decimal scale"))?)
		.ok_or(Error::Budget("decimal scale"))?;
	let growth = usize::try_from(scale.unsigned_abs())
		.map_err(|_| Error::Budget("decimal scale"))?
		.checked_mul(4)
		.ok_or(Error::Budget("decimal scale"))?;
	limits.width(
		growth
			.checked_add(text.len().saturating_mul(4))
			.ok_or(Error::Budget("decimal growth"))?,
	)?;
	let n = IBig::from_str_radix(&mantissa.replace('.', ""), 10)
		.map_err(|_| Error::Interchange("decimal syntax"))?;
	let power = IBig::from(10)
		.pow(usize::try_from(scale.unsigned_abs()).map_err(|_| Error::Budget("decimal scale"))?);
	Ok(if scale >= 0 {
		RBig::from(n.mul(power))
	} else {
		RBig::from_parts_signed(n, power)
	})
}
fn exact_binary64(value: f64, limits: PolynomialLimits) -> Result<RBig> {
	let parts = crate::dyadic::parts(value).ok_or(Error::Nonfinite)?;
	let mantissa = parts.mantissa;
	let shift = parts.exponent;
	limits.width(
		usize::try_from(shift.unsigned_abs())
			.map_err(|_| Error::Budget("binary64 exponent"))?
			.saturating_add(53),
	)?;
	let numerator = if parts.negative {
		IBig::from(mantissa).neg()
	} else {
		IBig::from(mantissa)
	};
	Ok(if shift >= 0 {
		RBig::from(std::ops::Shl::shl(
			numerator,
			usize::try_from(shift).map_err(|_| Error::Budget("binary64 exponent"))?,
		))
	} else {
		RBig::from_parts_signed(
			numerator,
			std::ops::Shl::shl(
				IBig::ONE,
				usize::try_from(shift.unsigned_abs())
					.map_err(|_| Error::Budget("binary64 exponent"))?,
			),
		)
	})
}
