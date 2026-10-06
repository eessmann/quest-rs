//! All normalized symmetric monomials, with degree zero kept as an external source.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked lift admission bounds indices and finite numerical normalization formulas"
)]
use crate::{CfdError, history::HistoryDynamics, polynomial::PolynomialOde};
use dashu_int::{UBig, ops::BitTest};
use mathcore::{RBig, multivariate::SparsePolynomial};
use quest_numerics::{Complex64, arithmetic::F64Backend};
use std::{collections::BTreeMap, sync::Arc};

/// Complete combinatorial dimension, independent of machine-size simulation admission.
/// # Errors
/// Rejects zero dimensions and more than 4096 lift degrees to bound integer work.
pub fn symmetric_dimension(physical_dimension: &UBig, order: usize) -> Result<UBig, CfdError> {
	if physical_dimension == &UBig::ZERO || order == 0 || order > 4096 {
		return Err(CfdError::InvalidInput(
			"invalid symmetric lift dimension/order",
		));
	}
	if physical_dimension
		.bit_len()
		.checked_add(13)
		.and_then(|n| n.checked_mul(order))
		.is_none_or(|n| n > 1_048_576 || n.checked_mul(order).is_none_or(|work| work > 134_217_728))
	{
		return Err(CfdError::InvalidInput(
			"symmetric dimension integer work budget",
		));
	}
	let mut count = UBig::ONE;
	for k in 1..=order {
		count = count * (physical_dimension + UBig::from(k)) / UBig::from(k);
	}
	Ok(count - UBig::ONE)
}
/// Budgets admitted before enumerating any complete hierarchy or symbolic recipe.
#[derive(Clone, Copy, Debug)]
pub struct CarlemanLimits {
	/// Maximum materialized full symmetric/ordered dimension.
	pub max_dimension: usize,
	/// Maximum lift degree for construction (estimates have a separate bound).
	pub max_order: usize,
	/// Maximum sparse contributions, including duplicated entries.
	pub max_entries: usize,
	/// Conservative live storage ceiling, including borrowed physical forms and scratch.
	pub max_bytes: usize,
	/// Conservative symbolic coefficient operation ceiling.
	pub max_work: usize,
}
impl Default for CarlemanLimits {
	fn default() -> Self {
		Self {
			max_dimension: 16_384,
			max_order: 16,
			max_entries: 1_000_000,
			max_bytes: 128 * 1024 * 1024,
			max_work: 100_000_000,
		}
	}
}
#[derive(Clone, Debug)]
struct Recipe {
	row: usize,
	column: Option<usize>,
	coefficient: usize,
	factor: f64,
}
/// Complete normalized symmetric hierarchy with sparse coefficient recipes.
#[derive(Clone, Debug)]
pub struct SymmetricCarleman {
	ode: Arc<PolynomialOde>,
	order: usize,
	scale: f64,
	powers: Vec<Vec<u32>>,
	normalizations: Vec<f64>,
	recipes: Vec<Recipe>,
	first: Vec<usize>,
	retained: usize,
}
fn admitted(
	ode: &PolynomialOde,
	order: usize,
	count: usize,
	limits: CarlemanLimits,
	exact_symmetric_entries: Option<usize>,
) -> Result<usize, CfdError> {
	if ode.dimension() > 512
		|| order == 0
		|| order > limits.max_order
		|| order > 64
		|| count == 0
		|| count > limits.max_dimension
	{
		return Err(CfdError::InvalidInput(
			"complete Carleman dimension/order exceeds budget",
		));
	}
	let selections = count
		.checked_mul(ode.coefficient_terms().len())
		.and_then(|n| n.checked_mul(order.min(ode.dimension())))
		.ok_or(CfdError::InvalidInput("Carleman entry budget overflow"))?;
	let entries = exact_symmetric_entries.unwrap_or(selections);
	let state_bytes = count
		.checked_mul(ode.dimension())
		.and_then(|n| n.checked_mul(16))
		.ok_or(CfdError::InvalidInput("Carleman state budget overflow"))?;
	let scratch = ode
		.coefficient_terms()
		.len()
		.checked_mul(
			ode.symbols()
				.len()
				.checked_mul(64)
				.and_then(|n| n.checked_add(1024))
				.ok_or(CfdError::InvalidInput("Carleman scratch overflow"))?,
		)
		.ok_or(CfdError::InvalidInput("Carleman scratch overflow"))?;
	let retained = entries
		.checked_mul(size_of::<Recipe>())
		.and_then(|n| n.checked_add(state_bytes))
		.and_then(|n| n.checked_add(count.checked_mul(128)?))
		.and_then(|n| n.checked_add(ode.retained_bytes()))
		.and_then(|n| n.checked_add(scratch))
		.ok_or(CfdError::InvalidInput("Carleman storage budget overflow"))?;
	let derivatives = count
		.checked_mul(order.min(ode.dimension()))
		.ok_or(CfdError::InvalidInput("Carleman derivative count overflow"))?;
	let work = entries
		.checked_add(derivatives)
		.and_then(|n| n.checked_add(count))
		.ok_or(CfdError::InvalidInput("Carleman enumeration work overflow"))?
		.checked_mul(
			ode.symbols()
				.len()
				.checked_add(order)
				.and_then(|n| n.checked_add(8))
				.ok_or(CfdError::InvalidInput("Carleman work overflow"))?,
		)
		.and_then(|n| n.checked_mul(8))
		.and_then(|n| n.checked_add(selections.checked_mul(2)?))
		.ok_or(CfdError::InvalidInput("Carleman work overflow"))?;
	if entries > limits.max_entries || retained > limits.max_bytes || work > limits.max_work {
		return Err(CfdError::InvalidInput(
			"complete Carleman recipes exceed admission budget",
		));
	}
	Ok(retained)
}
// For a fixed physical derivative axis, all alpha with alpha_i>0 and
// |alpha|<=d number binomial(m+d-1,d-1). Each physical coefficient contributes
// once to each such row; quadratic terms use d=r-1 because the r+1 block is omitted.
fn symmetric_recipe_count(ode: &PolynomialOde, order: usize) -> Result<usize, CfdError> {
	let count = |maximum_degree: usize| -> Result<usize, CfdError> {
		if maximum_degree == 0 {
			return Ok(0);
		}
		if maximum_degree == 1 {
			return Ok(1);
		}
		usize::try_from(
			symmetric_dimension(&UBig::from(ode.dimension()), maximum_degree - 1)? + UBig::ONE,
		)
		.map_err(|_| CfdError::InvalidInput("symmetric recipe count overflow"))
	};
	let linear_source = count(order)?;
	let quadratic = count(order.saturating_sub(1))?;
	ode.coefficient_terms()
		.iter()
		.try_fold(0usize, |total, term| {
			let degree: u32 = term.powers.iter().sum();
			total
				.checked_add(if degree == 2 {
					quadratic
				} else {
					linear_source
				})
				.ok_or(CfdError::InvalidInput("symmetric recipe count overflow"))
		})
}
fn enumerate(remaining: u32, axis: usize, powers: &mut [u32], output: &mut Vec<Vec<u32>>) {
	if axis + 1 == powers.len() {
		powers[axis] = remaining;
		output.push(powers.to_vec());
		return;
	}
	for value in (0..=remaining).rev() {
		powers[axis] = value;
		enumerate(remaining - value, axis + 1, powers, output);
	}
}
fn normalization(powers: &[u32]) -> f64 {
	let mut value = 1.;
	let mut used = 0u32;
	for &power in powers {
		for j in 1..=power {
			value *= f64::from(used + j) / f64::from(j);
		}
		used += power;
	}
	value.sqrt()
}
fn checked_factor(value: f64) -> Result<f64, CfdError> {
	if !value.is_finite() || value == 0. {
		Err(CfdError::InvalidInput(
			"Carleman normalization over/underflow",
		))
	} else {
		Ok(value)
	}
}
impl SymmetricCarleman {
	/// Build every degree-one through degree-r monomial; no physical coordinates are dropped.
	/// # Errors
	/// Rejects nonpositive scale, symbolic errors and budgets before hierarchy growth.
	#[allow(
		clippy::too_many_lines,
		reason = "Admission and complete symbolic recipe enumeration share a single bounded constructor"
	)]
	pub fn new(
		ode: Arc<PolynomialOde>,
		order: usize,
		scale: f64,
		limits: CarlemanLimits,
	) -> Result<Self, CfdError> {
		if !scale.is_finite() || scale <= 0. {
			return Err(CfdError::InvalidInput(
				"positive finite Carleman scale required",
			));
		}
		if order == 0
			|| order > limits.max_order
			|| order > 64
			|| order
				> usize::try_from(ode.limits().max_degree)
					.map_err(|_| CfdError::InvalidInput("polynomial degree budget conversion"))?
		{
			return Err(CfdError::InvalidInput(
				"Carleman degree exceeds construction budget",
			));
		}
		let count = usize::try_from(symmetric_dimension(&UBig::from(ode.dimension()), order)?)
			.map_err(|_| {
				CfdError::InvalidInput("complete Carleman dimension does not fit machine size")
			})?;
		let recipe_capacity = symmetric_recipe_count(&ode, order)?;
		let retained = admitted(&ode, order, count, limits, Some(recipe_capacity))?;
		let mut powers = Vec::with_capacity(count);
		for degree in 1..=order {
			enumerate(
				u32::try_from(degree)
					.map_err(|_| CfdError::InvalidInput("lift degree overflow"))?,
				0,
				&mut vec![0; ode.dimension()],
				&mut powers,
			);
		}
		let indices: BTreeMap<_, _> = powers
			.iter()
			.cloned()
			.enumerate()
			.map(|(i, p)| (p, i))
			.collect();
		let normalizations: Vec<_> = powers.iter().map(|p| normalization(p)).collect();
		let mut recipes = Vec::with_capacity(recipe_capacity);
		let zero_inputs = vec![0.; ode.symbols().len()];
		let plimits = ode.limits();
		// State-only polynomials are compiled through the same neutral exact algebra as the physical ODE.
		let mut state_terms = Vec::new();
		for coefficient in ode.coefficient_terms() {
			let mut p = coefficient.powers.clone();
			p.resize(ode.symbols().len(), 0);
			state_terms.push(SparsePolynomial::from_terms(
				ode.symbols().to_vec(),
				[(p, RBig::ONE)],
				plimits,
			)?);
		}
		for (row, alpha) in powers.iter().enumerate() {
			let degree: u32 = alpha.iter().sum();
			let mut p = alpha.clone();
			p.resize(ode.symbols().len(), 0);
			let monomial =
				SparsePolynomial::from_terms(ode.symbols().to_vec(), [(p, RBig::ONE)], plimits)?;
			for (axis, &multiplicity) in alpha.iter().enumerate() {
				if multiplicity == 0 {
					continue;
				}
				let derivative = monomial.differentiate(ode.symbols()[axis])?;
				for (ci, coefficient) in ode
					.coefficient_terms()
					.iter()
					.enumerate()
					.filter(|(_, c)| c.row == axis)
				{
					let target_degree = degree - 1 + coefficient.powers.iter().sum::<u32>();
					if usize::try_from(target_degree)
						.map_err(|_| CfdError::InvalidInput("target degree overflow"))?
						> order
					{
						continue;
					}
					let product = derivative.multiply(&state_terms[ci])?;
					let (beta, multiplier) = product.terms().next().ok_or(CfdError::Assembly(
						"nonzero monomial derivative disappeared",
					))?;
					let target_degree: u32 = beta[..ode.dimension()].iter().sum();
					if usize::try_from(target_degree)
						.map_err(|_| CfdError::InvalidInput("target degree overflow"))?
						> order
					{
						continue;
					}
					let column =
						if target_degree == 0 {
							None
						} else {
							Some(*indices.get(&beta[..ode.dimension()]).ok_or(
								CfdError::Assembly("incomplete symmetric monomial enumeration"),
							)?)
						};
					let constant = SparsePolynomial::constant(
						ode.symbols().to_vec(),
						multiplier.clone(),
						plimits,
					)?;
					let multiplier = constant
						.lower(&mut F64Backend)?
						.evaluate(&mut F64Backend, &zero_inputs)?;
					let target_norm = column.map_or(1., |i| normalizations[i]);
					let exponent = i32::try_from(target_degree)
						.and_then(|b| i32::try_from(degree).map(|a| b - a))
						.map_err(|_| CfdError::InvalidInput("normalization degree overflow"))?;
					let factor = checked_factor(
						multiplier * normalizations[row] / target_norm * scale.powi(exponent),
					)?;
					if recipes.len() >= recipe_capacity {
						return Err(CfdError::Assembly(
							"symmetric recipe count underestimates contributions",
						));
					}
					recipes.push(Recipe {
						row,
						column,
						coefficient: ci,
						factor,
					});
				}
			}
		}
		if recipes.len() != recipe_capacity {
			return Err(CfdError::Assembly(
				"symmetric recipe count differs from complete enumeration",
			));
		}
		let first = (0..ode.dimension())
			.map(|axis| {
				let mut p = vec![0; ode.dimension()];
				p[axis] = 1;
				indices
					.get(&p)
					.copied()
					.ok_or(CfdError::Assembly("physical coordinate missing from lift"))
			})
			.collect::<Result<Vec<_>, _>>()?;
		Ok(Self {
			ode,
			order,
			scale,
			powers,
			normalizations,
			recipes,
			first,
			retained,
		})
	}
	/// Complete materialized hierarchy dimension.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.powers.len()
	}
	/// Complete physical dimension before lifting.
	#[must_use]
	pub fn physical_dimension(&self) -> usize {
		self.ode.dimension()
	}
	/// Maximum retained monomial degree.
	#[must_use]
	pub const fn order(&self) -> usize {
		self.order
	}
	/// Explicit physical normalization scale.
	#[must_use]
	pub const fn scale(&self) -> f64 {
		self.scale
	}
	/// All state exponent vectors, grouped in increasing degree.
	#[must_use]
	pub fn powers(&self) -> &[Vec<u32>] {
		&self.powers
	}
	/// Evaluate one initial amplitude without constructing the complete lifted state.
	/// # Errors
	/// Rejects invalid row, physical state, exponent or numerical overflow.
	pub fn lift_entry(&self, row: usize, state: &[f64]) -> Result<f64, CfdError> {
		if row >= self.dimension()
			|| state.len() != self.physical_dimension()
			|| state.iter().any(|v| !v.is_finite())
		{
			return Err(CfdError::InvalidInput(
				"invalid scalar Carleman initial query",
			));
		}
		let mut value = self.normalizations[row];
		for (&a, &power) in state.iter().zip(&self.powers[row]) {
			value *= (a / self.scale).powi(
				i32::try_from(power)
					.map_err(|_| CfdError::InvalidInput("monomial exponent overflow"))?,
			);
		}
		if !value.is_finite() {
			return Err(CfdError::InvalidInput("Carleman initial query overflow"));
		}
		Ok(value)
	}
	fn row_recipes(&self, row: usize) -> Result<&[Recipe], CfdError> {
		if row >= self.dimension() {
			return Err(CfdError::InvalidInput("Carleman row query"));
		}
		let start = self.recipes.partition_point(|r| r.row < row);
		let end = self.recipes.partition_point(|r| r.row <= row);
		Ok(&self.recipes[start..end])
	}
	/// Lift a complete physical state in the normalized symmetric basis.
	/// # Errors
	/// Rejects malformed/nonfinite state and numerical overflow.
	pub fn lift(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		if state.len() != self.physical_dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput(
				"invalid physical state for complete lift",
			));
		}
		self.powers
			.iter()
			.zip(&self.normalizations)
			.map(|(alpha, &c)| {
				let mut value = c;
				for (&a, &power) in state.iter().zip(alpha) {
					value *= (a / self.scale).powi(
						i32::try_from(power)
							.map_err(|_| CfdError::InvalidInput("monomial exponent overflow"))?,
					);
				}
				if value.is_finite() {
					Ok(value)
				} else {
					Err(CfdError::InvalidInput("Carleman lift overflow"))
				}
			})
			.collect()
	}
	/// Recover every physical coordinate from the complete degree-one block.
	/// # Errors
	/// Rejects malformed/nonfinite hierarchy and overflow.
	pub fn recover(&self, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		if state.len() != self.dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid lifted state"));
		}
		self.first
			.iter()
			.map(|&i| {
				let a = self.scale * state[i];
				if a.is_finite() {
					Ok(a)
				} else {
					Err(CfdError::InvalidInput("Carleman recovery overflow"))
				}
			})
			.collect()
	}
	/// Evaluate sparse truncated lifted dynamics including its external source.
	/// # Errors
	/// Rejects invalid states, time, or coefficient arithmetic.
	pub fn drift(&self, time: f64, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		if state.len() != self.dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid lifted state"));
		}
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite Carleman time"));
		}
		let values = self
			.ode
			.coefficient_terms()
			.iter()
			.map(|c| c.value(time))
			.collect::<Result<Vec<_>, _>>()?;
		let mut output = vec![0.; self.dimension()];
		for r in &self.recipes {
			output[r.row] += r.factor * values[r.coefficient] * r.column.map_or(1., |i| state[i]);
		}
		if output.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("Carleman drift overflow"));
		}
		Ok(output)
	}
	/// Classical integration of the same finite hierarchy, for truncation/reference evidence only.
	/// # Errors
	/// Rejects invalid steps/states and arithmetic overflow.
	pub fn integrate_rk4(
		&self,
		initial: &[f64],
		dt: f64,
		steps: u32,
	) -> Result<Vec<f64>, CfdError> {
		if !dt.is_finite() || dt <= 0. || steps > 1_000_000 {
			return Err(CfdError::InvalidInput("invalid Carleman reference step"));
		}
		let mut state = self.lift(initial)?;
		for step in 0..steps {
			let time = f64::from(step) * dt;
			let k1 = self.drift(time, &state)?;
			let shift = |k: &[f64], c: f64| {
				state
					.iter()
					.zip(k)
					.map(|(a, b)| a + c * dt * b)
					.collect::<Vec<_>>()
			};
			let k2 = self.drift(time + 0.5 * dt, &shift(&k1, 0.5))?;
			let k3 = self.drift(time + 0.5 * dt, &shift(&k2, 0.5))?;
			let k4 = self.drift(time + dt, &shift(&k3, 1.))?;
			for i in 0..state.len() {
				state[i] += dt * (k1[i] + 2. * k2[i] + 2. * k3[i] + k4[i]) / 6.;
			}
		}
		self.recover(&state)?;
		Ok(state)
	}

	/// Physical ODE residual induced by an arbitrary truncated hierarchy state.
	///
	/// Compares the degree-one derivative (including physical scaling) with
	/// F evaluated at the reconstructed complete physical state. Unlike the
	/// chain-rule truncation defect, this also detects off-manifold moments.
	/// # Errors
	/// Rejects invalid hierarchy states, time, or nonfinite residuals.
	pub fn physical_reconstruction_defect(
		&self,
		time: f64,
		state: &[f64],
	) -> Result<f64, CfdError> {
		let physical = self.recover(state)?;
		let full = self.ode.drift(time, &physical)?;
		let derivative = self.recover(&self.drift(time, state)?)?;
		let defect = full
			.iter()
			.zip(derivative)
			.fold(0_f64, |sum, (a, b)| sum.hypot(a - b));
		if !defect.is_finite() {
			return Err(CfdError::InvalidInput(
				"physical reconstruction defect overflow",
			));
		}
		Ok(defect)
	}
	/// Euclidean residual of the exact lifted chain rule minus the truncated hierarchy.
	/// # Errors
	/// Rejects malformed states and overflow. This is a measured defect, not a theorem.
	pub fn reconstruction_defect(&self, time: f64, state: &[f64]) -> Result<f64, CfdError> {
		let physical = self.ode.drift(time, state)?;
		let lifted = self.lift(state)?;
		let truncated = self.drift(time, &lifted)?;
		let mut residual = 0.;
		for (row, alpha) in self.powers.iter().enumerate() {
			let mut exact = 0.;
			for (axis, &power) in alpha.iter().enumerate() {
				if power == 0 {
					continue;
				}
				let mut value =
					self.normalizations[row] * f64::from(power) * physical[axis] / self.scale;
				for (j, (&a, &p)) in state.iter().zip(alpha).enumerate() {
					let exponent = p - u32::from(j == axis);
					value *= (a / self.scale).powi(
						i32::try_from(exponent)
							.map_err(|_| CfdError::InvalidInput("defect exponent"))?,
					);
				}
				exact += value;
			}
			residual += (exact - truncated[row]).powi(2);
		}
		if !residual.is_finite() {
			return Err(CfdError::InvalidInput("Carleman defect overflow"));
		}
		Ok(residual.sqrt())
	}
}
impl crate::stream_history::HistoryRowDynamics for SymmetricCarleman {
	fn dimension(&self) -> usize {
		self.dimension()
	}
	fn max_row_entries(&self) -> usize {
		self.ode.coefficient_terms().len()
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(self.retained)
	}
	fn row_query_bytes(&self) -> usize {
		self.ode
			.symbols()
			.len()
			.checked_mul(size_of::<f64>())
			.and_then(|n| n.checked_add(512))
			.unwrap_or(usize::MAX)
	}
	fn row_query_work(&self) -> usize {
		let terms = self
			.ode
			.coefficient_terms()
			.iter()
			.map(|c| c.symbolic().terms().count())
			.max()
			.unwrap_or(0);
		self.ode
			.symbols()
			.len()
			.checked_add(1)
			.and_then(|n| {
				n.checked_mul(
					usize::try_from(
						self.ode
							.coefficient_terms()
							.iter()
							.map(|c| c.symbolic().degree())
							.max()
							.unwrap_or(0),
					)
					.ok()?
					.checked_add(1)?,
				)
			})
			.and_then(|n| n.checked_mul(terms.checked_add(1)?))
			.and_then(|n| n.checked_mul(self.ode.coefficient_terms().len().checked_add(1)?))
			.and_then(|n| n.checked_mul(16))
			.and_then(|n| n.checked_add(usize::try_from(usize::BITS).ok()?.checked_mul(4)?))
			.unwrap_or(usize::MAX)
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
		for recipe in self.row_recipes(row)? {
			if let Some(column) = recipe.column {
				let value =
					recipe.factor * self.ode.coefficient_terms()[recipe.coefficient].value(time)?;
				if !value.is_finite() {
					return Err(CfdError::InvalidInput("Carleman row overflow"));
				}
				visitor(column, Complex64::new(value, 0.))?;
			}
		}
		Ok(())
	}
	fn source_entry(&self, time: f64, row: usize) -> Result<Complex64, CfdError> {
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite Carleman source time"));
		}
		let mut value = 0.;
		for recipe in self.row_recipes(row)? {
			if recipe.column.is_none() {
				value +=
					recipe.factor * self.ode.coefficient_terms()[recipe.coefficient].value(time)?;
			}
		}
		if !value.is_finite() {
			return Err(CfdError::InvalidInput("Carleman source entry overflow"));
		}
		Ok(Complex64::new(value, 0.))
	}
}
impl HistoryDynamics for SymmetricCarleman {
	fn dimension(&self) -> usize {
		self.dimension()
	}
	fn max_generator_entries(&self) -> usize {
		self.recipes.iter().filter(|r| r.column.is_some()).count()
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(self.retained)
	}
	fn visit_generator(
		&self,
		time: f64,
		visitor: &mut dyn FnMut(usize, usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite Carleman time"));
		}
		let values = self
			.ode
			.coefficient_terms()
			.iter()
			.map(|c| c.value(time))
			.collect::<Result<Vec<_>, _>>()?;
		for r in &self.recipes {
			if let Some(column) = r.column {
				let value = r.factor * values[r.coefficient];
				if !value.is_finite() {
					return Err(CfdError::InvalidInput("Carleman generator overflow"));
				}
				visitor(r.row, column, Complex64::new(value, 0.))?;
			}
		}
		Ok(())
	}
	fn source(&self, time: f64, output: &mut [Complex64]) -> Result<(), CfdError> {
		if !time.is_finite() || output.len() != self.dimension() {
			return Err(CfdError::InvalidInput("Carleman source dimension"));
		}
		output.fill(Complex64::new(0., 0.));
		for r in &self.recipes {
			if r.column.is_none() {
				let value = r.factor * self.ode.coefficient_terms()[r.coefficient].value(time)?;
				if !value.is_finite() {
					return Err(CfdError::InvalidInput("Carleman source overflow"));
				}
				output[r.row] += Complex64::new(value, 0.);
			}
		}
		if output.iter().any(|v| !v.re.is_finite()) {
			return Err(CfdError::InvalidInput("Carleman source sum overflow"));
		}
		Ok(())
	}
}

