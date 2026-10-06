//! Generated full-coordinate mass-scaled `KvN` rows and scalar initial amplitudes.
//!
//! The physical polynomial ODE is already prepared and fully retained. This removes
//! configuration-wide drift/generator tables, not the cost of the physical input.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked dimensions and fixed conservative query envelopes bound local arithmetic"
)]
use crate::{
	CfdError, configuration::ConfigurationGrid, polynomial::PolynomialOde,
	stream_history::HistoryRowDynamics,
};
use quest_numerics::Complex64;
/// Complete tensor index, retained/query payload, and query-operation admission.
#[derive(Clone, Copy, Debug)]
pub struct KvnRecipeLimits {
	pub max_dimension: usize,
	pub max_bytes: usize,
	pub max_query_work: usize,
}
impl Default for KvnRecipeLimits {
	fn default() -> Self {
		Self {
			max_dimension: usize::MAX,
			max_bytes: 268_435_456,
			max_query_work: 100_000_000,
		}
	}
}
const fn overflow() -> CfdError {
	CfdError::InvalidInput("KvN recipe resource overflow")
}
/// Resource receipt; counts include borrowed complete grid and physical kernels.
#[derive(Clone, Copy, Debug)]
pub struct KvnRecipeResources {
	pub retained_bytes: usize,
	pub row_query_bytes: usize,
	/// Conservative elementary arithmetic/index/validation operations, not CPU cycles.
	pub row_query_work: usize,
	pub maximum_row_entries: usize,
	pub maximum_drift_evaluations_per_row: usize,
}
/// Immutable row source for `W^(1/2) [-1/2 sum(FD+DF)] W^(-1/2)`.
///
/// Every physical chart coordinate is retained. The existing periodic central-DG
/// configuration closure is preserved. No complete drift, generator or RHS is stored.
/// Rows emit deterministic duplicate contributions; consumers may coalesce them.
/// Physical forcing is part of the drift; the linear `KvN` equation has zero source.
pub struct KvnHistoryRecipe<'a> {
	grid: &'a ConfigurationGrid,
	ode: &'a PolynomialOde,
	resources: KvnRecipeResources,
}
impl<'a> KvnHistoryRecipe<'a> {
	/// Admit the prepared complete physical input and every repeated drift query.
	/// # Errors
	/// Rejects coordinate reduction, malformed grid coefficients and resource overflow/budgets.
	pub fn new(
		grid: &'a ConfigurationGrid,
		ode: &'a PolynomialOde,
		limits: KvnRecipeLimits,
	) -> Result<Self, CfdError> {
		if grid.axes() != ode.dimension()
			|| grid.dimension() > limits.max_dimension
			|| limits.max_query_work == 0
		{
			return Err(CfdError::InvalidInput(
				"KvN complete coordinate or query budget",
			));
		}
		let retained_bytes = grid
			.retained_bytes()?
			.checked_add(ode.retained_bytes())
			.and_then(|v| v.checked_add(size_of::<Self>()))
			.ok_or_else(overflow)?;
		let row_query_bytes = grid
			.axes()
			.checked_mul(8)
			.and_then(|v| v.checked_add(32))
			.and_then(|v| v.checked_mul(size_of::<f64>()))
			.and_then(|v| v.checked_add(512))
			.ok_or_else(overflow)?;
		if retained_bytes
			.checked_add(row_query_bytes)
			.is_none_or(|v| v > limits.max_bytes)
		{
			return Err(CfdError::InvalidInput("KvN retained/query storage budget"));
		}
		let mut axis_entries = 0;
		for node in 0..grid.axis_dimension() {
			let row = grid
				.axis_derivative_row(node)
				.ok_or(CfdError::Assembly("configuration derivative row"))?;
			let weight = grid
				.axis_weight(node)
				.ok_or(CfdError::Assembly("configuration axis weight"))?;
			let point = grid
				.axis_node(node)
				.ok_or(CfdError::Assembly("configuration axis point"))?;
			if !point.is_finite()
				|| !weight.is_finite()
				|| weight <= 0.
				|| row
					.iter()
					.any(|&(i, v)| i >= grid.axis_dimension() || !v.is_finite())
			{
				return Err(CfdError::InvalidInput("nonfinite KvN grid coefficient"));
			}
			axis_entries = axis_entries.max(row.len());
		}
		let maximum_row_entries = grid.axes().checked_mul(axis_entries).ok_or_else(overflow)?;
		let evaluations = maximum_row_entries.checked_add(1).ok_or_else(overflow)?;
		// Prepared PolynomialKernel evaluation scans every symbol for every component,
		// then every term's exponents and each repeated multiplication (including time).
		let symbols = ode.symbols().len();
		let mut drift_work = ode
			.dimension()
			.checked_mul(
				symbols
					.checked_mul(16)
					.and_then(|v| v.checked_add(32))
					.ok_or_else(overflow)?,
			)
			.ok_or_else(overflow)?;
		for polynomial in ode.components() {
			for (powers, _) in polynomial.terms() {
				let degree = powers
					.iter()
					.try_fold(0usize, |sum, &v| sum.checked_add(usize::try_from(v).ok()?))
					.ok_or_else(overflow)?;
				let work = symbols
					.checked_add(degree)
					.and_then(|v| v.checked_add(8))
					.and_then(|v| v.checked_mul(32))
					.ok_or_else(overflow)?;
				drift_work = drift_work.checked_add(work).ok_or_else(overflow)?;
			}
		}
		let row_query_work = grid
			.axes()
			.checked_mul(32)
			.and_then(|v| v.checked_add(drift_work))
			.and_then(|v| v.checked_mul(evaluations))
			.and_then(|v| v.checked_add(maximum_row_entries.checked_mul(64)?))
			.ok_or_else(overflow)?;
		if row_query_work > limits.max_query_work {
			return Err(CfdError::InvalidInput("KvN row query work budget"));
		}
		Ok(Self {
			grid,
			ode,
			resources: KvnRecipeResources {
				retained_bytes,
				row_query_bytes,
				row_query_work,
				maximum_row_entries,
				maximum_drift_evaluations_per_row: evaluations,
			},
		})
	}
	/// Conservative local query and complete retained-input costs.
	#[must_use]
	pub const fn resources(&self) -> KvnRecipeResources {
		self.resources
	}
	const fn validate(&self, time: f64, row: usize) -> Result<(), CfdError> {
		if !time.is_finite() || row >= self.grid.dimension() {
			Err(CfdError::InvalidInput("KvN row/time out of bounds"))
		} else {
			Ok(())
		}
	}
}
impl HistoryRowDynamics for KvnHistoryRecipe<'_> {
	fn dimension(&self) -> usize {
		self.grid.dimension()
	}
	fn max_row_entries(&self) -> usize {
		self.resources.maximum_row_entries
	}
	fn retained_bytes(&self) -> Result<usize, CfdError> {
		Ok(self.resources.retained_bytes)
	}
	fn row_query_bytes(&self) -> usize {
		self.resources.row_query_bytes
	}
	fn row_query_work(&self) -> usize {
		self.resources.row_query_work
	}
	fn visit_row(
		&self,
		time: f64,
		row: usize,
		visitor: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError> {
		self.validate(time, row)?;
		let point = self
			.grid
			.point(row)
			.ok_or(CfdError::Assembly("KvN row point"))?;
		let left = self.ode.drift(time, &point)?;
		let mut stride = 1;
		let n = self.grid.axis_dimension();
		for axis in 0..self.grid.axes() {
			let a = (row / stride) % n;
			let wa = self
				.grid
				.axis_weight(a)
				.ok_or(CfdError::Assembly("KvN row weight"))?;
			for &(b, derivative) in self
				.grid
				.axis_derivative_row(a)
				.ok_or(CfdError::Assembly("KvN derivative row"))?
			{
				let column = row - a * stride + b * stride;
				let point = self
					.grid
					.point(column)
					.ok_or(CfdError::Assembly("KvN neighbor point"))?;
				let right = self.ode.drift(time, &point)?;
				let wb = self
					.grid
					.axis_weight(b)
					.ok_or(CfdError::Assembly("KvN neighbor weight"))?;
				let value = -0.5 * (left[axis] + right[axis]) * derivative * (wa / wb).sqrt();
				if !value.is_finite() {
					return Err(CfdError::InvalidInput("KvN row arithmetic overflow"));
				}
				visitor(column, Complex64::new(value, 0.))?;
			}
			stride *= n;
		}
		Ok(())
	}
	fn source_entry(&self, time: f64, row: usize) -> Result<Complex64, CfdError> {
		self.validate(time, row)?;
		Ok(Complex64::new(0., 0.))
	}
}

