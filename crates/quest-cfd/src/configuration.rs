//! Tensor nodal DG on the complete constraint chart, with central periodic fluxes.
//!
//! Periodic closure is a configuration-domain truncation, not a physical condition
//! on the CFD drift. Boundary mass must be checked separately under refinement.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::as_conversions,
	clippy::manual_midpoint,
	reason = "Bounded tensor indices and explicit DG formulas are validated before assembly"
)]

use crate::{CfdError, PeriodicBdm1};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};

/// Reference Gauss-Lobatto nodal elements. The diagonal mass is quadrature mass.
#[derive(Clone, Debug)]
pub(crate) struct Element {
	pub nodes: Vec<f64>,
	pub weights: Vec<f64>,
	pub derivative: Vec<Vec<f64>>,
}
impl Element {
	pub fn retained_bytes(&self) -> usize {
		size_of::<Self>()
			+ self.nodes.capacity() * size_of::<f64>()
			+ self.weights.capacity() * size_of::<f64>()
			+ self.derivative.capacity() * size_of::<Vec<f64>>()
			+ self
				.derivative
				.iter()
				.map(|row| row.capacity() * size_of::<f64>())
				.sum::<usize>()
	}
	pub fn new(order: usize) -> Result<Self, CfdError> {
		match order {
			1 => Ok(Self {
				nodes: vec![-1.0, 1.0],
				weights: vec![1.0, 1.0],
				derivative: vec![vec![-0.5, 0.5], vec![-0.5, 0.5]],
			}),
			2 => Ok(Self {
				nodes: vec![-1.0, 0.0, 1.0],
				weights: vec![1.0 / 3.0, 4.0 / 3.0, 1.0 / 3.0],
				derivative: vec![
					vec![-1.5, 2.0, -0.5],
					vec![-0.5, 0.0, 0.5],
					vec![0.5, -2.0, 1.5],
				],
			}),
			_ => Err(CfdError::Unsupported(
				"configuration/time DG currently supports orders one and two".to_owned(),
			)),
		}
	}
}

