//! Locally generated causal history rows, with no complete matrix or RHS allocation.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::cast_precision_loss,
	clippy::as_conversions,
	clippy::suboptimal_flops,
	reason = "Checked global dimensions and fixed DG1/DG2 arrays bound indexed arithmetic"
)]
use crate::CfdError;
use quest_numerics::{Complex64, sparse_stream::SparseEntry};
use std::ops::Range;

/// Immutable row-addressable lifted dynamics. Source queries never require a
/// complete lifted vector.
///
/// Implementations admit their own kernels before use.
/// Declarations must be stable, and include all borrowed kernel storage and
/// transient row/source work; classical lookup is not a coherent oracle.
pub trait HistoryRowDynamics {
	fn dimension(&self) -> usize;
	fn max_row_entries(&self) -> usize;
	/// # Errors
	/// Rejects storage-accounting overflow.
	fn retained_bytes(&self) -> Result<usize, CfdError>;
	fn row_query_bytes(&self) -> usize;
	/// Upper bound for either one row visit or one scalar source query.
	fn row_query_work(&self) -> usize;
	/// Emit deterministic row entries, including duplicates in canonical source order.
	/// # Errors
	/// Reports recipe or visitor failures.
	fn visit_row(
		&self,
		time: f64,
		row: usize,
		visitor: &mut dyn FnMut(usize, Complex64) -> Result<(), CfdError>,
	) -> Result<(), CfdError>;
	/// # Errors
	/// Reports source domain or indexing failures.
	fn source_entry(&self, time: f64, row: usize) -> Result<Complex64, CfdError>;
}
#[derive(Clone, Copy, Debug)]
pub struct HistoryStreamLimits {
	pub max_dimension: usize,
	pub max_bytes: usize,
	pub max_work_per_visit: usize,
}
impl Default for HistoryStreamLimits {
	fn default() -> Self {
		Self {
			max_dimension: usize::MAX,
			max_bytes: 268_435_456,
			max_work_per_visit: 100_000_000,
		}
	}
}
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct HistoryStreamResources {
	pub dimension: usize,
	pub maximum_row_entries: usize,
	/// Kernel, fixed recipe, one local row buffer and declared query scratch.
	pub peak_managed_bytes: usize,
	pub maximum_work_per_row: usize,
	/// Includes gaps reserved for absent entries; independent of partitioning.
	pub ordinal_slots: u64,
}
/// Borrowed immutable recipe for the exact same temporal DG as `HistorySystem`.
///
/// Storage does not grow with the time horizon's number of coefficients. A
/// single row iterator is admitted at a time by its caller's aggregate budget;
/// simultaneous independent iterators each own and charge a row buffer.
pub struct TemporalHistoryRecipe<'a, D: HistoryRowDynamics> {
	dynamics: &'a D,
	n: usize,
	q: usize,
	dt: f64,
	horizon: f64,
	cells: usize,
	order: usize,
	nodes: [f64; 3],
	weights: [f64; 3],
	stiffness: [[f64; 3]; 3],
	max_generator_entries: usize,
	limits: HistoryStreamLimits,
	resources: HistoryStreamResources,
}
impl<'a, D: HistoryRowDynamics> TemporalHistoryRecipe<'a, D> {
	/// Admit index space and one row's simultaneous resources before any allocation.
	/// # Errors
	/// Rejects invalid order/horizon, count or ordinal overflow and storage budgets.
	pub fn new(
		dynamics: &'a D,
		horizon: f64,
		cells: usize,
		order: usize,
		limits: HistoryStreamLimits,
	) -> Result<Self, CfdError> {
		let n = dynamics.dimension();
		if n == 0
			|| cells == 0
			|| u64::try_from(cells).map_or(true, |n| n > (1_u64 << 53))
			|| !horizon.is_finite()
			|| horizon <= 0.
		{
			return Err(CfdError::InvalidInput(
				"invalid streaming history shape/horizon",
			));
		}
		let (q, nodes, weights, derivative) = match order {
			1 => (
				2,
				[-1., 1., 0.],
				[1., 1., 0.],
				[[-0.5, 0.5, 0.], [-0.5, 0.5, 0.], [0.; 3]],
			),
			2 => (
				3,
				[-1., 0., 1.],
				[1. / 3., 4. / 3., 1. / 3.],
				[[-1.5, 2., -0.5], [-0.5, 0., 0.5], [0.5, -2., 1.5]],
			),
			_ => {
				return Err(CfdError::Unsupported(
					"streaming history requires DG1 or DG2".into(),
				));
			}
		};
		let dt = horizon / (cells as f64);
		if !dt.is_finite() || dt <= 0. || 0.5 * dt * weights[0] <= 0. {
			return Err(CfdError::InvalidInput("streaming time mass underflow"));
		}
		let dimension =
			n.checked_mul(cells)
				.and_then(|v| v.checked_mul(q))
				.ok_or(CfdError::InvalidInput(
					"streaming history dimension overflow",
				))?;
		let max_generator_entries = dynamics.max_row_entries();
		let maximum_row_entries = max_generator_entries
			.checked_add(q)
			.and_then(|v| v.checked_add(1))
			.ok_or(CfdError::InvalidInput("streaming row overflow"))?;
		let ordinal_slots = u64::try_from(dimension)
			.ok()
			.and_then(|n| n.checked_mul(u64::try_from(maximum_row_entries).ok()?))
			.ok_or(CfdError::InvalidInput("streaming history ordinal overflow"))?;
		let peak_managed_bytes = maximum_row_entries
			.checked_mul(size_of::<SparseEntry>())
			.and_then(|v| v.checked_add(size_of::<HistoryRows<'_, '_, D>>() + size_of::<Self>()))
			.and_then(|v| v.checked_add(dynamics.row_query_bytes()))
			.and_then(|v| v.checked_add(dynamics.retained_bytes().ok()?))
			.ok_or(CfdError::InvalidInput("streaming history storage overflow"))?;
		let maximum_work_per_row = maximum_row_entries
			.checked_mul(16)
			.and_then(|v| v.checked_add(dynamics.row_query_work()))
			.ok_or(CfdError::InvalidInput("streaming history work overflow"))?;
		if dimension > limits.max_dimension || peak_managed_bytes > limits.max_bytes {
			return Err(CfdError::InvalidInput(
				"streaming history exceeds admission",
			));
		}
		let mut stiffness = [[0.; 3]; 3];
		for a in 0..q {
			for b in 0..q {
				stiffness[a][b] =
					-derivative[b][a] * weights[b] + f64::from(a == q - 1 && b == q - 1);
			}
		}
		Ok(Self {
			dynamics,
			n,
			q,
			dt,
			horizon,
			cells,
			order,
			nodes,
			weights,
			stiffness,
			max_generator_entries,
			limits,
			resources: HistoryStreamResources {
				dimension,
				maximum_row_entries,
				peak_managed_bytes,
				maximum_work_per_row,
				ordinal_slots,
			},
		})
	}
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.resources.dimension
	}
	#[must_use]
	pub const fn resources(&self) -> HistoryStreamResources {
		self.resources
	}
	/// Fixed temporal/grid/ordinal semantics for collective source agreement.
	/// The version word binds the DG sign, quadrature, column ordering and ordinal
	/// convention. These words do not certify caller generator coefficients.
	/// # Errors
	/// Rejects unrepresentable scalar metadata; no source rows or vectors are queried.
	pub fn semantic_words(&self) -> Result<[u64; 11], CfdError> {
		let word =
			|n| u64::try_from(n).map_err(|_| CfdError::InvalidInput("history semantic width"));
		Ok([
			0x4447_4849_5354_3031,
			self.horizon.to_bits(),
			self.dt.to_bits(),
			word(self.cells)?,
			word(self.order)?,
			word(self.n)?,
			word(self.q)?,
			word(self.max_generator_entries)?,
			word(self.resources.maximum_row_entries)?,
			word(self.dimension())?,
			self.resources.ordinal_slots,
		])
	}
	fn node(&self, row: usize) -> Result<(usize, usize, usize, f64, f64), CfdError> {
		if row >= self.dimension() {
			return Err(CfdError::InvalidInput("streaming history row"));
		}
		let block = row / self.n;
		let cell = block / self.q;
		let a = block % self.q;
		let time = self.dt * ((cell as f64) + f64::midpoint(self.nodes[a], 1.));
		let mass = 0.5 * self.dt * self.weights[a];
		if !time.is_finite() {
			return Err(CfdError::InvalidInput("streaming history time overflow"));
		}
		Ok((cell, a, block, time, mass))
	}
	/// Produce only the caller-owned contiguous rows, preserving global input ordinals.
	/// A failed recipe terminates the iterator after returning one error. The caller
	/// must collectively agree errors before using any partially produced resource.
	/// # Errors
	/// Rejects invalid ranges, row-query work budgets and allocation failure.
	pub fn rows(&self, range: Range<usize>) -> Result<HistoryRows<'_, 'a, D>, CfdError> {
		if range.start > range.end
			|| range.end > self.dimension()
			|| range
				.len()
				.checked_mul(self.resources.maximum_work_per_row)
				.is_none_or(|v| v > self.limits.max_work_per_visit)
		{
			return Err(CfdError::InvalidInput("streaming row range/work budget"));
		}
		let mut buffer = Vec::new();
		buffer
			.try_reserve_exact(self.resources.maximum_row_entries)
			.map_err(|_| CfdError::InvalidInput("streaming row allocation"))?;
		Ok(HistoryRows {
			recipe: self,
			range,
			buffer,
			offset: 0,
			failed: false,
		})
	}
	/// Evaluate a scalar RHS coefficient from an independently prepared initial-state recipe.
	/// Initial callback costs belong to the caller. Source query storage/work is charged here.
	/// # Errors
	/// Rejects row/work admission, source errors, nonfinite initial or resulting values.
	pub fn rhs_value(
		&self,
		row: usize,
		mut initial: impl FnMut(usize) -> Result<Complex64, CfdError>,
	) -> Result<Complex64, CfdError> {
		if self.resources.maximum_work_per_row > self.limits.max_work_per_visit {
			return Err(CfdError::InvalidInput("streaming RHS work budget"));
		}
		let (_, _, block, time, mass) = self.node(row)?;
		let source = self.dynamics.source_entry(time, row % self.n)?;
		let mut value = mass * source;
		if block == 0 {
			value += initial(row % self.n)?;
		}
		if !source.re.is_finite()
			|| !source.im.is_finite()
			|| !value.re.is_finite()
			|| !value.im.is_finite()
		{
			return Err(CfdError::InvalidInput("nonfinite streaming RHS"));
		}
		Ok(value)
	}
}
/// Bounded iterator storage is one row, regardless of total or local matrix size.
pub struct HistoryRows<'r, 'a, D: HistoryRowDynamics> {
	recipe: &'r TemporalHistoryRecipe<'a, D>,
	range: Range<usize>,
	buffer: Vec<SparseEntry>,
	offset: usize,
	failed: bool,
}
impl<D: HistoryRowDynamics> HistoryRows<'_, '_, D> {
	/// Actual owned row buffer capacity plus iterator metadata, without borrowed kernels.
	/// # Errors
	/// Rejects capacity accounting overflow.
	pub fn retained_bytes(&self) -> Result<usize, CfdError> {
		self.buffer
			.capacity()
			.checked_mul(size_of::<SparseEntry>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(CfdError::InvalidInput(
				"streaming actual row capacity overflow",
			))
	}

	fn fill(&mut self, row: usize) -> Result<(), CfdError> {
		self.buffer.clear();
		self.offset = 0;
		let recipe = self.recipe;
		let (cell, a, block, time, mass) = recipe.node(row)?;
		let ordinal = u64::try_from(row)
			.ok()
			.and_then(|r| r.checked_mul(u64::try_from(recipe.resources.maximum_row_entries).ok()?))
			.ok_or(CfdError::InvalidInput("history ordinal overflow"))?;
		for b in 0..recipe.q {
			let k = recipe.stiffness[a][b];
			if k != 0. {
				self.buffer.push(SparseEntry {
					row,
					column: (cell * recipe.q + b) * recipe.n + row % recipe.n,
					ordinal: ordinal
						+ u64::try_from(b)
							.map_err(|_| CfdError::InvalidInput("temporal ordinal"))?,
					value: Complex64::new(k, 0.),
				});
			}
		}
		if cell > 0 && a == 0 {
			self.buffer.push(SparseEntry {
				row,
				column: ((cell - 1) * recipe.q + recipe.q - 1) * recipe.n + row % recipe.n,
				ordinal: ordinal
					+ u64::try_from(recipe.q)
						.map_err(|_| CfdError::InvalidInput("temporal ordinal"))?,
				value: Complex64::new(-1., 0.),
			});
		}
		let mut count = 0usize;
		recipe
			.dynamics
			.visit_row(time, row % recipe.n, &mut |column, value| {
				if column >= recipe.n
					|| count >= recipe.max_generator_entries
					|| !value.re.is_finite()
					|| !value.im.is_finite()
				{
					return Err(CfdError::InvalidInput("malformed history row recipe"));
				}
				let scaled = -mass * value;
				if !scaled.re.is_finite() || !scaled.im.is_finite() {
					return Err(CfdError::InvalidInput("history row scaling overflow"));
				}
				self.buffer.push(SparseEntry {
					row,
					column: block * recipe.n + column,
					ordinal: ordinal
						+ u64::try_from(recipe.q + 1 + count)
							.map_err(|_| CfdError::InvalidInput("temporal ordinal"))?,
					value: scaled,
				});
				count += 1;
				Ok(())
			})
	}
}
impl<D: HistoryRowDynamics> Iterator for HistoryRows<'_, '_, D> {
	type Item = Result<SparseEntry, CfdError>;
	fn next(&mut self) -> Option<Self::Item> {
		if self.failed {
			return None;
		}
		loop {
			if let Some(entry) = self.buffer.get(self.offset).copied() {
				self.offset += 1;
				return Some(Ok(entry));
			}
			let row = self.range.next()?;
			if let Err(error) = self.fill(row) {
				self.failed = true;
				return Some(Err(error));
			}
		}
	}
}