/// Separable compact regularization, generated one mass-weighted amplitude at a time.
///
/// The result is unnormalized; collective amplitude preparation normalizes it.
/// A bounded axis scan rejects empty sampled support and complete floating-point
/// underflow before any global state allocation. The center's actual capacity is charged.
pub struct CompactBumpRecipe<'a> {
	grid: &'a ConfigurationGrid,
	center: Vec<f64>,
	width: f64,
	retained_bytes: usize,
	query_work: usize,
}
impl<'a> CompactBumpRecipe<'a> {
	/// Prepare a scalar recipe and prove that at least one sampled amplitude is nonzero.
	/// # Errors
	/// Rejects malformed data, empty/underflowed sampled support and work/storage limits.
	pub fn new(
		grid: &'a ConfigurationGrid,
		center: Vec<f64>,
		width: f64,
		limits: KvnRecipeLimits,
	) -> Result<Self, CfdError> {
		if center.len() != grid.axes()
			|| center.iter().any(|v| !v.is_finite())
			|| !width.is_finite()
			|| width <= 0.
			|| grid.dimension() > limits.max_dimension
		{
			return Err(CfdError::InvalidInput("invalid compact KvN bump"));
		}
		let retained_bytes = center
			.capacity()
			.checked_mul(size_of::<f64>())
			.and_then(|v| v.checked_add(size_of::<Self>()))
			.and_then(|v| v.checked_add(grid.retained_bytes().ok()?))
			.ok_or_else(overflow)?;
		let query_work = grid.axes().checked_mul(128).ok_or_else(overflow)?;
		let construction_work = query_work
			.checked_mul(grid.axis_dimension())
			.ok_or_else(overflow)?;
		if retained_bytes
			.checked_add(256)
			.is_none_or(|v| v > limits.max_bytes)
			|| construction_work > limits.max_query_work
		{
			return Err(CfdError::InvalidInput("compact bump source budget"));
		}
		let out = Self {
			grid,
			center,
			width,
			retained_bytes,
			query_work,
		};
		// Product structure makes the maximum log-amplitude the sum of axis maxima;
		// this scans only the one-dimensional grid, not the full tensor state.
		let mut maximum = 0.;
		for axis in 0..grid.axes() {
			let mut best = f64::NEG_INFINITY;
			for node in 0..grid.axis_dimension() {
				best = best.max(out.log_axis(axis, node)?);
			}
			maximum += best;
		}
		let maximum = maximum.exp();
		if !maximum.is_finite() || maximum == 0. {
			return Err(CfdError::InvalidInput(
				"initial regularization unresolved or underflowed",
			));
		}
		Ok(out)
	}
	fn log_axis(&self, axis: usize, node: usize) -> Result<f64, CfdError> {
		let x = self
			.grid
			.axis_node(node)
			.ok_or(CfdError::Assembly("bump axis point"))?;
		let w = self
			.grid
			.axis_weight(node)
			.ok_or(CfdError::Assembly("bump axis weight"))?;
		if !x.is_finite() || !w.is_finite() || w <= 0. {
			return Err(CfdError::InvalidInput("bump axis geometry"));
		}
		let r = (x - self.center[axis]) / self.width;
		Ok(if r.abs() >= 1. {
			f64::NEG_INFINITY
		} else {
			-1. / (1. - r * r) + 0.5 * w.ln()
		})
	}
	/// Unnormalized mass-weighted amplitude; evaluation uses logarithms to avoid
	/// premature underflow in the compact-bump/tensor-weight product.
	/// # Errors
	/// Rejects an out-of-range tensor index or arithmetic overflow.
	pub fn amplitude(&self, mut index: usize) -> Result<Complex64, CfdError> {
		if index >= self.grid.dimension() {
			return Err(CfdError::InvalidInput("bump index out of bounds"));
		}
		let mut log = 0.;
		for axis in 0..self.grid.axes() {
			log += self.log_axis(axis, index % self.grid.axis_dimension())?;
			index /= self.grid.axis_dimension();
		}
		let value = log.exp();
		if !value.is_finite() {
			return Err(CfdError::InvalidInput("bump amplitude overflow"));
		}
		Ok(Complex64::new(value, 0.))
	}
	/// Includes the borrowed axis recipe and owned center capacity.
	#[must_use]
	pub const fn retained_bytes(&self) -> usize {
		self.retained_bytes
	}
	/// Fixed scalar evaluation scratch; no heap allocation during a query.
	#[must_use]
	pub const fn query_bytes(&self) -> usize {
		256
	}
	/// Per-entry logical work allowance including transcendental calls.
	#[must_use]
	pub const fn query_work(&self) -> usize {
		self.query_work
	}
}
