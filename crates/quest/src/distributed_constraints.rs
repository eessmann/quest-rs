//! Complete constraint coordinates from cell-local mass whitening and distributed CPQR.
//!
//! Dense fill is retained only on physical-row owners. Scalar communication is a
//! charged baseline. Numerical rank and multiplier gauge are explicit, not exact
//! rank proofs or physical pressure certificates. Pivots below the zero threshold
//! are discarded: queries use this numerical truncated factorization, including
//! the reported largest discarded column residual norm. All queries are
//! collective and must occur in the same order on every communicator rank.
#![allow(
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	reason = "Matrix and partition extents are checked at admission; numerical binary64 arithmetic is checked for finiteness at phase boundaries"
)]
use crate::{
	Error, MemoryBudget, Result,
	collective::{CollectiveEnvironment, equal},
	environment::{Reservation, RuntimeResources},
	values::reserve_vec,
};
use quest_numerics::constraint_chart::{CellInput, CellWhitening};
use quest_sys::mpi::MpiCollectiveLane;
use std::ops::Range;
mod wire;
use wire::{agree, fatal, reduce, scalar};
/// Explicit binary64 pivot-norm decision band, in whitened absolute units.
#[derive(Clone, Copy, Debug)]
pub struct RankPolicy {
	pub zero_at_most: f64,
	pub nonzero_at_least: f64,
	pub compatibility_tolerance: f64,
}
impl Default for RankPolicy {
	fn default() -> Self {
		Self {
			zero_at_most: 1e-12,
			nonzero_at_least: 1e-10,
			compatibility_tolerance: 1e-10,
		}
	}
}
/// Logical resource bounds, excluding MPI implementation-internal allocations.
#[derive(Clone, Copy, Debug)]
pub struct ConstraintLimits {
	pub max_rows: usize,
	pub max_constraints: usize,
	pub max_construct_work: usize,
	pub max_query_work: usize,
	pub max_transport_bytes: usize,
	pub max_local_bytes: usize,
	pub ranks_per_node: usize,
	pub node_budget: MemoryBudget,
}
impl Default for ConstraintLimits {
	fn default() -> Self {
		Self {
			max_rows: 1_048_576,
			max_constraints: 1_048_576,
			max_construct_work: 268_435_456,
			max_query_work: 268_435_456,
			max_transport_bytes: usize::MAX,
			max_local_bytes: 268_435_456,
			ranks_per_node: usize::MAX,
			node_budget: MemoryBudget::new(usize::MAX),
		}
	}
}
/// Storage capacities and conservative work/global directed payload ceilings.
#[derive(Clone, Copy, Debug)]
pub struct ConstraintResources {
	pub local_rows: usize,
	pub local_matrix_entries: usize,
	pub local_reflector_entries: usize,
	pub retained_bytes: usize,
	pub construction_peak_bytes: usize,
	pub query_peak_bytes: usize,
	pub construct_work: usize,
	pub query_work: usize,
	pub construction_transport_bytes: usize,
	pub query_transport_bytes: usize,
	pub max_packet_bytes: usize,
}
/// Disjoint complete uniform cell ranges. Mixed dimensions are explicitly rejected.
pub struct ConstraintShard {
	total_cells: usize,
	dimension: usize,
	columns: usize,
	rank: usize,
	parts: usize,
	cells: Vec<CellInput>,
}
impl ConstraintShard {
	/// # Errors
	/// Rejects incomplete/duplicate ownership, mixed cell dimensions, shape or nonfinite blocks.
	pub fn from_parts(
		total_cells: usize,
		total_rows: usize,
		constraint_count: usize,
		rank: usize,
		parts: usize,
		cells: Vec<CellInput>,
	) -> Result<Self> {
		if total_cells == 0
			|| total_rows == 0
			|| !total_rows.is_multiple_of(total_cells)
			|| !parts.is_power_of_two()
			|| rank >= parts
		{
			return Err(Error::Value("constraint shard dimensions/ownership"));
		}
		let dimension = total_rows / total_cells;
		let range = partition(total_cells, rank, parts);
		if range.len() != cells.len() {
			return Err(Error::Value("constraint shard cell coverage"));
		}
		for (cell, id) in cells.iter().zip(range) {
			if cell.cell_id != id
				|| cell.dimension != dimension
				|| dimension.checked_mul(dimension) != Some(cell.mass.len())
				|| dimension.checked_mul(constraint_count) != Some(cell.constraints_transpose.len())
				|| cell
					.mass
					.iter()
					.chain(&cell.constraints_transpose)
					.any(|x| !x.is_finite())
			{
				return Err(Error::Value("constraint shard cell block"));
			}
		}
		Ok(Self {
			total_cells,
			dimension,
			columns: constraint_count,
			rank,
			parts,
			cells,
		})
	}
}
/// Numerical pivot evidence under an explicit absolute band; not an exact rank proof.
#[derive(Clone, Copy, Debug)]
pub struct RankEvidence {
	pub numerical_rank: usize,
	pub minimum_accepted_norm: Option<f64>,
	pub maximum_discarded_norm: f64,
	pub policy: RankPolicy,
}
/// Ambiguous numerical pivot; no reusable operator is published.
#[derive(Clone, Debug)]
pub struct RankAmbiguity {
	pub candidate_rank: usize,
	pub original_column: usize,
	pub observed_norm: f64,
	pub zero_at_most: f64,
	pub nonzero_at_least: f64,
}
#[allow(
	clippy::large_enum_variant,
	reason = "Fallibly admitted chart owner returns without an extra infallible allocation"
)]
pub enum ChartOutcome<'env, 'comm, 'runtime> {
	Prepared(ConstraintChart<'env, 'comm, 'runtime>),
	Ambiguous(RankAmbiguity),
}
/// Local output storage stays charged to the originating environment until drop.
/// Conservatively retains the complete admitted query scratch allowance, even
/// after transient query buffers have been freed.
pub struct ChartVector<'env> {
	values: Vec<f64>,
	range: Range<usize>,
	_reservation: Reservation<'env>,
}
impl ChartVector<'_> {
	#[must_use]
	pub fn as_slice(&self) -> &[f64] {
		&self.values
	}
	#[must_use]
	pub fn global_range(&self) -> Range<usize> {
		self.range.clone()
	}
}
/// Generic algebraic gauge; CFD pressure normalization is a separate operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiplierGauge {
	PivotedDependentZero,
}
pub struct MultiplierSolution<'env> {
	pub values: ChartVector<'env>,
	pub gauge: MultiplierGauge,
}
pub struct ConstraintLift<'env> {
	pub velocity: ChartVector<'env>,
	pub compatibility_residual: f64,
}
/// Immutable factors, distributed R rows and local Householder slices, tied to one environment.
pub struct ConstraintChart<'env, 'comm, 'runtime> {
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	id: u64,
	rank: usize,
	parts: usize,
	total_cells: usize,
	dimension: usize,
	rows: usize,
	columns: usize,
	row_range: Range<usize>,
	numerical_rank: usize,
	minimum_accepted_norm: Option<f64>,
	maximum_discarded_norm: f64,
	pivots: Vec<usize>,
	whitening: Vec<CellWhitening>,
	matrix: Vec<f64>,
	reflectors: Vec<f64>,
	resources: ConstraintResources,
	limits: ConstraintLimits,
	policy: RankPolicy,
	_reservation: Reservation<'env>,
}
fn partition(n: usize, rank: usize, parts: usize) -> Range<usize> {
	let base = n / parts;
	let rem = n % parts;
	let start = rank * base + rank.min(rem);
	start..start + base + usize::from(rank < rem)
}
const fn owner(n: usize, index: usize, parts: usize) -> usize {
	let base = n / parts;
	let rem = n % parts;
	let large = (base + 1) * rem;
	if index < large {
		index / (base + 1)
	} else {
		rem + (index - large) / base
	}
}
fn mul(a: usize, b: usize) -> Result<usize> {
	a.checked_mul(b).ok_or(Error::Overflow)
}
fn add(a: usize, b: usize) -> Result<usize> {
	a.checked_add(b).ok_or(Error::Overflow)
}
fn zeros(n: usize) -> Result<Vec<f64>> {
	let mut v = reserve_vec(n)?;
	v.resize(n, 0.);
	Ok(v)
}
fn numerical<T>(r: quest_numerics::Result<T>) -> Result<T> {
	r.map_err(|_| Error::Value("cell mass whitening rejected"))
}
fn finite(v: &[f64]) -> Result<()> {
	if v.iter().all(|x| x.is_finite()) {
		Ok(())
	} else {
		Err(Error::Value("nonfinite constraint arithmetic"))
	}
}
fn metadata(
	lane: &mut MpiCollectiveLane<'_>,
	cells: usize,
	d: usize,
	m: usize,
	limits: ConstraintLimits,
	policy: RankPolicy,
) -> Result<()> {
	for value in [
		cells,
		d,
		m,
		limits.max_rows,
		limits.max_constraints,
		limits.max_construct_work,
		limits.max_query_work,
		limits.max_transport_bytes,
		limits.max_local_bytes,
		limits.ranks_per_node,
		limits.node_budget.bytes(),
	] {
		equal(
			lane,
			&u64::try_from(value)
				.map_err(|_| Error::Overflow)?
				.to_le_bytes(),
		)?;
	}
	for value in [
		policy.zero_at_most,
		policy.nonzero_at_least,
		policy.compatibility_tolerance,
	] {
		equal(lane, &value.to_bits().to_le_bytes())?;
	}
	Ok(())
}
#[allow(
	clippy::too_many_arguments,
	reason = "Complete dimensions, ownership, actual source capacity and limits enter one checked planner"
)]
fn plan(
	cells: usize,
	d: usize,
	m: usize,
	rank: usize,
	parts: usize,
	input_bytes: usize,
	limits: ConstraintLimits,
	policy: RankPolicy,
) -> Result<ConstraintResources> {
	let n = mul(cells, d)?;
	let local = mul(partition(cells, rank, parts).len(), d)?;
	let t = n.min(m);
	if cells == 0
		|| d == 0
		|| !parts.is_power_of_two()
		|| rank >= parts
		|| n > limits.max_rows
		|| m > limits.max_constraints
		|| limits.ranks_per_node == 0
		|| limits.ranks_per_node > parts
		|| !policy.zero_at_most.is_finite()
		|| !policy.nonzero_at_least.is_finite()
		|| policy.zero_at_most < 0.
		|| policy.nonzero_at_least <= policy.zero_at_most
		|| !policy.compatibility_tolerance.is_finite()
		|| policy.compatibility_tolerance < 0.
	{
		return Err(Error::Value("constraint dimensions/limits/rank policy"));
	}
	let entries = mul(local, m)?;
	let reflectors = mul(local, t)?;
	let mass = mul(mul(local, d)?, 8)?;
	let headers = mul(
		partition(cells, rank, parts).len(),
		size_of::<CellWhitening>(),
	)?;
	let retained = add(
		add(add(mul(add(entries, reflectors)?, 8)?, mass)?, headers)?,
		mul(m, size_of::<usize>())?,
	)?;
	let query_peak = add(
		mul(
			add(mul(local, 3)?, mul(partition(m, rank, parts).len(), 3)?)?,
			8,
		)?,
		8192,
	)?;
	let numerical_construct_work = add(
		mul(mul(mul(n, m)?, t.max(1))?, 16)?,
		mul(mul(mul(n, d)?, d)?, 16)?,
	)?;
	let numerical_query_work = mul(
		add(mul(n, t.max(1))?, add(mul(m, t.max(1))?, mul(n, d)?)?)?,
		32,
	)?;
	// Include scalar protocol processing even on empty physical-row owners.
	let scalar_steps = add(mul(m, t.max(1))?, mul(t.max(1), t.max(1))?)?;
	let protocol_work = add(
		mul(parts, 1024)?,
		mul(
			mul(scalar_steps, 64)?,
			add(
				usize::try_from(parts.ilog2()).map_err(|_| Error::Overflow)?,
				1,
			)?,
		)?,
	)?;
	let construct_work = add(numerical_construct_work, protocol_work)?;
	let query_work = add(numerical_query_work, protocol_work)?;
	let coordination = mul(mul(parts, parts)?, 8192)?;
	let ct = add(
		mul(
			mul(
				mul(add(mul(m, t.max(1))?, mul(t.max(1), t.max(1))?)?, parts)?,
				usize::try_from(parts.ilog2()).map_err(|_| Error::Overflow)? + 1,
			)?,
			32,
		)?,
		coordination,
	)?;
	let qt = add(
		mul(
			mul(add(mul(m, t.max(1))?, mul(t.max(1), t.max(1))?)?, parts)?,
			64,
		)?,
		coordination,
	)?;
	let peak = add(add(retained, input_bytes)?, add(mul(d, 8)?, 8192)?)?;
	if construct_work > limits.max_construct_work
		|| query_work > limits.max_query_work
		|| add(ct, qt)? > limits.max_transport_bytes
	{
		return Err(Error::Value("constraint work/transport budget"));
	}
	Ok(ConstraintResources {
		local_rows: local,
		local_matrix_entries: entries,
		local_reflector_entries: reflectors,
		retained_bytes: retained,
		construction_peak_bytes: peak,
		query_peak_bytes: query_peak,
		construct_work,
		query_work,
		construction_transport_bytes: ct,
		query_transport_bytes: qt,
		max_packet_bytes: 32,
	})
}
fn capacity(
	lane: &mut MpiCollectiveLane<'_>,
	resources: &RuntimeResources,
	extra: usize,
	limits: ConstraintLimits,
	parts: usize,
) -> Result<()> {
	let peak = agree(lane, add(resources.allocated_bytes(), extra))?;
	let mut maximum = 0;
	for peer in 0..parts {
		let mut packet = u64::try_from(peak)
			.map_err(|_| Error::Overflow)?
			.to_le_bytes();
		lane.broadcast_bytes(
			i32::try_from(peer).map_err(|_| Error::Overflow)?,
			&mut packet,
		)
		.map_err(|_| Error::Value("chart capacity transport"))?;
		maximum =
			maximum.max(usize::try_from(u64::from_le_bytes(packet)).map_err(|_| Error::Overflow)?);
	}
	agree(
		lane,
		if maximum > limits.max_local_bytes
			|| mul(maximum, limits.ranks_per_node).map_or(true, |v| v > limits.node_budget.bytes())
		{
			Err(Error::Value("constraint live rank/node capacity"))
		} else {
			Ok(())
		},
	)
}
impl<'comm, 'runtime> CollectiveEnvironment<'comm, 'runtime> {
	/// Generate only this rank's complete cells, under collective admission before allocation.
	/// Callbacks are scalar and must not perform mismatched MPI collectives.
	/// Admission charges their invocation counts; callback-internal work and
	/// allocations are caller-owned.
	/// # Errors
	/// Agrees dimensions, capacities and any local allocation/generator failure.
	#[allow(
		clippy::too_many_arguments,
		reason = "Explicit source dimensions, rank policy and two independent scalar block generators"
	)]
	pub fn prepare_constraint_chart_from_fn(
		&self,
		total_cells: usize,
		dimension: usize,
		constraint_count: usize,
		mut limits: ConstraintLimits,
		policy: RankPolicy,
		mut mass_value: impl FnMut(usize, usize, usize) -> Result<f64>,
		mut constraint_value: impl FnMut(usize, usize, usize) -> Result<f64>,
	) -> Result<ChartOutcome<'_, 'comm, 'runtime>> {
		let id = self.identifier();
		let mut lane = self.begin(90, id, 0, 0)?;
		let rank = usize::try_from(self.rank()?).map_err(|_| Error::Overflow)?;
		let parts = usize::try_from(self.size()?).map_err(|_| Error::Overflow)?;
		if limits.ranks_per_node == usize::MAX {
			limits.ranks_per_node = parts;
		}
		metadata(
			&mut lane,
			total_cells,
			dimension,
			constraint_count,
			limits,
			policy,
		)?;
		let input_bytes = agree(
			&mut lane,
			(|| {
				let local_cells = partition(total_cells, rank, parts).len();
				add(
					mul(
						mul(
							local_cells,
							add(
								mul(dimension, dimension)?,
								mul(dimension, constraint_count)?,
							)?,
						)?,
						8,
					)?,
					mul(local_cells, size_of::<CellInput>())?,
				)
			})(),
		)?;
		let resources = agree(
			&mut lane,
			plan(
				total_cells,
				dimension,
				constraint_count,
				rank,
				parts,
				input_bytes,
				limits,
				policy,
			),
		)?;
		capacity(
			&mut lane,
			&self.resources,
			resources.construction_peak_bytes,
			limits,
			parts,
		)?;
		let input_reservation = agree(&mut lane, self.resources.reserve(input_bytes))?;
		let generated = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let range = partition(total_cells, rank, parts);
			let mut cells = reserve_vec(range.len())?;
			for cell_id in range {
				let mut mass = reserve_vec(mul(dimension, dimension)?)?;
				let mut ct = reserve_vec(mul(dimension, constraint_count)?)?;
				for i in 0..dimension {
					for j in 0..dimension {
						mass.push(mass_value(cell_id, i, j)?);
					}
					for j in 0..constraint_count {
						ct.push(constraint_value(cell_id, i, j)?);
					}
				}
				cells.push(CellInput {
					cell_id,
					dimension,
					mass,
					constraints_transpose: ct,
				});
			}
			ConstraintShard::from_parts(
				total_cells,
				mul(total_cells, dimension)?,
				constraint_count,
				rank,
				parts,
				cells,
			)
		}))
		.unwrap_or(Err(Error::Value("constraint source generator panicked")));
		let shard = agree(&mut lane, generated)?;
		drop(lane);
		drop(input_reservation);
		self.prepare_constraint_chart(shard, limits, policy)
	}
	/// Import already-local blocks; source creation failures must be agreed by the caller.
	/// # Errors
	/// Agrees ownership, actual capacities, mass and arithmetic admission. Transport failure is fatal.
	#[allow(
		clippy::too_many_lines,
		reason = "Collective phase agreements stay beside their allocations and numerical preflights"
	)]
	pub fn prepare_constraint_chart(
		&self,
		shard: ConstraintShard,
		mut limits: ConstraintLimits,
		policy: RankPolicy,
	) -> Result<ChartOutcome<'_, 'comm, 'runtime>> {
		let id = self.identifier();
		let mut lane = self.begin(91, id, 0, 0)?;
		let rank = usize::try_from(self.rank()?).map_err(|_| Error::Overflow)?;
		let parts = usize::try_from(self.size()?).map_err(|_| Error::Overflow)?;
		agree(
			&mut lane,
			if shard.rank == rank && shard.parts == parts {
				Ok(())
			} else {
				Err(Error::Value("constraint communicator ownership"))
			},
		)?;
		if limits.ranks_per_node == usize::MAX {
			limits.ranks_per_node = parts;
		}
		metadata(
			&mut lane,
			shard.total_cells,
			shard.dimension,
			shard.columns,
			limits,
			policy,
		)?;
		let input_bytes = agree(
			&mut lane,
			(|| {
				let mut bytes = mul(shard.cells.capacity(), size_of::<CellInput>())?;
				for cell in &shard.cells {
					bytes = add(
						bytes,
						mul(
							add(cell.mass.capacity(), cell.constraints_transpose.capacity())?,
							8,
						)?,
					)?;
				}
				Ok(bytes)
			})(),
		)?;
		let resources = agree(
			&mut lane,
			plan(
				shard.total_cells,
				shard.dimension,
				shard.columns,
				rank,
				parts,
				input_bytes,
				limits,
				policy,
			),
		)?;
		capacity(
			&mut lane,
			&self.resources,
			resources.construction_peak_bytes,
			limits,
			parts,
		)?;
		let reservation = agree(&mut lane, self.resources.reserve(resources.retained_bytes))?;
		let temporary = agree(
			&mut lane,
			self.resources
				.reserve(resources.construction_peak_bytes - resources.retained_bytes),
		)?;
		let rows = mul(shard.total_cells, shard.dimension)?;
		let cell_range = partition(shard.total_cells, rank, parts);
		let row_range =
			mul(cell_range.start, shard.dimension)?..mul(cell_range.end, shard.dimension)?;
		let (whitening, matrix, reflectors, pivots) = agree(
			&mut lane,
			(|| {
				let mut whitening = reserve_vec(shard.cells.len())?;
				let mut matrix = zeros(resources.local_matrix_entries)?;
				let reflectors = zeros(resources.local_reflector_entries)?;
				let mut pivots = reserve_vec(shard.columns)?;
				pivots.extend(0..shard.columns);
				let mut scratch = zeros(shard.dimension)?;
				for (local_cell, cell) in shard.cells.into_iter().enumerate() {
					let mut mass = reserve_vec(cell.mass.len())?;
					mass.extend_from_slice(&cell.mass);
					let w = numerical(CellWhitening::new(
						cell.dimension,
						mass,
						limits.max_local_bytes,
					))?;
					for j in 0..shard.columns {
						for i in 0..shard.dimension {
							scratch[i] = cell.constraints_transpose[i * shard.columns + j];
						}
						numerical(w.force_to_coordinates(&mut scratch))?;
						for i in 0..shard.dimension {
							matrix[(local_cell * shard.dimension + i) * shard.columns + j] =
								scratch[i];
						}
					}
					whitening.push(w);
				}
				Ok((whitening, matrix, reflectors, pivots))
			})(),
		)?;
		let mut chart = ConstraintChart {
			environment: self,
			id,
			rank,
			parts,
			total_cells: shard.total_cells,
			dimension: shard.dimension,
			rows,
			columns: shard.columns,
			row_range,
			numerical_rank: 0,
			minimum_accepted_norm: None,
			maximum_discarded_norm: 0.,
			pivots,
			whitening,
			matrix,
			reflectors,
			resources,
			limits,
			policy,
			_reservation: reservation,
		};
		let outcome = chart.factor(&mut lane)?;
		drop(temporary);
		Ok(outcome.map_or_else(|| ChartOutcome::Prepared(chart), ChartOutcome::Ambiguous))
	}
}
impl<'env> ConstraintChart<'env, '_, '_> {
	#[must_use]
	pub const fn numerical_rank(&self) -> usize {
		self.numerical_rank
	}
	#[must_use]
	pub const fn nullity(&self) -> usize {
		self.rows - self.numerical_rank
	}
	#[must_use]
	pub const fn numerical_evidence(&self) -> RankEvidence {
		RankEvidence {
			numerical_rank: self.numerical_rank,
			minimum_accepted_norm: self.minimum_accepted_norm,
			maximum_discarded_norm: self.maximum_discarded_norm,
			policy: self.policy,
		}
	}
	#[must_use]
	pub fn pivots(&self) -> &[usize] {
		&self.pivots
	}
	#[must_use]
	pub const fn resources(&self) -> ConstraintResources {
		self.resources
	}
	#[must_use]
	pub fn local_row_range(&self) -> Range<usize> {
		self.row_range.clone()
	}
	#[must_use]
	pub fn local_constraint_range(&self) -> Range<usize> {
		partition(self.columns, self.rank, self.parts)
	}
	#[must_use]
	pub fn local_null_range(&self) -> Range<usize> {
		self.row_range.start.max(self.numerical_rank) - self.numerical_rank
			..self.row_range.end.max(self.numerical_rank) - self.numerical_rank
	}
	const fn row_owner(&self, index: usize) -> usize {
		owner(self.total_cells, index / self.dimension, self.parts)
	}
	fn get_row(&self, lane: &mut MpiCollectiveLane<'_>, values: &[f64], global: usize) -> f64 {
		let value = if self.row_range.contains(&global) {
			values[global - self.row_range.start]
		} else {
			0.
		};
		fatal(|| scalar(lane, self.rank, self.parts, self.row_owner(global), value))
	}
	fn get_r(&self, lane: &mut MpiCollectiveLane<'_>, row: usize, col: usize) -> f64 {
		let value = if self.row_range.contains(&row) {
			self.matrix[(row - self.row_range.start) * self.columns + col]
		} else {
			0.
		};
		fatal(|| scalar(lane, self.rank, self.parts, self.row_owner(row), value))
	}
	fn get_column(&self, lane: &mut MpiCollectiveLane<'_>, values: &[f64], col: usize) -> f64 {
		let range = self.local_constraint_range();
		let value = if range.contains(&col) {
			values[col - range.start]
		} else {
			0.
		};
		fatal(|| {
			scalar(
				lane,
				self.rank,
				self.parts,
				owner(self.columns, col, self.parts),
				value,
			)
		})
	}
	fn sum(&self, lane: &mut MpiCollectiveLane<'_>, value: f64, norm: bool) -> Result<f64> {
		let value = fatal(|| reduce(lane, self.rank, self.parts, value, norm));
		agree(
			lane,
			if value.is_finite() {
				Ok(value)
			} else {
				Err(Error::Value("nonfinite distributed chart reduction"))
			},
		)
	}
	#[allow(
		clippy::too_many_lines,
		clippy::float_cmp,
		reason = "Explicit distributed phases and exact residual-norm ties use original column IDs"
	)]
	fn factor(&mut self, lane: &mut MpiCollectiveLane<'_>) -> Result<Option<RankAmbiguity>> {
		let local = self.row_range.len();
		let t = self.rows.min(self.columns);
		for k in 0..t {
			let mut best = k;
			let mut best_norm = -1.;
			for j in k..self.columns {
				let mut norm = 0_f64;
				for i in 0..local {
					if self.row_range.start + i >= k {
						norm = norm.hypot(self.matrix[i * self.columns + j]);
					}
				}
				let norm = self.sum(lane, norm, true)?;
				if norm > best_norm || (norm == best_norm && self.pivots[j] < self.pivots[best]) {
					best = j;
					best_norm = norm;
				}
			}
			if best_norm <= self.policy.zero_at_most {
				self.maximum_discarded_norm = best_norm;
				break;
			}
			if best_norm < self.policy.nonzero_at_least {
				return Ok(Some(RankAmbiguity {
					candidate_rank: k,
					original_column: self.pivots[best],
					observed_norm: best_norm,
					zero_at_most: self.policy.zero_at_most,
					nonzero_at_least: self.policy.nonzero_at_least,
				}));
			}
			for i in 0..local {
				self.matrix
					.swap(i * self.columns + k, i * self.columns + best);
			}
			self.pivots.swap(k, best);
			let head = if self.row_range.contains(&k) {
				self.matrix[(k - self.row_range.start) * self.columns + k]
			} else {
				0.
			};
			let head = fatal(|| scalar(lane, self.rank, self.parts, self.row_owner(k), head));
			let sign = if head >= 0. { 1. } else { -1. };
			let denom = (2. * (1. + (head / best_norm).abs())).sqrt();
			for i in 0..local {
				let global = self.row_range.start + i;
				self.reflectors[k * local + i] = if global >= k {
					(self.matrix[i * self.columns + k] / best_norm
						+ if global == k { sign } else { 0. })
						/ denom
				} else {
					0.
				};
			}
			for j in k..self.columns {
				let mut dot = 0.;
				for i in 0..local {
					dot = self.reflectors[k * local + i]
						.mul_add(self.matrix[i * self.columns + j], dot);
				}
				let dot = self.sum(lane, dot, false)?;
				let mut column_finite = true;
				for i in 0..local {
					let value = (-2. * self.reflectors[k * local + i])
						.mul_add(dot, self.matrix[i * self.columns + j]);
					self.matrix[i * self.columns + j] = value;
					column_finite &= value.is_finite();
				}
				agree(
					lane,
					if column_finite {
						Ok(())
					} else {
						Err(Error::Value("nonfinite QR column update"))
					},
				)?;
			}
			for i in 0..local {
				let global = self.row_range.start + i;
				if global == k {
					self.matrix[i * self.columns + k] = -sign * best_norm;
				} else if global > k {
					self.matrix[i * self.columns + k] = 0.;
				}
			}
			self.numerical_rank = k + 1;
			self.minimum_accepted_norm = Some(
				self.minimum_accepted_norm
					.map_or(best_norm, |old| old.min(best_norm)),
			);
		}
		Ok(None)
	}
	fn admit_query(
		&self,
		lane: &mut MpiCollectiveLane<'_>,
		input: &[f64],
		expected: usize,
	) -> Result<Reservation<'env>> {
		agree(
			lane,
			if input.len() == expected {
				finite(input)
			} else {
				Err(Error::Value("constraint query local shape"))
			},
		)?;
		capacity(
			lane,
			&self.environment.resources,
			self.resources.query_peak_bytes,
			self.limits,
			self.parts,
		)?;
		agree(
			lane,
			self.environment
				.resources
				.reserve(self.resources.query_peak_bytes),
		)
	}
	fn apply_q(
		&self,
		lane: &mut MpiCollectiveLane<'_>,
		v: &mut [f64],
		transpose: bool,
	) -> Result<()> {
		let local = self.row_range.len();
		for step in 0..self.numerical_rank {
			let k = if transpose {
				step
			} else {
				self.numerical_rank - 1 - step
			};
			let mut dot = 0.;
			for (i, &x) in v.iter().enumerate() {
				dot = self.reflectors[k * local + i].mul_add(x, dot);
			}
			let dot = self.sum(lane, dot, false)?;
			for (i, x) in v.iter_mut().enumerate() {
				*x = (-2. * self.reflectors[k * local + i]).mul_add(dot, *x);
			}
			agree(lane, finite(v))?;
		}
		Ok(())
	}
	fn whiten(&self, lane: &mut MpiCollectiveLane<'_>, v: &mut [f64], operation: u8) -> Result<()> {
		agree(
			lane,
			(|| {
				for (cell, w) in v.chunks_exact_mut(self.dimension).zip(&self.whitening) {
					numerical(match operation {
						0 => w.velocity_to_coordinates(cell),
						1 => w.coordinates_to_velocity(cell),
						2 => w.force_to_coordinates(cell),
						_ => w.coordinates_to_force(cell),
					})?;
				}
				Ok(())
			})(),
		)
	}
	/// u=L^-T Q [0; a], retaining every null coordinate.
	/// # Errors
	/// Rejects local input shape/nonfinite values or live storage admission before publishing output.
	pub fn lift_null(&self, local_null: &[f64]) -> Result<ChartVector<'env>> {
		let mut lane = self.environment.begin(92, self.id, 0, 0)?;
		let reservation = self.admit_query(&mut lane, local_null, self.local_null_range().len())?;
		let mut v = agree(&mut lane, zeros(self.row_range.len()))?;
		for (global, x) in self.row_range.clone().zip(&mut v) {
			if global >= self.numerical_rank {
				*x = local_null[global - self.row_range.start.max(self.numerical_rank)];
			}
		}
		self.apply_q(&mut lane, &mut v, false)?;
		self.whiten(&mut lane, &mut v, 1)?;
		Ok(ChartVector {
			values: v,
			range: self.local_row_range(),
			_reservation: reservation,
		})
	}
	/// a=(Q^T L^T u)_tail.
	/// # Errors
	/// Rejects shape, arithmetic or live storage admission.
	pub fn lower_velocity(&self, local_velocity: &[f64]) -> Result<ChartVector<'env>> {
		let mut lane = self.environment.begin(92, self.id, 1, 0)?;
		let reservation = self.admit_query(&mut lane, local_velocity, self.row_range.len())?;
		let mut v = agree(
			&mut lane,
			(|| {
				let mut v = reserve_vec(local_velocity.len())?;
				v.extend_from_slice(local_velocity);
				Ok(v)
			})(),
		)?;
		self.whiten(&mut lane, &mut v, 0)?;
		self.apply_q(&mut lane, &mut v, true)?;
		let offset = self
			.numerical_rank
			.saturating_sub(self.row_range.start)
			.min(v.len());
		v.drain(..offset);
		Ok(ChartVector {
			values: v,
			range: self.local_null_range(),
			_reservation: reservation,
		})
	}
	/// Reduced dual force (Q^T L^-1 f)_tail.
	/// # Errors
	/// Rejects input, arithmetic or resource admission.
	pub fn project_force_to_null(&self, local_force: &[f64]) -> Result<ChartVector<'env>> {
		let mut lane = self.environment.begin(92, self.id, 2, 0)?;
		let reservation = self.admit_query(&mut lane, local_force, self.row_range.len())?;
		let mut v = agree(
			&mut lane,
			(|| {
				let mut v = reserve_vec(local_force.len())?;
				v.extend_from_slice(local_force);
				Ok(v)
			})(),
		)?;
		self.whiten(&mut lane, &mut v, 2)?;
		self.apply_q(&mut lane, &mut v, true)?;
		let offset = self
			.numerical_rank
			.saturating_sub(self.row_range.start)
			.min(v.len());
		v.drain(..offset);
		Ok(ChartVector {
			values: v,
			range: self.local_null_range(),
			_reservation: reservation,
		})
	}
	/// Values for all original constraint IDs from the numerical factorization,
	/// sharded by original column ID.
	/// # Errors
	/// Rejects shape, arithmetic and capacity before publishing local output.
	pub fn constraint_values(&self, local_velocity: &[f64]) -> Result<ChartVector<'env>> {
		let mut lane = self.environment.begin(92, self.id, 3, 0)?;
		let reservation = self.admit_query(&mut lane, local_velocity, self.row_range.len())?;
		let (mut v, mut g) = agree(
			&mut lane,
			(|| {
				let mut v = reserve_vec(local_velocity.len())?;
				v.extend_from_slice(local_velocity);
				Ok((v, zeros(self.local_constraint_range().len())?))
			})(),
		)?;
		self.whiten(&mut lane, &mut v, 0)?;
		self.apply_q(&mut lane, &mut v, true)?;
		let range = self.local_constraint_range();
		for (j, &original) in self.pivots.iter().enumerate() {
			let mut value = 0.;
			for k in 0..self.numerical_rank {
				let r = self.get_r(&mut lane, k, j);
				let b = self.get_row(&mut lane, &v, k);
				value = r.mul_add(b, value);
			}
			agree(&mut lane, finite(&[value]))?;
			if range.contains(&original) {
				g[original - range.start] = value;
			}
		}
		Ok(ChartVector {
			values: g,
			range,
			_reservation: reservation,
		})
	}
	/// Solve all constraints, verifying dependent rows before returning a minimum mass norm lift.
	/// # Errors
	/// Rejects incompatible RHS, nonfinite arithmetic, input shape or capacity.
	pub fn lift_constraints(&self, local_rhs: &[f64]) -> Result<ConstraintLift<'env>> {
		let mut lane = self.environment.begin(92, self.id, 4, 0)?;
		let reservation =
			self.admit_query(&mut lane, local_rhs, self.local_constraint_range().len())?;
		let mut b = agree(&mut lane, zeros(self.row_range.len()))?;
		for k in 0..self.numerical_rank {
			let mut value = self.get_column(&mut lane, local_rhs, self.pivots[k]);
			for j in 0..k {
				let r = self.get_r(&mut lane, j, k);
				value = (-r).mul_add(self.get_row(&mut lane, &b, j), value);
			}
			value /= self.get_r(&mut lane, k, k);
			agree(&mut lane, finite(&[value]))?;
			if self.row_range.contains(&k) {
				b[k - self.row_range.start] = value;
			}
		}
		let mut residual = 0_f64;
		let mut rhs_norm = 0_f64;
		for (j, &original) in self.pivots.iter().enumerate() {
			let mut predicted = 0.;
			for k in 0..self.numerical_rank {
				predicted = self
					.get_r(&mut lane, k, j)
					.mul_add(self.get_row(&mut lane, &b, k), predicted);
			}
			let rhs = self.get_column(&mut lane, local_rhs, original);
			residual = residual.hypot(predicted - rhs);
			rhs_norm = rhs_norm.hypot(rhs);
		}
		agree(
			&mut lane,
			if residual.is_finite()
				&& rhs_norm.is_finite()
				&& residual / rhs_norm.max(1.) <= self.policy.compatibility_tolerance
			{
				Ok(())
			} else {
				Err(Error::Value("incompatible constraint RHS"))
			},
		)?;
		self.apply_q(&mut lane, &mut b, false)?;
		self.whiten(&mut lane, &mut b, 1)?;
		Ok(ConstraintLift {
			velocity: ChartVector {
				values: b,
				range: self.local_row_range(),
				_reservation: reservation,
			},
			compatibility_residual: residual,
		})
	}
	/// Generic multiplier solve with dependent pivot multipliers fixed to zero.
	/// This is an algebraic gauge, not a physical pressure normalization/certificate.
	/// # Errors
	/// Rejects shape/nonfinite arithmetic or live resource admission.
	pub fn multipliers_from_force(&self, local_force: &[f64]) -> Result<MultiplierSolution<'env>> {
		let mut lane = self.environment.begin(92, self.id, 5, 0)?;
		let reservation = self.admit_query(&mut lane, local_force, self.row_range.len())?;
		let (mut h, mut lambda) = agree(
			&mut lane,
			(|| {
				let mut h = reserve_vec(local_force.len())?;
				h.extend_from_slice(local_force);
				Ok((h, zeros(self.local_constraint_range().len())?))
			})(),
		)?;
		self.whiten(&mut lane, &mut h, 2)?;
		self.apply_q(&mut lane, &mut h, true)?;
		let range = self.local_constraint_range();
		for k in (0..self.numerical_rank).rev() {
			let mut value = self.get_row(&mut lane, &h, k);
			for j in k + 1..self.numerical_rank {
				value = (-self.get_r(&mut lane, k, j))
					.mul_add(self.get_column(&mut lane, &lambda, self.pivots[j]), value);
			}
			value /= self.get_r(&mut lane, k, k);
			agree(&mut lane, finite(&[value]))?;
			let original = self.pivots[k];
			if range.contains(&original) {
				lambda[original - range.start] = value;
			}
		}
		Ok(MultiplierSolution {
			values: ChartVector {
				values: lambda,
				range,
				_reservation: reservation,
			},
			gauge: MultiplierGauge::PivotedDependentZero,
		})
	}
	/// C^T lambda using retained R and all original multiplier rows.
	/// # Errors
	/// Rejects local shape, arithmetic or capacity.
	pub fn multiplier_force(&self, local_lambda: &[f64]) -> Result<ChartVector<'env>> {
		let mut lane = self.environment.begin(92, self.id, 6, 0)?;
		let reservation =
			self.admit_query(&mut lane, local_lambda, self.local_constraint_range().len())?;
		let mut v = agree(&mut lane, zeros(self.row_range.len()))?;
		for k in 0..self.numerical_rank {
			let mut value = 0.;
			for (j, &original) in self.pivots.iter().enumerate() {
				value = self
					.get_r(&mut lane, k, j)
					.mul_add(self.get_column(&mut lane, local_lambda, original), value);
			}
			agree(&mut lane, finite(&[value]))?;
			if self.row_range.contains(&k) {
				v[k - self.row_range.start] = value;
			}
		}
		self.apply_q(&mut lane, &mut v, false)?;
		self.whiten(&mut lane, &mut v, 3)?;
		Ok(ChartVector {
			values: v,
			range: self.local_row_range(),
			_reservation: reservation,
		})
	}
}