/// Full tensor configuration space, with no omitted chart coordinates.
#[derive(Clone, Debug)]
pub struct ConfigurationGrid {
	axes: usize,
	lower: f64,
	upper: f64,
	cells: usize,
	nodes: Vec<f64>,
	weights: Vec<f64>,
	derivative: Vec<Vec<(usize, f64)>>,
	dimension: usize,
}
impl ConfigurationGrid {
	/// Admit the entire tensor dimension before allocating node or drift arrays.
	///
	/// # Errors
	/// Rejects invalid extents, unsupported order, overflow and exceeded budgets.
	#[allow(
		clippy::cast_precision_loss,
		reason = "Axis storage is bounded by the caller's admitted machine dimension"
	)]
	pub fn uniform(
		axes: usize,
		lower: f64,
		upper: f64,
		cells: usize,
		order: usize,
		max_dimension: usize,
	) -> Result<Self, CfdError> {
		if axes == 0 || cells == 0 || !lower.is_finite() || !upper.is_finite() || lower >= upper {
			return Err(CfdError::InvalidInput(
				"configuration grid requires positive counts and finite increasing bounds",
			));
		}
		let element = Element::new(order)?;
		let q = element.nodes.len();
		let axis_dimension = cells
			.checked_mul(q)
			.ok_or(CfdError::InvalidInput("configuration axis overflow"))?;
		let exponent = u32::try_from(axes)
			.map_err(|_| CfdError::InvalidInput("configuration exponent overflow"))?;
		let dimension = axis_dimension
			.checked_pow(exponent)
			.filter(|n| *n <= max_dimension)
			.ok_or(CfdError::InvalidInput(
				"full configuration tensor exceeds dimension budget",
			))?;
		let h = (upper - lower) / (cells as f64);
		if !h.is_finite() || h <= 0.0 {
			return Err(CfdError::InvalidInput("configuration cell width overflow"));
		}
		let mut nodes = Vec::new();
		let mut weights = Vec::new();
		let mut derivative = Vec::new();
		nodes
			.try_reserve_exact(axis_dimension)
			.map_err(|_| CfdError::InvalidInput("configuration node allocation"))?;
		weights
			.try_reserve_exact(axis_dimension)
			.map_err(|_| CfdError::InvalidInput("configuration weight allocation"))?;
		derivative
			.try_reserve_exact(axis_dimension)
			.map_err(|_| CfdError::InvalidInput("configuration derivative allocation"))?;
		for cell in 0..cells {
			for a in 0..q {
				nodes.push(lower + h * ((cell as f64) + 0.5 * (element.nodes[a] + 1.0)));
				weights.push(0.5 * h * element.weights[a]);
				let mut row: Vec<_> = (0..q)
					.map(|b| (cell * q + b, 2.0 / h * element.derivative[a][b]))
					.collect();
				if a == 0 {
					row.push((cell * q, 1.0 / (h * element.weights[a])));
					row.push((
						((cell + cells - 1) % cells) * q + q - 1,
						-1.0 / (h * element.weights[a]),
					));
				}
				if a == q - 1 {
					row.push((cell * q + a, -1.0 / (h * element.weights[a])));
					row.push((((cell + 1) % cells) * q, 1.0 / (h * element.weights[a])));
				}
				derivative.push(row);
			}
		}
		let minimum = weights.iter().copied().fold(f64::INFINITY, f64::min);
		let maximum = weights.iter().copied().fold(0.0, f64::max);
		let (mut lower_mass, mut upper_mass) = (1.0, 1.0);
		for _ in 0..axes {
			lower_mass *= minimum;
			upper_mass *= maximum;
			if lower_mass <= 0.0 || !upper_mass.is_finite() {
				return Err(CfdError::InvalidInput(
					"configuration tensor mass underflow/overflow",
				));
			}
		}
		Ok(Self {
			axes,
			lower,
			upper,
			cells,
			nodes,
			weights,
			derivative,
			dimension,
		})
	}
	/// Retained tensor-grid recipes and metadata; no lifted basis table is stored.
	/// # Errors
	/// Rejects byte-accounting overflow; allocator bookkeeping is excluded.
	pub fn retained_bytes(&self) -> Result<usize, CfdError> {
		let mut bytes = self
			.nodes
			.capacity()
			.checked_add(self.weights.capacity())
			.and_then(|n| n.checked_mul(size_of::<f64>()))
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.and_then(|n| {
				n.checked_add(
					self.derivative
						.capacity()
						.checked_mul(size_of::<Vec<(usize, f64)>>())?,
				)
			})
			.ok_or(CfdError::InvalidInput(
				"configuration retained storage overflow",
			))?;
		for row in &self.derivative {
			bytes = row
				.capacity()
				.checked_mul(size_of::<(usize, f64)>())
				.and_then(|n| bytes.checked_add(n))
				.ok_or(CfdError::InvalidInput(
					"configuration retained storage overflow",
				))?;
		}
		Ok(bytes)
	}
	/// Number of retained independent physical coordinates.
	#[must_use]
	pub const fn axes(&self) -> usize {
		self.axes
	}
	/// Number of configuration DG coefficients.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.dimension
	}
	/// Number of DG coefficients per configuration coordinate.
	#[must_use]
	pub const fn axis_dimension(&self) -> usize {
		self.nodes.len()
	}
	/// Domain boundaries on every chart coordinate.
	#[must_use]
	pub const fn bounds(&self) -> (f64, f64) {
		(self.lower, self.upper)
	}
	/// Physical chart coordinates associated with one tensor coefficient.
	#[must_use]
	pub fn point(&self, mut index: usize) -> Option<Vec<f64>> {
		if index >= self.dimension {
			return None;
		}
		let mut point = Vec::with_capacity(self.axes);
		for _ in 0..self.axes {
			point.push(self.nodes[index % self.nodes.len()]);
			index /= self.nodes.len();
		}
		Some(point)
	}
	/// Full tensor quadrature mass, useful for recovering unweighted amplitudes.
	#[must_use]
	pub fn weight(&self, mut index: usize) -> Option<f64> {
		if index >= self.dimension {
			return None;
		}
		let mut weight = 1.0;
		for _ in 0..self.axes {
			weight *= self.weights[index % self.nodes.len()];
			index /= self.nodes.len();
		}
		Some(weight)
	}
	/// Mass-scaled skew-adjoint `KvN` generator for the entire five-dimensional fixture.
	///
	/// # Errors
	/// Rejects dimension mismatch, drift failures and sparse resource limits.
	pub fn generator(
		&self,
		flow: &PeriodicBdm1,
		limits: SparseLimits,
	) -> Result<SparseMatrix, CfdError> {
		self.generator_from(flow.dimension(), |point| flow.drift(point), limits)
	}
	/// Assemble `W^(1/2) [-1/2 sum(F D + D F)] W^(-1/2)`.
	///
	/// The central SBP identity certifies algebraic skew-adjointness. It does not
	/// certify resolution of the transported probability distribution.
	///
	/// # Errors
	/// Rejects nonfinite drift, chart mismatch, allocation and operation budgets.
	pub fn generator_from(
		&self,
		drift_dimension: usize,
		drift: impl Fn(&[f64]) -> Result<Vec<f64>, CfdError>,
		limits: SparseLimits,
	) -> Result<SparseMatrix, CfdError> {
		if drift_dimension != self.axes {
			return Err(CfdError::InvalidInput(
				"KvN lift must retain every drift coordinate",
			));
		}
		let values = self
			.dimension
			.checked_mul(self.axes)
			.ok_or(CfdError::InvalidInput("drift sample size overflow"))?;
		let max_row = self.derivative.iter().map(Vec::len).max().unwrap_or(0);
		let entries = values
			.checked_mul(max_row)
			.ok_or(CfdError::InvalidInput("KvN entry count overflow"))?;
		let grid_bytes = self.retained_bytes()?;
		let bytes = entries
			.checked_mul(size_of::<(usize, usize, Complex64)>())
			.and_then(|n| n.checked_add(values.checked_mul(size_of::<f64>())?))
			.and_then(|n| n.checked_add(grid_bytes))
			.ok_or(CfdError::InvalidInput("KvN assembly byte overflow"))?;
		if self.dimension > limits.max_dimension
			|| entries > limits.max_entries
			|| bytes > limits.max_bytes
			|| entries > limits.max_work
		{
			return Err(CfdError::InvalidInput(
				"full KvN assembly exceeds sparse limits",
			));
		}
		let mut samples = Vec::new();
		samples
			.try_reserve_exact(values)
			.map_err(|_| CfdError::InvalidInput("drift allocation failed"))?;
		for row in 0..self.dimension {
			let point = self.point(row).ok_or(CfdError::Assembly("tensor index"))?;
			let sample = drift(&point)?;
			if sample.len() != self.axes || sample.iter().any(|x| !x.is_finite()) {
				return Err(CfdError::InvalidInput("drift dimension/nonfinite value"));
			}
			samples.extend(sample);
		}
		let mut triplets = Vec::new();
		triplets
			.try_reserve_exact(entries)
			.map_err(|_| CfdError::InvalidInput("KvN sparse allocation failed"))?;
		for row in 0..self.dimension {
			let mut stride = 1;
			for axis in 0..self.axes {
				let a = (row / stride) % self.nodes.len();
				for &(b, d) in &self.derivative[a] {
					let col = row - a * stride + b * stride;
					let value = -0.5
						* (samples[row * self.axes + axis] + samples[col * self.axes + axis])
						* d
						* (self.weights[a] / self.weights[b]).sqrt();
					if value != 0.0 {
						triplets.push((row, col, Complex64::new(value, 0.0)));
					}
				}
				stride *= self.nodes.len();
			}
		}
		drop(samples);
		let remaining = limits
			.max_bytes
			.checked_sub(grid_bytes)
			.ok_or(CfdError::InvalidInput(
				"configuration retained storage exceeds budget",
			))?;
		Ok(SparseMatrix::from_triplets(
			self.dimension,
			self.dimension,
			SparseFormat::Csr,
			triplets,
			SparseLimits {
				max_bytes: remaining,
				..limits
			},
		)?)
	}
	/// Smooth compact regularization, returned as normalized mass-weighted amplitudes.
	///
	/// # Errors
	/// Rejects an unresolved bump, incorrect dimension or invalid width/center.
	pub fn initial_bump(&self, center: &[f64], width: f64) -> Result<Vec<Complex64>, CfdError> {
		if center.len() != self.axes
			|| !width.is_finite()
			|| width <= 0.0
			|| center.iter().any(|x| !x.is_finite())
		{
			return Err(CfdError::InvalidInput("invalid initial regularization"));
		}
		let mut result = Vec::new();
		result
			.try_reserve_exact(self.dimension)
			.map_err(|_| CfdError::InvalidInput("initial state allocation failed"))?;
		for index in 0..self.dimension {
			let point = self
				.point(index)
				.ok_or(CfdError::Assembly("tensor index"))?;
			let log_amplitude = point.iter().zip(center).try_fold(0.0, |sum, (x, c)| {
				let r = (x - c) / width;
				if r.abs() >= 1.0 {
					None
				} else {
					Some(sum - 1.0 / (1.0 - r * r))
				}
			});
			let value = log_amplitude.map_or(0.0, |log| {
				log.exp() * self.weight(index).unwrap_or(0.0).sqrt()
			});
			result.push(Complex64::new(value, 0.0));
		}
		let norm = result.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
		if !norm.is_finite() || norm == 0.0 {
			return Err(CfdError::InvalidInput(
				"initial regularization unresolved or underflowed",
			));
		}
		for z in &mut result {
			*z /= norm;
		}
		Ok(result)
	}
	/// Probability in the outermost configuration cells (counted once at corners).
	///
	/// # Errors
	/// Rejects a nonfinite or incorrectly sized mass-weighted state.
	pub fn boundary_mass(&self, state: &[Complex64]) -> Result<f64, CfdError> {
		if state.len() != self.dimension
			|| state.iter().any(|z| !z.re.is_finite() || !z.im.is_finite())
		{
			return Err(CfdError::InvalidInput("invalid configuration state"));
		}
		let q = self.nodes.len() / self.cells;
		Ok(state
			.iter()
			.enumerate()
			.filter(|(index, _)| {
				let mut index = *index;
				(0..self.axes).any(|_| {
					let cell = (index % self.nodes.len()) / q;
					index /= self.nodes.len();
					cell == 0 || cell == self.cells - 1
				})
			})
			.map(|(_, z)| z.norm_sqr())
			.sum())
	}
}

