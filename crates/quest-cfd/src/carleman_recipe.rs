//! Stateless full normalized symmetric Carleman rows in the stored hierarchy's order.
//!
//! Only the complete physical ODE is retained. Binary64 queries are bounded
//! classical generation, not a coherent oracle or a truncation certificate.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Admitted dimensions/degrees bound exponent indices; binomial and resource arithmetic is checked, numerical results checked for finiteness"
)]
use crate::{CfdError, polynomial::PolynomialOde, stream_history::HistoryRowDynamics};
use quest_numerics::Complex64;
use std::sync::Arc;
/// Admission for executable indices and each independent scalar/row query.
#[derive(Clone, Copy, Debug)]
pub struct CarlemanRecipeLimits {
	pub max_dimension: usize,
	pub max_physical_dimension: usize,
	pub max_order: usize,
	pub max_terms: usize,
	pub max_bytes: usize,
	pub max_construct_work: usize,
	pub max_index_work: usize,
	pub max_query_work: usize,
}
impl Default for CarlemanRecipeLimits {
	fn default() -> Self {
		Self {
			max_dimension: usize::MAX,
			max_physical_dimension: 4096,
			max_order: 64,
			max_terms: 65536,
			max_bytes: 134_217_728,
			max_construct_work: 100_000_000,
			max_index_work: 100_000_000,
			max_query_work: 100_000_000,
		}
	}
}
/// Conservative logical counts, with the still-replicated complete physical forms charged.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct CarlemanRecipeResources {
	pub dimension: usize,
	pub physical_dimension: usize,
	pub maximum_row_entries: usize,
	pub owned_bytes: usize,
	pub borrowed_ode_bytes: usize,
	pub retained_bytes: usize,
	pub query_bytes: usize,
	pub peak_bytes: usize,
	pub construct_work: usize,
	pub index_work: usize,
	pub kernel_work: usize,
	pub query_work: usize,
}
/// Immutable full hierarchy recipe. No powers, hierarchy-index or coefficient-recipe catalogue.
#[derive(Clone, Debug)]
pub struct StatelessCarleman {
	ode: Arc<PolynomialOde>,
	order: usize,
	scale: f64,
	dimension: usize,
	resources: CarlemanRecipeResources,
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b)
		.ok_or(CfdError::InvalidInput("stateless Carleman count overflow"))
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or(CfdError::InvalidInput(
		"stateless Carleman work/storage overflow",
	))
}
fn machine(value: u128) -> Result<usize, CfdError> {
	usize::try_from(value)
		.map_err(|_| CfdError::InvalidInput("stateless Carleman index does not fit machine size"))
}
// No arbitrary-width allocations in an executable index query. The arbitrary-width
// estimate remains carleman::symmetric_dimension. An intermediate overflow rejects.
fn binomial(n: usize, k: usize) -> Result<u128, CfdError> {
	if k > n {
		return Err(CfdError::InvalidInput("invalid binomial index"));
	}
	let k = k.min(n - k);
	let mut value = 1_u128;
	for j in 1..=k {
		value = value
			.checked_mul(
				u128::try_from(add(n - k, j)?)
					.map_err(|_| CfdError::InvalidInput("binomial input width"))?,
			)
			.ok_or(CfdError::InvalidInput("binomial intermediate exceeds u128"))?
			/ u128::try_from(j).map_err(|_| CfdError::InvalidInput("binomial divisor width"))?;
	}
	Ok(value)
}
fn compositions(variables: usize, degree: usize) -> Result<usize, CfdError> {
	machine(binomial(
		add(variables, degree)?
			.checked_sub(1)
			.ok_or(CfdError::InvalidInput("zero composition dimension"))?,
		degree,
	)?)
}
fn exponent_buffer(n: usize) -> Result<Vec<u32>, CfdError> {
	let mut v = Vec::new();
	v.try_reserve_exact(n)
		.map_err(|_| CfdError::InvalidInput("Carleman exponent allocation"))?;
	v.resize(n, 0);
	Ok(v)
}
fn normalization(powers: &[u32]) -> f64 {
	let mut value = 1.;
	let mut used = 0_u32;
	for &power in powers {
		for j in 1..=power {
			value *= f64::from(used + j) / f64::from(j);
		}
		used += power;
	}
	value.sqrt()
}
fn factor(value: f64) -> Result<f64, CfdError> {
	if value.is_finite() && value != 0. {
		Ok(value)
	} else {
		Err(CfdError::InvalidInput(
			"Carleman normalization over/underflow",
		))
	}
}
impl StatelessCarleman {
	/// Retain admitted physical forms and scalar metadata; allocate no hierarchy catalogue.
	/// # Errors
	/// Rejects index/count overflow, depths above 64, scale, coefficient shapes or budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "Physical kernel declarations and all complete query bounds are admitted together before publishing the immutable recipe"
	)]
	pub fn new(
		ode: Arc<PolynomialOde>,
		order: usize,
		scale: f64,
		limits: CarlemanRecipeLimits,
	) -> Result<Self, CfdError> {
		let m = ode.dimension();
		let k = ode.coefficient_terms().len();
		if m == 0
			|| m > limits.max_physical_dimension
			|| order == 0
			|| order > 64
			|| order > limits.max_order
			|| !scale.is_finite()
			|| scale <= 0.
			|| k > limits.max_terms
		{
			return Err(CfdError::InvalidInput(
				"stateless Carleman dimensions/depth/scale/terms",
			));
		}
		let dimension = machine(
			binomial(add(m, order)?, order)?
				.checked_sub(1)
				.ok_or(CfdError::InvalidInput("zero Carleman dimension"))?,
		)?;
		if dimension > limits.max_dimension {
			return Err(CfdError::InvalidInput(
				"stateless Carleman dimension budget",
			));
		}
		let symbols = ode.symbols().len();
		let mut kernel_work = 0;
		for term in ode.coefficient_terms() {
			if term.row >= m
				|| term.powers.len() != m
				|| term
					.powers
					.iter()
					.try_fold(0_u32, |a, &b| a.checked_add(b))
					.is_none_or(|d| d > 2)
			{
				return Err(CfdError::InvalidInput(
					"incomplete physical coefficient form",
				));
			}
			let mut work = mul(add(symbols, 16)?, 16)?;
			for (powers, _) in term.symbolic().terms() {
				let degree = powers.iter().try_fold(0_usize, |a, &b| {
					add(
						a,
						usize::try_from(b)
							.map_err(|_| CfdError::InvalidInput("coefficient exponent width"))?,
					)
				})?;
				work = add(work, mul(add(add(symbols, degree)?, 16)?, 16)?)?;
			}
			kernel_work = add(kernel_work, work)?;
		}
		let index_work = mul(mul(add(m, 1)?, mul(add(order, 1)?, add(order, 1)?)?)?, 32)?;
		let selection = mul(mul(k, order.min(m))?, 16)?;
		let normalization_work = mul(mul(add(k, 1)?, add(add(m, order)?, 8)?)?, 16)?;
		let query_work = add(
			add(
				add(mul(add(k, 1)?, index_work)?, selection)?,
				normalization_work,
			)?,
			add(kernel_work, mul(mul(m, add(order, 1)?)?, 16)?)?,
		)?;
		let construct_work = add(
			add(index_work, kernel_work)?,
			mul(mul(add(m, 1)?, add(k, 1)?)?, 32)?,
		)?;
		let owned_bytes = add(size_of::<Self>(), mul(2, size_of::<usize>())?)?;
		let borrowed_ode_bytes = ode.retained_bytes();
		let retained_bytes = add(owned_bytes, borrowed_ode_bytes)?;
		let query_bytes = add(
			add(mul(m, 16)?, mul(symbols, 8)?)?,
			add(mul(3, size_of::<Vec<u32>>())?, 512)?,
		)?;
		let peak_bytes = add(retained_bytes, query_bytes)?;
		if index_work > limits.max_index_work
			|| query_work > limits.max_query_work
			|| construct_work > limits.max_construct_work
			|| peak_bytes > limits.max_bytes
		{
			return Err(CfdError::InvalidInput(
				"stateless Carleman work/storage budget",
			));
		}
		Ok(Self {
			ode,
			order,
			scale,
			dimension,
			resources: CarlemanRecipeResources {
				dimension,
				physical_dimension: m,
				maximum_row_entries: k,
				owned_bytes,
				borrowed_ode_bytes,
				retained_bytes,
				query_bytes,
				peak_bytes,
				construct_work,
				index_work,
				kernel_work,
				query_work,
			},
		})
	}
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.dimension
	}
	#[must_use]
	pub fn physical_dimension(&self) -> usize {
		self.ode.dimension()
	}
	#[must_use]
	pub const fn order(&self) -> usize {
		self.order
	}
	#[must_use]
	pub const fn scale(&self) -> f64 {
		self.scale
	}
	#[must_use]
	pub const fn resources(&self) -> CarlemanRecipeResources {
		self.resources
	}
	fn degree(&self, powers: &[u32]) -> Result<usize, CfdError> {
		if powers.len() != self.physical_dimension() {
			return Err(CfdError::InvalidInput("complete Carleman exponent shape"));
		}
		let degree = powers.iter().try_fold(0_usize, |a, &b| {
			add(
				a,
				usize::try_from(b)
					.map_err(|_| CfdError::InvalidInput("Carleman exponent width"))?,
			)
		})?;
		if degree == 0 || degree > self.order {
			return Err(CfdError::InvalidInput("Carleman exponent degree"));
		}
		Ok(degree)
	}
	const fn row(&self, row: usize) -> Result<(), CfdError> {
		if row < self.dimension {
			Ok(())
		} else {
			Err(CfdError::InvalidInput("stateless Carleman row index"))
		}
	}
	/// Increasing degree, then descending lexicographic exponents, exactly as `SymmetricCarleman`.
	/// # Errors
	/// Rejects degree zero, malformed complete exponent arrays, depth or index overflow.
	pub fn rank(&self, powers: &[u32]) -> Result<usize, CfdError> {
		let degree = self.degree(powers)?;
		let m = self.physical_dimension();
		let mut index = 0;
		for d in 1..degree {
			index = add(index, compositions(m, d)?)?;
		}
		let mut remaining = degree;
		for (axis, &power) in powers.iter().take(m - 1).enumerate() {
			let power =
				usize::try_from(power).map_err(|_| CfdError::InvalidInput("rank power width"))?;
			if power > remaining {
				return Err(CfdError::InvalidInput("rank remaining degree"));
			}
			for candidate in (add(power, 1)?..=remaining).rev() {
				index = add(index, compositions(m - axis - 1, remaining - candidate)?)?;
			}
			remaining -= power;
		}
		self.row(index)?;
		Ok(index)
	}
	/// Decode one executable index with an `O(physical_dimension)` exponent buffer.
	/// Returned storage is caller-owned; simultaneous queries need aggregate admission.
	/// # Errors
	/// Rejects invalid indices, allocation or checked combinatorial arithmetic.
	pub fn unrank(&self, row: usize) -> Result<Vec<u32>, CfdError> {
		self.row(row)?;
		let m = self.physical_dimension();
		let mut offset = row;
		let mut degree = 1;
		while degree <= self.order {
			let count = compositions(m, degree)?;
			if offset < count {
				break;
			}
			offset -= count;
			degree += 1;
		}
		if degree > self.order {
			return Err(CfdError::InvalidInput("Carleman degree decode"));
		}
		let mut powers = exponent_buffer(m)?;
		let mut remaining = degree;
		for (axis, power) in powers.iter_mut().take(m - 1).enumerate() {
			let mut choice = None;
			for candidate in (0..=remaining).rev() {
				let count = compositions(m - axis - 1, remaining - candidate)?;
				if offset < count {
					choice = Some(candidate);
					break;
				}
				offset -= count;
			}
			let chosen = choice.ok_or(CfdError::InvalidInput("Carleman composition decode"))?;
			*power = u32::try_from(chosen)
				.map_err(|_| CfdError::InvalidInput("decoded exponent width"))?;
			remaining -= chosen;
		}
		powers[m - 1] = u32::try_from(remaining)
			.map_err(|_| CfdError::InvalidInput("decoded final exponent width"))?;
		if offset != 0 {
			return Err(CfdError::InvalidInput("Carleman final offset"));
		}
		Ok(powers)
	}
	/// One normalized initial monomial, retaining all physical/auxiliary coordinates.
	/// # Errors
	/// Rejects invalid complete state, row, allocation or numerical overflow.
	pub fn lift_entry(&self, row: usize, state: &[f64]) -> Result<f64, CfdError> {
		if state.len() != self.physical_dimension() || state.iter().any(|x| !x.is_finite()) {
			return Err(CfdError::InvalidInput("complete Carleman initial state"));
		}
		let powers = self.unrank(row)?;
		let mut value = normalization(&powers);
		for (&x, &power) in state.iter().zip(&powers) {
			value *= (x / self.scale).powi(
				i32::try_from(power)
					.map_err(|_| CfdError::InvalidInput("initial exponent width"))?,
			);
		}
		if value.is_finite() {
			Ok(value)
		} else {
			Err(CfdError::InvalidInput("Carleman initial scalar overflow"))
		}
	}
	/// Degree-one coordinate index; no physical, mean or auxiliary coordinate is omitted.
	/// # Errors
	/// Rejects an axis outside the complete physical ODE.
	pub fn physical_coordinate_index(&self, axis: usize) -> Result<usize, CfdError> {
		if axis < self.physical_dimension() {
			Ok(axis)
		} else {
			Err(CfdError::InvalidInput("Carleman physical coordinate index"))
		}
	}
	/// Recover one physical coordinate from its degree-one hierarchy amplitude.
	/// # Errors
	/// Rejects invalid axis/amplitude or scaling overflow.
	pub fn recover_entry(&self, axis: usize, amplitude: f64) -> Result<f64, CfdError> {
		self.physical_coordinate_index(axis)?;
		let value = self.scale * amplitude;
		if amplitude.is_finite() && value.is_finite() {
			Ok(value)
		} else {
			Err(CfdError::InvalidInput("Carleman physical scalar recovery"))
		}
	}
}
impl HistoryRowDynamics for StatelessCarleman {
	fn dimension(&self) -> usize {
		self.dimension
	}
	fn max_row_entries(&self) -> usize {
		self.resources.maximum_row_entries
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(self.resources.retained_bytes)
	}
	fn row_query_bytes(&self) -> usize {
		self.resources.query_bytes
	}
	fn row_query_work(&self) -> usize {
		self.resources.query_work
	}
	fn visit_row(
		&self,
		time: f64,
		row: usize,
		visitor: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite Carleman row time"));
		}
		let alpha = self.unrank(row)?;
		let degree = self.degree(&alpha)?;
		let norm = normalization(&alpha);
		let mut beta = exponent_buffer(self.physical_dimension())?;
		for (axis, &multiplicity) in alpha.iter().enumerate() {
			if multiplicity == 0 {
				continue;
			}
			for coefficient in self
				.ode
				.coefficient_terms()
				.iter()
				.filter(|c| c.row == axis)
			{
				let term_degree = coefficient.powers.iter().try_fold(0_usize, |a, &b| {
					add(
						a,
						usize::try_from(b)
							.map_err(|_| CfdError::InvalidInput("coefficient degree width"))?,
					)
				})?;
				let target_degree = add(degree - 1, term_degree)?;
				if target_degree == 0 || target_degree > self.order {
					continue;
				}
				beta.copy_from_slice(&alpha);
				beta[axis] -= 1;
				for (b, &p) in beta.iter_mut().zip(&coefficient.powers) {
					*b = b
						.checked_add(p)
						.ok_or(CfdError::InvalidInput("target exponent overflow"))?;
				}
				let column = self.rank(&beta)?;
				let exponent = i32::try_from(term_degree)
					.map_err(|_| CfdError::InvalidInput("scale exponent width"))?
					- 1;
				let factor = factor(
					f64::from(multiplicity) * norm / normalization(&beta)
						* self.scale.powi(exponent),
				)?;
				let value = factor * coefficient.value(time)?;
				if !value.is_finite() {
					return Err(CfdError::InvalidInput("Carleman row coefficient overflow"));
				}
				visitor(column, Complex64::new(value, 0.))?;
			}
		}
		Ok(())
	}
	#[allow(
		clippy::suboptimal_flops,
		reason = "Preserve the stored source recipe multiplication and accumulation order"
	)]
	fn source_entry(&self, time: f64, row: usize) -> Result<Complex64, CfdError> {
		self.row(row)?;
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite Carleman source time"));
		}
		let mut value = 0.;
		if row < self.physical_dimension() {
			for coefficient in self
				.ode
				.coefficient_terms()
				.iter()
				.filter(|c| c.row == row && c.powers.iter().all(|&p| p == 0))
			{
				value += factor(self.scale.powi(-1))? * coefficient.value(time)?;
			}
		}
		if value.is_finite() {
			Ok(Complex64::new(value, 0.))
		} else {
			Err(CfdError::InvalidInput("Carleman external source overflow"))
		}
	}
}
