//! Diagonal `KvN` observations and conditional statistical cost premises.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Admitted dimensions and finite interval arithmetic bound scalar observation formulas"
)]
use crate::{
	CfdError, PeriodicBdm1,
	configuration::ConfigurationGrid,
	kvn_recipe::KvnRecipeLimits,
	physical_space::PhysicalSpace,
	stream_history::{HistoryRowDynamics, TemporalHistoryRecipe},
};
use quest_numerics::Interval;
/// Supplied finite closed range of a real diagonal observable, checked on queried values.
/// A finite query test alone does not certify unqueried values for future measurements.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct DiagonalRange {
	lower: f64,
	upper: f64,
}
impl DiagonalRange {
	/// # Errors
	/// Rejects nonfinite or reversed endpoints.
	pub fn new(lower: f64, upper: f64) -> Result<Self, CfdError> {
		if !lower.is_finite() || !upper.is_finite() || lower > upper {
			return Err(CfdError::InvalidInput("diagonal observable range"));
		}
		Ok(Self { lower, upper })
	}
	#[must_use]
	pub const fn lower(self) -> f64 {
		self.lower
	}
	#[must_use]
	pub const fn upper(self) -> f64 {
		self.upper
	}
	#[must_use]
	pub fn contains(self, value: f64) -> bool {
		value.is_finite() && value >= self.lower && value <= self.upper
	}
}
/// Every attempted shot independently repeats complete preparation, inverse and measurement.
/// The joint postselection bound must come from caller evidence, never simulator frequency.
#[derive(Clone, Copy, Debug)]
pub struct SamplingRequest<'a> {
	pub range: DiagonalRange,
	pub absolute_error: f64,
	pub failure_probability: f64,
	pub joint_success_lower_bound: f64,
	pub success_bound_provenance: &'a str,
	pub max_selected_shots: u64,
	pub max_attempted_shots: u64,
	pub max_provenance_bytes: usize,
	/// A caller bound for all fixed biases of this conditional normalized observable,
	/// including postselection amplification and initial-ensemble to physical-target error.
	/// A raw amplitude-error bound alone is insufficient.
	pub systematic_bias_bound: Option<f64>,
}
/// Statistical estimate conditional on the supplied range/probability and i.i.d. trials.
/// No measurements execute, and neither premise is certified by this planner.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct SamplingPlan<'a> {
	pub selected_shots: u64,
	pub attempted_shots: u64,
	pub range: DiagonalRange,
	pub statistical_absolute_error: f64,
	pub failure_probability: f64,
	pub caller_joint_success_lower_bound: f64,
	pub success_bound_provenance: &'a str,
	pub systematic_bias_bound: Option<f64>,
	pub total_error_bound: Option<f64>,
	pub quantum_measurements_executed: bool,
}
#[allow(
	clippy::cast_possible_truncation,
	clippy::cast_sign_loss,
	clippy::as_conversions,
	reason = "Positive finite values at most 2^53 convert to exact bounded integer shot counts"
)]
fn ceiling_count(value: f64, limit: u64) -> Result<u64, CfdError> {
	if !value.is_finite() || value < 0. || value > 9_007_199_254_740_992. {
		return Err(CfdError::InvalidInput("sampling count not representable"));
	}
	let count = (value.ceil() as u64).max(1);
	if count > limit {
		return Err(CfdError::InvalidInput("sampling shot budget"));
	}
	Ok(count)
}
/// Outward Hoeffding mean bound plus an independent conservative binomial lower-tail bound.
///
/// Reserve delta/2 each for estimation error and failure to collect N successes.
/// `N >= range² ln(4/delta)/(2 epsilon²); M*p_min >= max(2N,8 ln(2/delta))`.
/// Chernoff then bounds `P[successes<N]` by `exp(-M*p_min/8)`. Use the first N
/// successes; conditioning on getting enough successes does not alter their i.i.d. values.
/// Fixed preparation, polynomial, discretization and floating biases are separate.
/// # Errors
/// Rejects invalid premises, absent/budgeted provenance, nonrepresentable counts or shot caps.
#[allow(
	clippy::cast_precision_loss,
	clippy::as_conversions,
	clippy::float_cmp,
	reason = "Admitted counts at most 2^53 are exact; equal endpoints establish a constant range without epsilon proofs"
)]
pub fn plan_sampling(request: SamplingRequest<'_>) -> Result<SamplingPlan<'_>, CfdError> {
	let r = request;
	if r.success_bound_provenance.len() > r.max_provenance_bytes
		|| r.success_bound_provenance.trim().is_empty()
		|| !r.absolute_error.is_finite()
		|| r.absolute_error <= 0.
		|| !r.failure_probability.is_finite()
		|| r.failure_probability <= 0.
		|| r.failure_probability >= 1.
		|| !r.joint_success_lower_bound.is_finite()
		|| r.joint_success_lower_bound <= 0.
		|| r.joint_success_lower_bound > 1.
		|| r.systematic_bias_bound
			.is_some_and(|x| !x.is_finite() || x < 0.)
	{
		return Err(CfdError::InvalidInput("sampling premises or provenance"));
	}
	let delta = Interval::point(r.failure_probability)?;
	let width = Interval::point(r.range.upper)?.checked_sub(Interval::point(r.range.lower)?)?;
	// Constant outcomes have zero statistical variation even when epsilon² underflows.
	let count = if r.range.lower == r.range.upper {
		0.
	} else {
		width
			.square()?
			.checked_mul(Interval::point(4.)?.checked_div(delta)?.ln()?)?
			.checked_div(
				Interval::point(2.)?.checked_mul(Interval::point(r.absolute_error)?.square()?)?,
			)?
			.upper()
	};
	let selected = ceiling_count(count, r.max_selected_shots)?;
	let twice = Interval::point(selected as f64)?
		.checked_mul(Interval::point(2.)?)?
		.upper();
	let tail = Interval::point(8.)?
		.checked_mul(Interval::point(2.)?.checked_div(delta)?.ln()?)?
		.upper();
	let attempted = ceiling_count(
		Interval::point(twice.max(tail))?
			.checked_div(Interval::point(r.joint_success_lower_bound)?)?
			.upper(),
		r.max_attempted_shots,
	)?;
	let total_error_bound = r
		.systematic_bias_bound
		.map(|bias| {
			Ok::<_, CfdError>(
				Interval::point(bias)?
					.checked_add(Interval::point(r.absolute_error)?)?
					.upper(),
			)
		})
		.transpose()?;
	Ok(SamplingPlan {
		selected_shots: selected,
		attempted_shots: attempted,
		range: r.range,
		statistical_absolute_error: r.absolute_error,
		failure_probability: r.failure_probability,
		caller_joint_success_lower_bound: r.joint_success_lower_bound,
		success_bound_provenance: r.success_bound_provenance,
		systematic_bias_bound: r.systematic_bias_bound,
		total_error_bound,
		quantum_measurements_executed: false,
	})
}
/// DG nodes on neighboring slabs at the same time represent different one-sided traces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum TemporalNodeSide {
	SlabLeft,
	Interior,
	SlabRight,
}
/// Proven nodal block of this history basis, not an arbitrary modal coefficient slice.
///
/// Its coefficients are configuration-mass-scaled amplitudes without temporal mass scaling.
/// Quadrature weight is reported separately; an instantaneous expectation does not multiply it.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct TemporalNodeSelection {
	semantics: [u64; 11],
	start: usize,
	end: usize,
	slab: usize,
	node: usize,
	time: f64,
	quadrature_weight: f64,
	side: TemporalNodeSide,
}
impl TemporalNodeSelection {
	/// # Errors
	/// Rejects out-of-range slabs/nodes and nonrepresentable temporal metadata.
	#[allow(
		clippy::cast_precision_loss,
		clippy::as_conversions,
		reason = "History construction bounds slab indices at 2^53; fixed node indices are at most two"
	)]
	pub fn new<D: HistoryRowDynamics>(
		history: &TemporalHistoryRecipe<'_, D>,
		slab: usize,
		node: usize,
	) -> Result<Self, CfdError> {
		let semantics = history.semantic_words()?;
		let cv = |i: usize| {
			usize::try_from(semantics[i])
				.map_err(|_| CfdError::InvalidInput("temporal observation metadata"))
		};
		let (cells, order, n, q) = (cv(3)?, cv(4)?, cv(5)?, cv(6)?);
		if slab >= cells || node >= q {
			return Err(CfdError::InvalidInput("temporal observation node"));
		}
		let dt = f64::from_bits(semantics[2]);
		let fraction = if order == 1 {
			node as f64
		} else {
			0.5 * (node as f64)
		};
		let time = dt * ((slab as f64) + fraction);
		let weight = if order == 1 {
			1.
		} else if node == 1 {
			4. / 3.
		} else {
			1. / 3.
		};
		let start = slab
			.checked_mul(q)
			.and_then(|x| x.checked_add(node))
			.and_then(|x| x.checked_mul(n))
			.ok_or(CfdError::InvalidInput("temporal observation range"))?;
		let end = start
			.checked_add(n)
			.ok_or(CfdError::InvalidInput("temporal observation range"))?;
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("temporal observation time"));
		}
		Ok(Self {
			semantics,
			start,
			end,
			slab,
			node,
			time,
			quadrature_weight: 0.5 * dt * weight,
			side: if node == 0 {
				TemporalNodeSide::SlabLeft
			} else if node == q - 1 {
				TemporalNodeSide::SlabRight
			} else {
				TemporalNodeSide::Interior
			},
		})
	}
	#[must_use]
	pub const fn contains(self, index: usize) -> bool {
		index >= self.start && index < self.end
	}
	#[must_use]
	pub const fn time(self) -> f64 {
		self.time
	}
	#[must_use]
	pub const fn slab(self) -> usize {
		self.slab
	}
	#[must_use]
	pub const fn node(self) -> usize {
		self.node
	}
	#[must_use]
	pub const fn side(self) -> TemporalNodeSide {
		self.side
	}
	#[must_use]
	pub const fn quadrature_weight(self) -> f64 {
		self.quadrature_weight
	}
	#[must_use]
	pub const fn configuration_index(self, index: usize) -> Option<usize> {
		if self.contains(index) {
			Some(index - self.start)
		} else {
			None
		}
	}
	#[cfg(feature = "distributed")]
	pub(crate) fn metadata_words(self) -> Result<[u64; 14], CfdError> {
		let mut words = [0; 14];
		words[0] = 1;
		words[1] =
			u64::try_from(self.slab).map_err(|_| CfdError::InvalidInput("temporal slab width"))?;
		words[2] =
			u64::try_from(self.node).map_err(|_| CfdError::InvalidInput("temporal node width"))?;
		words[3..].copy_from_slice(&self.semantics);
		Ok(words)
	}
	#[cfg(feature = "distributed")]
	pub(crate) fn matches(self, words: [u64; 11]) -> bool {
		self.semantics == words
	}
}
/// Generic coefficient projections have no physical-time interpretation.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub enum HistoryProjection {
	CoefficientProjection,
	TemporalNode(TemporalNodeSelection),
}
/// Full-coordinate chart kinetic energy `1/2 sum a_i²` at a configuration node.
///
/// This diagonal represents a conditional `KvN` ensemble expectation, not energy of its mean.
/// Numerical chart whitening is inherited; no affine boundary lifting is assumed.
pub struct KvnKineticEnergy<'a> {
	grid: &'a ConfigurationGrid,
	range: DiagonalRange,
	retained_bytes: usize,
	query_work: usize,
	construction_work: usize,
}
impl<'a> KvnKineticEnergy<'a> {
	/// # Errors
	/// Rejects missing coordinates, nonfinite range and declared storage/work limits.
	pub fn periodic_bdm1(
		grid: &'a ConfigurationGrid,
		model: &PeriodicBdm1,
		limits: KvnRecipeLimits,
	) -> Result<Self, CfdError> {
		Self::new(grid, model.dimension(), limits)
	}
	/// Homogeneous full box chart; prescribed tangential cavity data do not add a normal lift.
	/// # Errors
	/// Rejects missing coordinates, nonfinite range and declared storage/work limits.
	pub fn box_space(
		grid: &'a ConfigurationGrid,
		model: &PhysicalSpace,
		limits: KvnRecipeLimits,
	) -> Result<Self, CfdError> {
		Self::new(grid, model.dimension(), limits)
	}
	fn new(
		grid: &'a ConfigurationGrid,
		dimension: usize,
		limits: KvnRecipeLimits,
	) -> Result<Self, CfdError> {
		let retained_bytes = grid
			.retained_bytes()?
			.checked_add(size_of::<Self>())
			.ok_or(CfdError::InvalidInput("energy recipe bytes"))?;
		let query_work = grid
			.axes()
			.checked_mul(64)
			.and_then(|x| x.checked_add(64))
			.ok_or(CfdError::InvalidInput("energy recipe work"))?;
		let construction_work = grid
			.axis_dimension()
			.checked_mul(16)
			.and_then(|x| x.checked_add(query_work))
			.ok_or(CfdError::InvalidInput("energy construction work"))?;
		if grid.axes() != dimension
			|| grid.dimension() > limits.max_dimension
			|| retained_bytes > limits.max_bytes
			|| construction_work > limits.max_query_work
		{
			return Err(CfdError::InvalidInput(
				"full-coordinate energy recipe admission",
			));
		}
		// Monotone identical nonnegative multiply/add order bounds every rounded node value.
		let mut extreme = 0_f64;
		for i in 0..grid.axis_dimension() {
			let x = grid
				.axis_node(i)
				.ok_or(CfdError::InvalidInput("energy range node"))?;
			if !x.is_finite() {
				return Err(CfdError::InvalidInput("nonfinite energy range node"));
			}
			extreme = extreme.max(x.abs());
		}
		let mut maximum = 0.;
		for _ in 0..grid.axes() {
			maximum += 0.5 * extreme * extreme;
		}
		let range = DiagonalRange::new(0., maximum)?;
		Ok(Self {
			grid,
			range,
			retained_bytes,
			query_work,
			construction_work,
		})
	}
	#[must_use]
	pub const fn range(&self) -> DiagonalRange {
		self.range
	}
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		self.retained_bytes
	}
	#[must_use]
	pub const fn query_work(&self) -> usize {
		self.query_work
	}
	#[must_use]
	pub const fn construction_work(&self) -> usize {
		self.construction_work
	}
	/// No tensor-wide table or point-vector allocation; every coordinate is visited.
	/// # Errors
	/// Rejects invalid indices, node metadata, overflow or range violations.
	pub fn value(&self, mut index: usize) -> Result<f64, CfdError> {
		if index >= self.grid.dimension() {
			return Err(CfdError::InvalidInput("energy configuration index"));
		}
		let mut energy = 0.;
		let radix = self.grid.axis_dimension();
		for _ in 0..self.grid.axes() {
			let value = self
				.grid
				.axis_node(index % radix)
				.ok_or(CfdError::InvalidInput("energy axis node"))?;
			energy += 0.5 * value * value;
			index /= radix;
		}
		if !self.range.contains(energy) {
			return Err(CfdError::InvalidInput("energy observable range/overflow"));
		}
		Ok(energy)
	}
}

#[cfg(test)]
mod tests {
	use super::ceiling_count;
	#[test]
	fn shot_count_conversion_at_exact_binary64_integer_limit() {
		assert_eq!(
			ceiling_count(9_007_199_254_740_992., u64::MAX).ok(),
			Some(9_007_199_254_740_992)
		);
		assert_eq!(
			ceiling_count(9_007_199_254_740_991., u64::MAX).ok(),
			Some(9_007_199_254_740_991)
		);
		assert!(ceiling_count(9_007_199_254_740_992_f64.next_up(), u64::MAX).is_err());
		assert!(ceiling_count(9_007_199_254_740_992., 9_007_199_254_740_991).is_err());
	}
}