/// Observable evidence for a possibly subnormalized configuration state.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ConfigurationObservables {
	pub probability: f64,
	pub boundary_probability: f64,
	pub coordinate_means: Vec<f64>,
	pub coordinate_variances: Vec<f64>,
	pub mean_kinetic_energy: f64,
}
impl ConfigurationGrid {
	/// Compute normalized moments, retaining probability and outer-cell leakage separately.
	/// # Errors
	/// Rejects invalid/zero probability or a failed physical energy evaluation.
	pub fn observables(
		&self,
		state: &[Complex64],
		energy: impl Fn(&[f64]) -> Result<f64, CfdError>,
	) -> Result<ConfigurationObservables, CfdError> {
		let boundary_probability = self.boundary_mass(state)?;
		let probability = state.iter().map(Complex64::norm_sqr).sum::<f64>();
		if !probability.is_finite() || probability <= 0.0 {
			return Err(CfdError::InvalidInput(
				"zero or nonfinite configuration probability",
			));
		}
		let mut means = vec![0.0; self.axes];
		let mut second = vec![0.0; self.axes];
		let mut mean_energy = 0.0;
		for (index, z) in state.iter().enumerate() {
			let point = self
				.point(index)
				.ok_or(CfdError::Assembly("configuration observable index"))?;
			let mass = z.norm_sqr() / probability;
			for ((mean, square), x) in means.iter_mut().zip(&mut second).zip(&point) {
				*mean += mass * x;
				*square += mass * x * x;
			}
			let value = energy(&point)?;
			if !value.is_finite() {
				return Err(CfdError::InvalidInput("nonfinite physical observable"));
			}
			mean_energy += mass * value;
		}
		if means.iter().any(|x| !x.is_finite()) || !mean_energy.is_finite() {
			return Err(CfdError::InvalidInput("configuration observable overflow"));
		}
		second.fill(0.0);
		for (index, z) in state.iter().enumerate() {
			let point = self
				.point(index)
				.ok_or(CfdError::Assembly("configuration observable index"))?;
			let mass = z.norm_sqr() / probability;
			for ((variance, mean), x) in second.iter_mut().zip(&means).zip(&point) {
				*variance += mass * (x - mean).powi(2);
			}
		}
		if second.iter().any(|x| !x.is_finite()) {
			return Err(CfdError::InvalidInput("configuration variance overflow"));
		}
		let variances = second;
		Ok(ConfigurationObservables {
			probability,
			boundary_probability,
			coordinate_means: means,
			coordinate_variances: variances,
			mean_kinetic_energy: mean_energy,
		})
	}
}