/// Small ordered-tensor reference, retained only for independent intertwining validation.
#[derive(Clone, Debug)]
pub struct OrderedCarlemanReference {
	ode: Arc<PolynomialOde>,
	order: usize,
	scale: f64,
	words: Vec<Vec<usize>>,
	recipes: Vec<Recipe>,
	retained: usize,
}
impl OrderedCarlemanReference {
	/// Construct complete ordered powers under the same explicit budgets (at most 4096 coordinates).
	/// # Errors
	/// Rejects malformed scale, excessive complete dimensions and index arithmetic overflow.
	#[allow(
		clippy::too_many_lines,
		reason = "Admission and complete symbolic recipe enumeration share a single bounded constructor"
	)]
	pub fn new(
		ode: Arc<PolynomialOde>,
		order: usize,
		scale: f64,
		limits: CarlemanLimits,
	) -> Result<Self, CfdError> {
		if !scale.is_finite() || scale <= 0. || order == 0 || order > limits.max_order || order > 64
		{
			return Err(CfdError::InvalidInput(
				"invalid ordered reference scale/order",
			));
		}
		let mut count = 0usize;
		let mut power = 1usize;
		for _ in 0..order {
			power = power
				.checked_mul(ode.dimension())
				.ok_or(CfdError::InvalidInput(
					"ordered reference dimension overflow",
				))?;
			count = count.checked_add(power).ok_or(CfdError::InvalidInput(
				"ordered reference dimension overflow",
			))?;
		}
		if count > 4096 {
			return Err(CfdError::InvalidInput(
				"ordered reference exceeds bounded validation dimension",
			));
		}
		// Every ordered position can produce two mixed quadratic replacements.
		let entry_capacity = count
			.checked_mul(order)
			.and_then(|n| n.checked_mul(ode.coefficient_terms().len()))
			.and_then(|n| n.checked_mul(2))
			.ok_or(CfdError::InvalidInput("ordered entry budget overflow"))?;
		let retained = admitted(&ode, order, count, limits, None)?
			.checked_add(
				entry_capacity
					.checked_mul(size_of::<Recipe>())
					.ok_or(CfdError::InvalidInput("ordered storage overflow"))?,
			)
			.and_then(|n| n.checked_add(count.checked_mul(order)?.checked_mul(32)?))
			.ok_or(CfdError::InvalidInput("ordered storage overflow"))?;
		let work = entry_capacity
			.checked_add(count)
			.and_then(|n| n.checked_mul(order.checked_add(ode.dimension())?.checked_add(8)?))
			.ok_or(CfdError::InvalidInput("ordered work overflow"))?;
		if entry_capacity > limits.max_entries
			|| retained > limits.max_bytes
			|| work > limits.max_work
		{
			return Err(CfdError::InvalidInput(
				"ordered reference exceeds admission budget",
			));
		}
		let mut words = Vec::with_capacity(count);
		let mut level = vec![Vec::new()];
		for _ in 0..order {
			let mut next = Vec::new();
			for word in &level {
				for axis in 0..ode.dimension() {
					let mut w = word.clone();
					w.push(axis);
					next.push(w);
				}
			}
			words.extend(next.iter().cloned());
			level = next;
		}
		let indices: BTreeMap<_, _> = words
			.iter()
			.cloned()
			.enumerate()
			.map(|(i, w)| (w, i))
			.collect();
		let mut recipes = Vec::with_capacity(entry_capacity);
		for (row, word) in words.iter().enumerate() {
			for (position, &axis) in word.iter().enumerate() {
				for (ci, coefficient) in ode
					.coefficient_terms()
					.iter()
					.enumerate()
					.filter(|(_, c)| c.row == axis)
				{
					let mut replacement = Vec::new();
					for (j, &p) in coefficient.powers.iter().enumerate() {
						for _ in 0..p {
							replacement.push(j);
						}
					}
					if word.len() - 1 + replacement.len() > order {
						continue;
					}
					let mixed = replacement.len() == 2 && replacement[0] != replacement[1];
					let variants = if mixed { 2 } else { 1 };
					for variant in 0..variants {
						let mut target = word[..position].to_vec();
						if variant == 0 {
							target.extend_from_slice(&replacement);
						} else {
							target.extend(replacement.iter().rev());
						}
						target.extend_from_slice(&word[position + 1..]);
						let column = if target.is_empty() {
							None
						} else {
							Some(
								*indices
									.get(&target)
									.ok_or(CfdError::Assembly("incomplete ordered powers"))?,
							)
						};
						let exponent = i32::try_from(target.len())
							.and_then(|b| i32::try_from(word.len()).map(|a| b - a))
							.map_err(|_| CfdError::InvalidInput("ordered scale exponent"))?;
						recipes.push(Recipe {
							row,
							column,
							coefficient: ci,
							factor: checked_factor(scale.powi(exponent) / f64::from(variants))?,
						});
					}
				}
			}
		}
		if recipes.len() > limits.max_entries {
			return Err(CfdError::InvalidInput("ordered recipes exceed budget"));
		}
		Ok(Self {
			ode,
			order,
			scale,
			words,
			recipes,
			retained,
		})
	}
	/// Full ordered dimension.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.words.len()
	}
	/// Embed normalized symmetric coordinates isometrically into ordered coordinates.
	/// # Errors
	/// Rejects different physical ODEs, order, scale or malformed state.
	#[allow(
		clippy::suspicious_operation_groupings,
		reason = "Comparisons intentionally validate different representation dimensions"
	)]
	pub fn embed_symmetric(
		&self,
		symmetric: &SymmetricCarleman,
		state: &[f64],
	) -> Result<Vec<f64>, CfdError> {
		if !Arc::ptr_eq(&self.ode, &symmetric.ode)
			|| self.order != symmetric.order
			|| self.scale.to_bits() != symmetric.scale.to_bits()
			|| state.len() != symmetric.dimension()
			|| state.iter().any(|v| !v.is_finite())
		{
			return Err(CfdError::InvalidInput(
				"incompatible ordered/symmetric embedding",
			));
		}
		let indices: BTreeMap<_, _> = symmetric
			.powers
			.iter()
			.enumerate()
			.map(|(i, p)| (p.as_slice(), i))
			.collect();
		let mut output = Vec::with_capacity(self.dimension());
		for word in &self.words {
			let mut alpha = vec![0; self.ode.dimension()];
			for &axis in word {
				alpha[axis] += 1;
			}
			let i = *indices
				.get(alpha.as_slice())
				.ok_or(CfdError::Assembly("symmetric embedding missing monomial"))?;
			output.push(state[i] / symmetric.normalizations[i]);
		}
		Ok(output)
	}
	/// Evaluate the independent ordered product-rule generator and its source.
	/// # Errors
	/// Rejects malformed states, time, and overflow.
	pub fn drift(&self, time: f64, state: &[f64]) -> Result<Vec<f64>, CfdError> {
		if state.len() != self.dimension() || state.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("invalid ordered reference state"));
		}
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite Carleman time"));
		}
		let values = self
			.ode
			.coefficient_terms()
			.iter()
			.map(|c| c.value(time))
			.collect::<Result<Vec<_>, _>>()?;
		let mut output = vec![0.; self.dimension()];
		for r in &self.recipes {
			output[r.row] += r.factor * values[r.coefficient] * r.column.map_or(1., |i| state[i]);
		}
		if output.iter().any(|v| !v.is_finite()) {
			return Err(CfdError::InvalidInput("ordered reference drift overflow"));
		}
		Ok(output)
	}
	/// Conservative reference working-storage admission.
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		self.retained
	}
}
