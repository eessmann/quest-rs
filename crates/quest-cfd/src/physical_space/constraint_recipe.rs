//! Implicit complete box constraints. No global mesh, connectivity or coefficient table.
//!
//! Every cell owns all of its facet trace rows, so interior/periodic rows occur twice.
//! This changes multiplier coordinates, not the constrained velocity space. Multipliers
//! from this source are deliberately **not** a physical pressure reconstruction.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::many_single_char_names,
	reason = "Fixed d<=3, p<=2 element algebra; global counts and public indices are checked"
)]
use crate::{CfdError, simplex::BoxBoundary};
const PERMUTATIONS: [[usize; 3]; 6] = [
	[0, 1, 2],
	[0, 2, 1],
	[1, 0, 2],
	[1, 2, 0],
	[2, 0, 1],
	[2, 1, 0],
];
const CONSTRUCTION_WORK: usize = 1_000_000;
/// Upper bound on elementary arithmetic/index operations per generated scalar.
const QUERY_WORK: usize = 4096;
/// Admission for source storage and conservative elementary-operation counts.
#[derive(Clone, Copy, Debug)]
pub struct ConstraintRecipeLimits {
	pub max_source_bytes: usize,
	pub max_construction_work: usize,
	pub max_scalar_query_work: usize,
}
impl Default for ConstraintRecipeLimits {
	fn default() -> Self {
		Self {
			max_source_bytes: 65536,
			max_construction_work: CONSTRUCTION_WORK,
			max_scalar_query_work: QUERY_WORK,
		}
	}
}
/// Complete implicit BDM1/P0 or BDM2/P1 box constraint source.
///
/// Cell order is x-fastest Cartesian cubes, then lexicographic axis permutations.
/// Local vector coefficients are component-major nodal Lagrange coefficients.
/// Fixed element tables are independent of the number of cells. Construction and
/// scalar queries allocate no heap storage. No physical coordinate is discarded.
#[derive(Debug)]
pub struct BoxConstraintRecipe {
	dimension: usize,
	subdivisions: usize,
	extent: f64,
	cell_volume: f64,
	cell_gradient_scale: f64,
	periodic: bool,
	order: usize,
	permutations: usize,
	scalar: usize,
	facet_modes: usize,
	pressure_modes: usize,
	cells: usize,
	constraints: usize,
	normals: usize,
	nullity: usize,
	mass: [[f64; 10]; 10],
	divergence: [[[f64; 30]; 4]; 6],
	normal: [[[f64; 3]; 4]; 6],
}
const fn overflow() -> CfdError {
	CfdError::InvalidInput("implicit box count overflow")
}
fn factorial(n: u32) -> f64 {
	(1..=n).map(f64::from).product()
}
fn average(exponents: [u32; 4], d: usize) -> f64 {
	let degree: u32 = exponents.iter().sum();
	factorial(u32::try_from(d).unwrap_or(3))
		* exponents.iter().map(|&n| factorial(n)).product::<f64>()
		/ factorial(u32::try_from(d).unwrap_or(3) + degree)
}
#[allow(
	clippy::unreachable,
	reason = "All callers enumerate validated simplex vertex/edge nodes"
)]
fn edge(d: usize, index: usize) -> (usize, usize) {
	let mut k = d + 1;
	for i in 0..=d {
		for j in i + 1..=d {
			if k == index {
				return (i, j);
			}
			k += 1;
		}
	}
	unreachable!("validated quadratic nodal index")
}
fn polynomial(d: usize, p: usize, node: usize) -> [(f64, [u32; 4]); 2] {
	let mut a = [0; 4];
	if node <= d {
		a[node] = 1;
		if p == 1 {
			[(1., a), (0., [0; 4])]
		} else {
			let mut b = a;
			b[node] = 2;
			[(2., b), (-1., a)]
		}
	} else {
		let (i, j) = edge(d, node);
		a[i] = 1;
		a[j] = 1;
		[(4., a), (0., [0; 4])]
	}
}
impl BoxConstraintRecipe {
	/// Construct fixed element tables and checked implicit topology counts.
	/// Cavity lid speed affects tangential forcing, not these homogeneous constraints.
	/// # Errors
	/// Rejects unsupported parameters, nonrepresentable geometry, overflow or source budgets.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep fixed-table admission and analytic element construction together"
	)]
	pub fn new(
		dimension: usize,
		subdivisions: u32,
		extent: f64,
		boundary: BoxBoundary,
		order: usize,
		limits: ConstraintRecipeLimits,
	) -> Result<Self, CfdError> {
		if ![2, 3].contains(&dimension)
			|| ![1, 2].contains(&order)
			|| subdivisions == 0
			|| !extent.is_finite()
			|| extent <= 0.
			|| boundary == BoxBoundary::Mixed
			|| matches!(boundary,BoxBoundary::Cavity{lid_speed} if !lid_speed.is_finite())
		{
			return Err(CfdError::InvalidInput("invalid implicit box parameters"));
		}
		if limits.max_source_bytes < Self::construction_peak_bytes()
			|| limits.max_construction_work < CONSTRUCTION_WORK
			|| limits.max_scalar_query_work < QUERY_WORK
		{
			return Err(CfdError::InvalidInput(
				"implicit box source budget exceeded",
			));
		}
		let d = dimension;
		let periodic = boundary == BoxBoundary::Periodic;
		let (broken, rank) =
			crate::cases_high_order::box_chart_dimensions(d, subdivisions, periodic, order)?;
		let scalar = if order == 1 {
			d + 1
		} else {
			(d + 1) * (d + 2) / 2
		};
		let facet_modes = if order == 1 { d } else { d * (d + 1) / 2 };
		let pressure_modes = if order == 1 { 1 } else { d + 1 };
		let cells = broken / (d * scalar);
		let normals = cells
			.checked_mul((d + 1) * facet_modes)
			.ok_or_else(overflow)?;
		let constraints = cells
			.checked_mul(pressure_modes)
			.and_then(|v| v.checked_add(normals))
			.ok_or_else(overflow)?;
		let h = extent / f64::from(subdivisions);
		let volume = if d == 2 { h * h / 2. } else { h * h * h / 6. };
		let scale = volume / h;
		if !h.is_finite()
			|| !volume.is_finite()
			|| !scale.is_finite()
			|| volume <= 0.
			|| scale <= 0.
		{
			return Err(CfdError::InvalidInput(
				"nonrepresentable implicit box geometry",
			));
		}
		let mut out = Self {
			dimension: d,
			subdivisions: usize::try_from(subdivisions).map_err(|_| overflow())?,
			extent,
			cell_volume: volume,
			cell_gradient_scale: scale,
			periodic,
			order,
			permutations: if d == 2 { 2 } else { 6 },
			scalar,
			facet_modes,
			pressure_modes,
			cells,
			constraints,
			normals,
			nullity: broken - rank,
			mass: [[0.; 10]; 10],
			divergence: [[[0.; 30]; 4]; 6],
			normal: [[[0.; 3]; 4]; 6],
		};
		for i in 0..scalar {
			for j in i..scalar {
				let mut value = 0.;
				for (a, e) in polynomial(d, order, i) {
					for (b, f) in polynomial(d, order, j) {
						value += a * b * average(std::array::from_fn(|k| e[k] + f[k]), d);
					}
				}
				out.mass[i][j] = volume * value;
				out.mass[j][i] = out.mass[i][j]; // Native SPD admission requires bitwise symmetry.
			}
		}
		for pi in 0..out.permutations {
			let perm = out.permutation(pi);
			let mut gradients = [[0.; 3]; 4];
			gradients[0][perm[0]] = -1.;
			gradients[d][perm[d - 1]] = 1.;
			for k in 1..d {
				gradients[k][perm[k - 1]] = 1.;
				gradients[k][perm[k]] = -1.;
			}
			for (face, g) in gradients.iter().enumerate().take(d + 1) {
				let norm = g.iter().map(|x| x * x).sum::<f64>().sqrt();
				out.normal[pi][face] = std::array::from_fn(|axis| -g[axis] / norm);
			}
			for pressure in 0..pressure_modes {
				for axis in 0..d {
					for node in 0..scalar {
						let mut value = 0.;
						for (coefficient, e) in polynomial(d, order, node) {
							for k in 0..=d {
								if e[k] == 0 {
									continue;
								}
								let mut reduced = e;
								reduced[k] -= 1;
								if order == 2 {
									reduced[pressure] += 1;
								}
								value += coefficient
									* f64::from(e[k])
									* gradients[k][axis]
									* average(reduced, d);
							}
						}
						out.divergence[pi][pressure][axis * scalar + node] = scale * value;
					}
				}
			}
		}
		if out
			.mass
			.iter()
			.flatten()
			.chain(out.divergence.iter().flatten().flatten())
			.any(|x| !x.is_finite())
			|| (0..scalar).any(|i| out.mass[i][i] <= 0.)
		{
			return Err(CfdError::InvalidInput(
				"nonrepresentable implicit element tables",
			));
		}
		Ok(out)
	}
	pub(super) const fn permutation(&self, index: usize) -> [usize; 3] {
		if self.dimension == 2 {
			if index == 0 { [0, 1, 2] } else { [1, 0, 2] }
		} else {
			PERMUTATIONS[index]
		}
	}
	pub(super) fn permutation_index(&self, p: [usize; 3]) -> usize {
		(0..self.permutations)
			.find(|&i| self.permutation(i) == p)
			.unwrap_or(0)
	}
	pub(super) const fn cube(&self, cell: usize) -> [usize; 3] {
		let cube = cell / self.permutations;
		let n = self.subdivisions;
		[
			cube % n,
			(cube / n) % n,
			if self.dimension == 3 { cube / n / n } else { 0 },
		]
	}
	const fn cell(&self, cube: [usize; 3], pi: usize) -> usize {
		((cube[2] * self.subdivisions + cube[1]) * self.subdivisions + cube[0]) * self.permutations
			+ pi
	}
	/// Adjacent cell, local facet, and translation from its cube to the owner's cube.
	pub(super) fn partner(&self, cell: usize, face: usize) -> Option<(usize, [i32; 3])> {
		let d = self.dimension;
		let mut p = self.permutation(cell % self.permutations);
		let mut cube = self.cube(cell);
		let mut shift = [0; 3];
		if face > 0 && face < d {
			p.swap(face - 1, face);
		} else {
			let (axis, forward) = if face == 0 {
				(p[0], true)
			} else {
				(p[d - 1], false)
			};
			shift[axis] = if forward { 1 } else { -1 };
			if forward {
				if cube[axis] + 1 == self.subdivisions {
					if !self.periodic {
						return None;
					}
					cube[axis] = 0;
				} else {
					cube[axis] += 1;
				}
				p[..d].rotate_left(1);
			} else {
				if cube[axis] == 0 {
					if !self.periodic {
						return None;
					}
					cube[axis] = self.subdivisions - 1;
				} else {
					cube[axis] -= 1;
				}
				p[..d].rotate_right(1);
			}
		}
		Some((self.cell(cube, self.permutation_index(p)), shift))
	}
	pub(super) fn vertex(&self, pi: usize, index: usize) -> [i32; 3] {
		let p = self.permutation(pi);
		let mut v = [0; 3];
		for &axis in p.iter().take(index) {
			v[axis] = 1;
		}
		v
	}
	fn face_node(&self, face: usize, index: usize) -> (usize, Option<usize>) {
		let mut vertices = [0; 3];
		let mut k = 0;
		for v in 0..=self.dimension {
			if v != face {
				vertices[k] = v;
				k += 1;
			}
		}
		if index < self.dimension {
			(vertices[index], None)
		} else {
			let (i, j) = edge(self.dimension - 1, index);
			(vertices[i], Some(vertices[j]))
		}
	}
	fn node_index(&self, i: usize, j: Option<usize>) -> usize {
		j.map_or(i, |j| {
			let target = (i.min(j), i.max(j));
			(self.dimension + 1..self.scalar)
				.find(|&k| edge(self.dimension, k) == target)
				.unwrap_or(0)
		})
	}
	/// Number of complete scalar normal modes on one facet.
	#[must_use]
	pub const fn facet_mode_count(&self) -> usize {
		self.facet_modes
	}
	/// Local velocity scalar node for a facet mode in vertex/edge order.
	/// # Errors
	/// Rejects out-of-range face or mode.
	pub fn facet_velocity_node(&self, face: usize, mode: usize) -> Result<usize, CfdError> {
		if face > self.dimension || mode >= self.facet_modes {
			return Err(CfdError::InvalidInput("facet mode index"));
		}
		let (a, b) = self.face_node(face, mode);
		Ok(self.node_index(a, b))
	}
	/// Whether an implicit face carries prescribed exterior data.
	/// # Errors
	/// Rejects out-of-range cell or face.
	pub fn facet_is_exterior(&self, cell: usize, face: usize) -> Result<bool, CfdError> {
		if cell >= self.cells || face > self.dimension {
			return Err(CfdError::InvalidInput("exterior facet index"));
		}
		Ok(self.partner(cell, face).is_none())
	}
	pub(super) const fn pressure_modes(&self) -> usize {
		self.pressure_modes
	}
	/// Number of implicit simplices.
	#[must_use]
	pub const fn cell_count(&self) -> usize {
		self.cells
	}
	/// Broken vector coefficients per simplex.
	#[must_use]
	pub const fn local_velocity_dimension(&self) -> usize {
		self.dimension * self.scalar
	}
	/// Redundant facet rows plus every discontinuous pressure divergence row.
	#[must_use]
	pub const fn constraint_count(&self) -> usize {
		self.constraints
	}
	/// Topological nullity; distributed numerical rank must independently agree.
	#[must_use]
	pub const fn expected_nullity(&self) -> usize {
		self.nullity
	}
	/// Retained source storage, independent of mesh size (no heap allocations).
	#[must_use]
	pub const fn source_bytes(&self) -> usize {
		std::mem::size_of::<Self>()
	}
	/// Conservative stack-storage admission, including construction temporaries/copies.
	#[must_use]
	pub const fn construction_peak_bytes() -> usize {
		3 * std::mem::size_of::<Self>() + 8192
	}
	/// Conservative elementary-operation upper bound for either scalar query.
	#[must_use]
	pub const fn scalar_query_work() -> usize {
		QUERY_WORK
	}
	/// Row-major cell mass scalar. Component-major vector basis.
	/// # Errors
	/// Rejects any cell or local index outside the full physical space.
	pub const fn mass_value(&self, cell: usize, i: usize, j: usize) -> Result<f64, CfdError> {
		if cell >= self.cells
			|| i >= self.local_velocity_dimension()
			|| j >= self.local_velocity_dimension()
		{
			return Err(CfdError::InvalidInput("implicit mass index out of bounds"));
		}
		Ok(if i / self.scalar == j / self.scalar {
			self.mass[i % self.scalar][j % self.scalar]
		} else {
			0.
		})
	}
	/// Scalar of C^T. Facet columns precede cell-major divergence columns.
	/// # Errors
	/// Rejects out-of-range indices, including structurally empty entries.
	pub fn constraint_value(&self, cell: usize, i: usize, column: usize) -> Result<f64, CfdError> {
		if cell >= self.cells || i >= self.local_velocity_dimension() || column >= self.constraints
		{
			return Err(CfdError::InvalidInput(
				"implicit constraint index out of bounds",
			));
		}
		if column >= self.normals {
			let q = column - self.normals;
			return Ok(if q / self.pressure_modes == cell {
				self.divergence[cell % self.permutations][q % self.pressure_modes][i]
			} else {
				0.
			});
		}
		let per_cell = (self.dimension + 1) * self.facet_modes;
		let owner = column / per_cell;
		let face = (column % per_cell) / self.facet_modes;
		let (a, b) = self.face_node(face, column % self.facet_modes);
		let mut value = 0.;
		let normal = self.normal[owner % self.permutations][face][i / self.scalar];
		if owner == cell && self.node_index(a, b) == i % self.scalar {
			value += normal;
		}
		if let Some((neighbor, shift)) = self.partner(owner, face)
			&& neighbor == cell
		{
			let map = |node| {
				let v = self.vertex(owner % self.permutations, node);
				(0..=self.dimension)
					.find(|&other| {
						let w = self.vertex(neighbor % self.permutations, other);
						(0..self.dimension).all(|axis| v[axis] == w[axis] + shift[axis])
					})
					.unwrap_or(0)
			};
			if self.node_index(map(a), b.map(map)) == i % self.scalar {
				value -= normal;
			}
		}
		Ok(value)
	}
}

