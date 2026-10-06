//! Bounded amplitude interpolation of DG histories, preserving interference.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Admitted history extents and fixed DG1/DG2 arrays bound interpolation arithmetic"
)]
use crate::{
	CfdError,
	probability_observation::TemporalNodeSide,
	stream_history::{HistoryRowDynamics, TemporalHistoryRecipe},
};
use quest_numerics::{Complex64, Interval};

/// Per-coordinate reference query admission. Caller callback costs are declarations.
#[derive(Clone, Copy, Debug)]
pub struct TemporalInterpolationLimits {
	pub max_dimension: usize,
	pub max_bytes: usize,
	pub max_query_work: usize,
	pub amplitude_query_work: usize,
	pub amplitude_query_scratch_bytes: usize,
	pub amplitude_source_retained_bytes: usize,
}
impl Default for TemporalInterpolationLimits {
	fn default() -> Self {
		Self {
			max_dimension: usize::MAX,
			max_bytes: 1_048_576,
			max_query_work: 1_000_000,
			amplitude_query_work: 1,
			amplitude_query_scratch_bytes: 0,
			amplitude_source_retained_bytes: 0,
		}
	}
}
/// Costs of one recovered configuration amplitude; no measurement circuit is implied.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct TemporalInterpolationResources {
	pub retained_bytes: usize,
	pub query_peak_bytes: usize,
	pub query_work: usize,
	pub amplitude_queries: usize,
}
/// One-sided slab-local interpolation of configuration amplitudes, not probabilities.
///
/// The same coefficients define a rectangular linear map with one disjoint row per
/// configuration index. Its operator norm is the Euclidean norm of the weights.
/// A coherent implementation must separately encode that map and pay its success
/// normalization; this scalar reference does not perform coherent time projection.
#[derive(Clone, Debug)]
pub struct TemporalInterpolation {
	semantics: [u64; 11],
	slab: usize,
	n: usize,
	q: usize,
	history: usize,
	weights: [f64; 3],
	time: f64,
	side: TemporalNodeSide,
	norm_upper: f64,
	resources: TemporalInterpolationResources,
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("temporal interpolation admission/index/overflow")
}
impl TemporalInterpolation {
	/// Select a fraction in [0,1] of an explicit slab. At a shared boundary,
	/// `(slab,1)` and `(slab+1,0)` retain the two distinct DG traces.
	/// # Errors
	/// Rejects unsupported metadata, invalid slab/fraction and byte/work budgets.
	#[allow(
		clippy::cast_precision_loss,
		clippy::as_conversions,
		clippy::float_cmp,
		reason = "History slabs are bounded at 2^53; exact endpoint equality determines the one-sided trace"
	)]
	pub fn new<D: HistoryRowDynamics>(
		history: &TemporalHistoryRecipe<'_, D>,
		slab: usize,
		fraction: f64,
		limits: TemporalInterpolationLimits,
	) -> Result<Self, CfdError> {
		let semantics = history.semantic_words()?;
		let cv = |i: usize| usize::try_from(semantics[i]).map_err(|_| invalid());
		let (cells, order, n, q, total) = (cv(3)?, cv(4)?, cv(5)?, cv(6)?, cv(9)?);
		if slab >= cells
			|| !fraction.is_finite()
			|| !(0. ..=1.).contains(&fraction)
			|| total > limits.max_dimension
			|| !matches!((order, q), (1, 2) | (2, 3))
		{
			return Err(invalid());
		}
		let weights = if order == 1 {
			[1. - fraction, fraction, 0.]
		} else {
			[
				(1. - fraction) * (1. - 2. * fraction),
				4. * fraction * (1. - fraction),
				fraction * (2. * fraction - 1.),
			]
		};
		let mut norm = Interval::point(0.)?;
		for &w in &weights[..q] {
			norm = norm.checked_add(Interval::point(w)?.square()?)?;
		}
		let norm_upper = norm.sqrt()?.upper();
		let retained_bytes = size_of::<Self>();
		let query_peak_bytes = retained_bytes
			.checked_add(limits.amplitude_source_retained_bytes)
			.and_then(|v| v.checked_add(limits.amplitude_query_scratch_bytes))
			.and_then(|v| v.checked_add(256))
			.ok_or_else(invalid)?;
		let query_work = limits
			.amplitude_query_work
			.checked_add(64)
			.and_then(|v| v.checked_mul(q))
			.and_then(|v| v.checked_add(64))
			.ok_or_else(invalid)?;
		let time = f64::from_bits(semantics[2]) * ((slab as f64) + fraction);
		if !time.is_finite()
			|| !norm_upper.is_finite()
			|| query_peak_bytes > limits.max_bytes
			|| query_work > limits.max_query_work
		{
			return Err(invalid());
		}
		Ok(Self {
			semantics,
			slab,
			n,
			q,
			history: total,
			weights,
			time,
			side: if fraction == 0. {
				TemporalNodeSide::SlabLeft
			} else if fraction == 1. {
				TemporalNodeSide::SlabRight
			} else {
				TemporalNodeSide::Interior
			},
			norm_upper,
			resources: TemporalInterpolationResources {
				retained_bytes,
				query_peak_bytes,
				query_work,
				amplitude_queries: q,
			},
		})
	}
	#[must_use]
	pub const fn configuration_dimension(&self) -> usize {
		self.n
	}
	#[must_use]
	pub const fn history_dimension(&self) -> usize {
		self.history
	}
	#[must_use]
	pub const fn slab(&self) -> usize {
		self.slab
	}
	#[must_use]
	pub const fn node_count(&self) -> usize {
		self.q
	}
	#[must_use]
	pub fn weights(&self) -> &[f64] {
		&self.weights[..self.q]
	}
	#[must_use]
	pub const fn semantic_words(&self) -> [u64; 11] {
		self.semantics
	}
	#[must_use]
	pub const fn physical_time(&self) -> f64 {
		self.time
	}
	#[must_use]
	pub const fn side(&self) -> TemporalNodeSide {
		self.side
	}
	/// Upper bound on amplification of whole-history amplitude l2 error by the
	/// stored interpolation map. It excludes interpolation consistency and floating
	/// evaluation error; conditional observable normalization can amplify further.
	#[must_use]
	pub const fn norm_upper_bound(&self) -> f64 {
		self.norm_upper
	}
	#[must_use]
	pub const fn resources(&self) -> TemporalInterpolationResources {
		self.resources
	}
	/// Logical column of the original history, without encoding or flag qubits.
	/// # Errors
	/// Rejects out-of-range configuration/node indices and integer overflow.
	pub fn history_index(&self, configuration: usize, node: usize) -> Result<usize, CfdError> {
		if configuration >= self.n || node >= self.q {
			return Err(invalid());
		}
		self.slab
			.checked_mul(self.q)
			.and_then(|v| v.checked_add(node))
			.and_then(|v| v.checked_mul(self.n))
			.and_then(|v| v.checked_add(configuration))
			.filter(|&v| v < self.history)
			.ok_or_else(invalid)
	}
	/// Recover one physical configuration amplitude from physical history amplitudes.
	/// Apply the inverse's physical scaling before or after this linear map, exactly
	/// once. Squaring this result preserves temporal interference. The callback must
	/// follow its declared storage/work costs; it is a classical/reference query.
	/// # Errors
	/// Rejects invalid indices, callback errors and nonfinite input/output amplitudes.
	pub fn amplitude(
		&self,
		configuration: usize,
		mut query: impl FnMut(usize) -> Result<Complex64, CfdError>,
	) -> Result<Complex64, CfdError> {
		self.history_index(configuration, 0)?;
		let mut sum = Complex64::new(0., 0.);
		for node in 0..self.q {
			let value = query(self.history_index(configuration, node)?)?;
			if !value.re.is_finite() || !value.im.is_finite() {
				return Err(invalid());
			}
			sum += value * self.weights[node];
		}
		if !sum.re.is_finite() || !sum.im.is_finite() {
			return Err(invalid());
		}
		Ok(sum)
	}
}