/// Source costs charged in addition to native factorization work and storage.
#[derive(Clone, Copy, Debug)]
pub struct ConstraintSourceCosts {
	/// Fixed source plus conservative query/adapter stack allowance, live alongside QR.
	pub live_bytes: usize,
	/// Maximum number of scalar callbacks on any rank (whole-cell partition).
	pub callback_count_per_rank: usize,
	/// Source construction plus callback-internal elementary-operation upper bound.
	pub work_per_rank: usize,
	/// Conservative source-identity/reservation control-traffic allowance, additional to QR.
	pub coordination_bytes: usize,
}
impl BoxConstraintRecipe {
	/// Conservative source contribution for a whole-cell distributed factory.
	/// # Errors
	/// Rejects a zero rank count and arithmetic overflow.
	pub fn factory_source_costs(&self, ranks: usize) -> Result<ConstraintSourceCosts, CfdError> {
		if ranks == 0 {
			return Err(CfdError::InvalidInput("zero recipe ranks"));
		}
		let local = self.local_velocity_dimension();
		let cells = self.cells / ranks + usize::from(!self.cells.is_multiple_of(ranks));
		let callbacks = local
			.checked_add(self.constraints)
			.and_then(|v| v.checked_mul(local))
			.and_then(|v| v.checked_mul(cells))
			.ok_or_else(overflow)?;
		let work = callbacks
			.checked_mul(QUERY_WORK)
			.and_then(|v| v.checked_add(CONSTRUCTION_WORK))
			.ok_or_else(overflow)?;
		Ok(ConstraintSourceCosts {
			live_bytes: self.source_bytes() + 8192,
			callback_count_per_rank: callbacks,
			work_per_rank: work,
			coordination_bytes: ranks
				.checked_mul(ranks)
				.and_then(|v| v.checked_mul(1024))
				.ok_or_else(overflow)?,
		})
	}
	/// First pressure-divergence constraint, after redundant normal traces.
	#[must_use]
	pub const fn pressure_constraint_start(&self) -> usize {
		self.normals
	}
	/// Integral of one cell pressure basis function (uniform affine boxes).
	#[must_use]
	pub const fn pressure_integral_weight(&self) -> f64 {
		self.cell_volume
			/ if self.order == 1 {
				1.
			} else if self.dimension == 2 {
				3.
			} else {
				4.
			}
	}
	/// Null multiplier direction for removing a constant pressure multiplier.
	/// # Errors
	/// Rejects an invalid constraint or nonrepresentable facet integral.
	pub fn pressure_gauge_coefficient(&self, column: usize) -> Result<f64, CfdError> {
		if column >= self.constraints {
			return Err(CfdError::InvalidInput("pressure gauge index"));
		}
		if column >= self.normals {
			return Ok(-1.);
		}
		let per_cell = (self.dimension + 1) * self.facet_modes;
		let cell = column / per_cell;
		let face = (column % per_cell) / self.facet_modes;
		let node = column % self.facet_modes;
		let scale = if face == 0 || face == self.dimension {
			1.
		} else {
			2_f64.sqrt()
		};
		let measure = self.cell_gradient_scale * if self.dimension == 2 { 2. } else { 3. } * scale;
		let integral = if self.order == 1 {
			measure / if self.dimension == 2 { 2. } else { 3. }
		} else if node < self.dimension {
			if self.dimension == 2 {
				measure / 6.
			} else {
				0.
			}
		} else if self.dimension == 2 {
			2. * measure / 3.
		} else {
			measure / 3.
		};
		let value = integral
			* if self.partner(cell, face).is_some() {
				0.5
			} else {
				1.
			};
		if !value.is_finite() {
			return Err(CfdError::InvalidInput("pressure facet integral overflow"));
		}
		Ok(value)
	}
	/// Physical dimension of the box.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.dimension
	}
	/// Nodal polynomial degree, one or two.
	#[must_use]
	pub const fn order(&self) -> usize {
		self.order
	}
	/// Length of each box edge.
	#[must_use]
	pub const fn extent(&self) -> f64 {
		self.extent
	}
	pub(super) const fn periodic(&self) -> bool {
		self.periodic
	}
	pub(super) const fn permutation_count(&self) -> usize {
		self.permutations
	}
	pub(super) const fn cell_volume(&self) -> f64 {
		self.cell_volume
	}
	pub(super) fn cell_width(&self) -> f64 {
		self.cell_volume / self.cell_gradient_scale
	}
	pub(super) const fn normal(&self, pi: usize, face: usize) -> [f64; 3] {
		self.normal[pi][face]
	}
}

#[cfg(feature = "distributed")]
mod collective {
	use super::{BoxConstraintRecipe, ConstraintSourceCosts};
	use quest::{
		Error, Result,
		collective::{CollectiveEnvironment, CollectiveReservation},
		distributed_constraints::{
			ChartOutcome, ConstraintChart, ConstraintLimits, RankAmbiguity, RankPolicy,
		},
	};
	/// A complete chart with its source memory reservation still live.
	///
	/// The recipe borrow prevents the accounted source from being destroyed or replaced.
	/// Native generic multiplier coordinates are not physical pressure coefficients.
	pub struct PreparedBoxConstraints<'source, 'env, 'comm, 'runtime> {
		source: &'source BoxConstraintRecipe,
		environment: &'env CollectiveEnvironment<'comm, 'runtime>,
		chart: ConstraintChart<'env, 'comm, 'runtime>,
		costs: ConstraintSourceCosts,
		_source_reservation: CollectiveReservation<'env>,
	}
	/// Preserve the native rank-ambiguity outcome instead of silently dropping modes.
	#[allow(
		clippy::large_enum_variant,
		reason = "Bounded stack owner avoids an unadmitted infallible Box allocation"
	)]
	pub enum BoxConstraintOutcome<'source, 'env, 'comm, 'runtime> {
		Prepared(PreparedBoxConstraints<'source, 'env, 'comm, 'runtime>),
		Ambiguous(RankAmbiguity),
	}
	impl<'env, 'comm, 'runtime> PreparedBoxConstraints<'_, 'env, 'comm, 'runtime> {
		pub(crate) const fn environment(&self) -> &'env CollectiveEnvironment<'comm, 'runtime> {
			self.environment
		}
		/// Borrow the complete distributed numerical chart.
		#[must_use]
		pub const fn chart(&self) -> &ConstraintChart<'env, 'comm, 'runtime> {
			&self.chart
		}
		/// Accounted implicit source.
		#[must_use]
		pub const fn source(&self) -> &BoxConstraintRecipe {
			self.source
		}
		/// Conservative per-rank source costs, additional to `chart.resources()`.
		#[must_use]
		pub const fn source_costs(&self) -> ConstraintSourceCosts {
			self.costs
		}
	}
	impl BoxConstraintRecipe {
		/// Generate only rank-owned physical cells and build the complete distributed chart.
		///
		/// All ranks must enter in the same order with successfully constructed recipes.
		/// Exact geometry/order/boundary metadata is collectively compared before callbacks.
		/// The shared `max_construct_work` budget includes source work (conservatively using
		/// the largest whole-cell shard), callback invocations, and native QR work.
		/// Source memory remains reserved for the returned owner's lifetime and therefore
		/// participates in subsequent native rank/node/query admission.
		///
		/// This does not provide sparse or cheap QR: native fill, global constraint labels,
		/// pressure nonlocality and communication remain included in the native report.
		/// # Errors
		/// Collectively rejects mismatched source identity, work/storage admission, native
		/// preparation errors, or numerical rank inconsistent with complete box topology.
		pub fn prepare_collective<'source, 'env, 'comm, 'runtime>(
			&'source self,
			env: &'env CollectiveEnvironment<'comm, 'runtime>,
			mut limits: ConstraintLimits,
			policy: RankPolicy,
		) -> Result<BoxConstraintOutcome<'source, 'env, 'comm, 'runtime>> {
			let ranks = usize::try_from(env.size()?).map_err(|_| Error::Overflow)?;
			let costs = self
				.factory_source_costs(ranks)
				.map_err(|_| Error::Overflow);
			{
				let mut lane =
					env.communicator()
						.collective_lane()
						.map_err(|source| Error::Backend {
							operation: "opening physical source agreement",
							source,
						})?;
				let words = [
					0x4346_4442_4f58_0001,
					u64::try_from(self.dimension).unwrap_or(0),
					u64::try_from(self.subdivisions).unwrap_or(0),
					self.extent.to_bits(),
					u64::from(self.periodic),
					u64::try_from(self.order).unwrap_or(0),
				];
				let mut identity = [0; 48];
				for (chunk, word) in identity.as_chunks_mut::<8>().0.iter_mut().zip(words) {
					chunk.copy_from_slice(&word.to_le_bytes());
				}
				let local = identity;
				lane.broadcast_bytes(0, &mut identity)
					.map_err(|source| Error::Backend {
						operation: "broadcasting physical source identity",
						source,
					})?;
				let accepted = costs.as_ref().is_ok_and(|c| {
					c.work_per_rank <= limits.max_construct_work
						&& c.coordination_bytes <= limits.max_transport_bytes
				});
				if !lane
					.all_agree(local == identity && accepted)
					.map_err(|source| Error::Backend {
						operation: "agreeing physical source admission",
						source,
					})? {
					return Err(Error::Value("physical source identity or work admission"));
				}
			}
			let costs = costs?;
			limits.max_construct_work = limits
				.max_construct_work
				.checked_sub(costs.work_per_rank)
				.ok_or(Error::Overflow)?;
			limits.max_transport_bytes = limits
				.max_transport_bytes
				.checked_sub(costs.coordination_bytes)
				.ok_or(Error::Overflow)?;
			let reservation = env.reserve_external_bytes(costs.live_bytes)?;
			let outcome = env.prepare_constraint_chart_from_fn(
				self.cells,
				self.local_velocity_dimension(),
				self.constraints,
				limits,
				policy,
				|cell, i, j| {
					self.mass_value(cell, i, j)
						.map_err(|_| Error::Value("generated physical mass"))
				},
				|cell, i, j| {
					self.constraint_value(cell, i, j)
						.map_err(|_| Error::Value("generated physical constraint"))
				},
			)?;
			match outcome {
				ChartOutcome::Ambiguous(a) => Ok(BoxConstraintOutcome::Ambiguous(a)),
				ChartOutcome::Prepared(chart) => {
					if chart.nullity() != self.nullity {
						return Err(Error::Value(
							"physical chart rank disagrees with complete box topology",
						));
					}
					Ok(BoxConstraintOutcome::Prepared(PreparedBoxConstraints {
						source: self,
						environment: env,
						chart,
						costs,
						_source_reservation: reservation,
					}))
				}
			}
		}
	}
}
#[cfg(feature = "distributed")]
pub use collective::{BoxConstraintOutcome, PreparedBoxConstraints};
