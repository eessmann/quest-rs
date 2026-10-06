//! Complete cell-local central convection and SIP force from implicit box topology.
//! No mesh, global facet table, matrix, basis catalogue or physical state is retained.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Fixed d<=3,p<=2 tables and checked cell indices bound all local element arithmetic"
)]
use super::{
	BoxConstraintRecipe,
	basis::{Basis, quadrature},
	reserved,
};
use crate::CfdError;
const CONSTRUCTION_WORK: usize = 100_000_000;
const CONSTRUCTION_SCRATCH: usize = 8 * 1024 * 1024;
const CELL_WORK: usize = 1_000_000;
const CELL_SCRATCH: usize = 8192;

/// Fixed-source construction and one complete cell query admission.
#[derive(Clone, Copy, Debug)]
pub struct ForceRecipeLimits {
	pub max_source_bytes: usize,
	pub max_construction_bytes: usize,
	pub max_construction_work: usize,
	pub max_cell_query_work: usize,
	pub max_scratch_bytes: usize,
}
impl Default for ForceRecipeLimits {
	fn default() -> Self {
		Self {
			max_source_bytes: 1024 * 1024,
			max_construction_bytes: 16 * 1024 * 1024,
			max_construction_work: CONSTRUCTION_WORK,
			max_cell_query_work: CELL_WORK,
			max_scratch_bytes: CELL_SCRATCH,
		}
	}
}
#[derive(Clone, Copy)]
struct Volume {
	weight: f64,
	values: [f64; 10],
	gradients: [[f64; 3]; 10],
}
#[derive(Clone, Copy)]
struct Facet {
	weight: f64,
	left: [f64; 10],
	right: [f64; 10],
	left_gradients: [[f64; 3]; 10],
	right_gradients: [[f64; 3]; 10],
}
/// Borrowed full constraint geometry with fixed prepared numeric quadrature tables.
///
/// Basis polynomials are constructed/lowered once through `MathCore`. Cell queries
/// evaluate only these numerical tables and fetch at most d+2 complete cells.
/// Interior convection uses the mean of both normal traces for arbitrary broken
/// states. It agrees with the bounded reference's unique-left trace on the full
/// conforming chart; no conformity is silently assumed by this source.
pub struct BoxForceRecipe<'source> {
	source: &'source BoxConstraintRecipe,
	viscosity: f64,
	lid_speed: f64,
	volume: Vec<Volume>,
	facets: Vec<Facet>,
	volume_count: usize,
	facet_count: usize,
}
const fn invalid(message: &'static str) -> CfdError {
	CfdError::InvalidInput(message)
}
fn gradients(source: &BoxConstraintRecipe, pi: usize) -> [[f64; 3]; 4] {
	let d = source.dimension();
	let p = source.permutation(pi);
	let mut g = [[0.; 3]; 4];
	let scale = 1. / source.cell_width();
	g[0][p[0]] = -scale;
	g[d][p[d - 1]] = scale;
	for k in 1..d {
		g[k][p[k - 1]] = scale;
		g[k][p[k]] = -scale;
	}
	g
}
fn barycentric(source: &BoxConstraintRecipe, pi: usize, x: [f64; 3]) -> [f64; 4] {
	let p = source.permutation(pi);
	let d = source.dimension();
	let mut b = [0.; 4];
	b[0] = 1. - x[p[0]];
	b[d] = x[p[d - 1]];
	for k in 1..d {
		b[k] = x[p[k - 1]] - x[p[k]];
	}
	b
}
fn adjacent(source: &BoxConstraintRecipe, pi: usize, face: usize) -> (usize, [f64; 3]) {
	let d = source.dimension();
	let mut p = source.permutation(pi);
	let mut shift = [0.; 3];
	if face > 0 && face < d {
		p.swap(face - 1, face);
	} else if face == 0 {
		shift[p[0]] = 1.;
		p[..d].rotate_left(1);
	} else {
		shift[p[d - 1]] = -1.;
		p[..d].rotate_right(1);
	}
	(source.permutation_index(p), shift)
}
fn value(words: &[f64; 30], scalar: usize, d: usize, basis: &[f64; 10]) -> [f64; 3] {
	std::array::from_fn(|axis| {
		if axis < d {
			(0..scalar)
				.map(|node| words[axis * scalar + node] * basis[node])
				.sum()
		} else {
			0.
		}
	})
}
fn gradient(words: &[f64; 30], scalar: usize, d: usize, basis: &[[f64; 3]; 10]) -> [[f64; 3]; 3] {
	std::array::from_fn(|component| {
		std::array::from_fn(|axis| {
			if component < d {
				(0..scalar)
					.map(|node| words[component * scalar + node] * basis[node][axis])
					.sum()
			} else {
				0.
			}
		})
	})
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
	a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
impl<'source> BoxForceRecipe<'source> {
	/// Prepare a complete numerical force source with fixed storage independent of cell count.
	/// `lid_speed` specifies the y=L tangential x velocity in either dimension.
	/// # Errors
	/// Rejects invalid viscosity/lid, fixed construction/query/storage budgets,
	/// failed `MathCore` basis preparation or nonrepresentable scaled tables.
	#[allow(
		clippy::too_many_lines,
		reason = "One preflight and fixed `MathCore`-to-numerical table construction"
	)]
	pub fn new(
		source: &'source BoxConstraintRecipe,
		viscosity: f64,
		lid_speed: f64,
		limits: ForceRecipeLimits,
	) -> Result<Self, CfdError> {
		if !viscosity.is_finite()
			|| viscosity < 0.
			|| !lid_speed.is_finite()
			|| (source.periodic() && lid_speed != 0.)
		{
			return Err(invalid("invalid implicit physical force parameters"));
		}
		let d = source.dimension();
		let scalar = source.local_velocity_dimension() / d;
		let volume_count = if d == 2 { 16 } else { 64 };
		let facet_count = if d == 2 { 4 } else { 16 };
		let planned = source.permutation_count() * volume_count * size_of::<Volume>()
			+ source.permutation_count() * (d + 1) * facet_count * size_of::<Facet>()
			+ size_of::<Self>();
		if planned > limits.max_source_bytes
			|| planned + CONSTRUCTION_SCRATCH > limits.max_construction_bytes
			|| CONSTRUCTION_WORK > limits.max_construction_work
			|| CELL_WORK > limits.max_cell_query_work
			|| CELL_SCRATCH > limits.max_scratch_bytes
		{
			return Err(invalid("implicit force construction/query resource budget"));
		}
		let basis = Basis::new(d, source.order())?;
		let mut out = Self {
			source,
			viscosity,
			lid_speed,
			volume: reserved(source.permutation_count() * volume_count)?,
			facets: reserved(source.permutation_count() * (d + 1) * facet_count)?,
			volume_count,
			facet_count,
		};
		let volume_rule = quadrature(d);
		let facet_rule = quadrature(d - 1);
		for pi in 0..source.permutation_count() {
			let g = gradients(source, pi);
			for (bary, weight) in &volume_rule {
				let values = basis.values(bary)?;
				let grad = basis.gradients(bary, &g[..=d])?;
				let mut sample = Volume {
					weight: weight * source.cell_volume(),
					values: [0.; 10],
					gradients: [[0.; 3]; 10],
				};
				sample.values[..scalar].copy_from_slice(&values);
				sample.gradients[..scalar].copy_from_slice(&grad);
				out.volume.push(sample);
			}
			for face in 0..=d {
				let measure = if face == 0 || face == d {
					1.
				} else {
					2_f64.sqrt()
				} * (if d == 2 { 2. } else { 3. })
					* source.cell_volume()
					/ source.cell_width();
				let (right_pi, shift) = adjacent(source, pi, face);
				let rg = gradients(source, right_pi);
				for (bary, weight) in &facet_rule {
					let mut left = [0.; 4];
					let mut index = 0;
					for (node, word) in left.iter_mut().enumerate().take(d + 1) {
						if node != face {
							*word = bary[index];
							index += 1;
						}
					}
					let point = std::array::from_fn(|axis| {
						(0..=d)
							.map(|node| left[node] * f64::from(source.vertex(pi, node)[axis]))
							.sum::<f64>()
							- shift[axis]
					});
					let right = barycentric(source, right_pi, point);
					let values = basis.values(&left[..=d])?;
					let grad = basis.gradients(&left[..=d], &g[..=d])?;
					let rv = basis.values(&right[..=d])?;
					let rgrad = basis.gradients(&right[..=d], &rg[..=d])?;
					let mut sample = Facet {
						weight: weight * measure,
						left: [0.; 10],
						right: [0.; 10],
						left_gradients: [[0.; 3]; 10],
						right_gradients: [[0.; 3]; 10],
					};
					sample.left[..scalar].copy_from_slice(&values);
					sample.right[..scalar].copy_from_slice(&rv);
					sample.left_gradients[..scalar].copy_from_slice(&grad);
					sample.right_gradients[..scalar].copy_from_slice(&rgrad);
					out.facets.push(sample);
				}
			}
		}
		if out.source_bytes() > limits.max_source_bytes
			|| out.source_bytes() + CONSTRUCTION_SCRATCH > limits.max_construction_bytes
			|| out.volume.iter().any(|s| {
				!s.weight.is_finite()
					|| s.values
						.iter()
						.chain(s.gradients.iter().flatten())
						.any(|v| !v.is_finite())
			})
			|| out.facets.iter().any(|s| {
				!s.weight.is_finite()
					|| s.left
						.iter()
						.chain(&s.right)
						.chain(s.left_gradients.iter().flatten())
						.chain(s.right_gradients.iter().flatten())
						.any(|v| !v.is_finite())
			}) {
			return Err(invalid(
				"implicit force table capacity or arithmetic overflow",
			));
		}
		Ok(out)
	}
	/// Retained source bytes including actual table capacities; excludes borrowed geometry.
	#[must_use]
	pub const fn source_bytes(&self) -> usize {
		size_of::<Self>()
			+ self.volume.capacity() * size_of::<Volume>()
			+ self.facets.capacity() * size_of::<Facet>()
	}
	/// Fixed construction work includes bounded `MathCore` preparation and every table kernel.
	#[must_use]
	pub const fn construction_work() -> usize {
		CONSTRUCTION_WORK
	}
	/// Elementary operations, fixed coefficient copies and topology/index selection per cell.
	/// Additional work of an arbitrary caller-supplied callback is the caller's obligation.
	#[must_use]
	pub const fn cell_query_work() -> usize {
		CELL_WORK
	}
	/// Fixed stack scratch upper bound, independent of the number of physical cells.
	#[must_use]
	pub const fn cell_scratch_bytes() -> usize {
		CELL_SCRATCH
	}
	#[cfg(feature = "distributed")]
	pub(super) const fn source(&self) -> &BoxConstraintRecipe {
		self.source
	}
	#[cfg(feature = "distributed")]
	pub(super) const fn parameters(&self) -> [f64; 2] {
		[self.viscosity, self.lid_speed]
	}
	/// Evaluate every force coefficient of one owned cell, fetching at most d+2 full cells.
	/// Fetch must observe one immutable physical state across repeated cell IDs.
	/// Unused array entries are zero. Callback failures/overflow return no partial force.
	/// # Errors
	/// Rejects invalid cell indices before callbacks, malformed/nonfinite fetched words,
	/// callback errors and complete force arithmetic overflow.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep the complete volume and both-side face force in one bounded cell query"
	)]
	pub fn cell_force(
		&self,
		cell: usize,
		fetch: &mut impl FnMut(usize) -> Result<[f64; 30], CfdError>,
	) -> Result<[f64; 30], CfdError> {
		let d = self.source.dimension();
		let scalar = self.source.local_velocity_dimension() / d;
		let mut trace = [[0.; 30]; 4];
		for (face, row) in trace.iter_mut().enumerate().take(d + 1) {
			if self.source.facet_is_exterior(cell, face)?
				&& self
					.source
					.normal(cell % self.source.permutation_count(), face)[1]
					> 0.5
			{
				for mode in 0..self.source.facet_mode_count() {
					row[self.source.facet_velocity_node(face, mode)?] = self.lid_speed;
				}
			}
		}
		debug_assert!(scalar <= 10);
		self.cell_force_with_data(cell, fetch, &trace, &[0.; 30])
	}
	/// Full nodal exterior traces and mass-integrated body acceleration.
	/// Trace arrays embed facet nodes in the cell's component/scalar-node layout.
	/// Unused faces, off-facet nodes and inactive components must be zero.
	/// # Errors
	/// Rejects malformed data, failed immutable state fetch, or nonfinite arithmetic.
	pub fn cell_force_with_data(
		&self,
		cell: usize,
		fetch: &mut impl FnMut(usize) -> Result<[f64; 30], CfdError>,
		trace: &[[f64; 30]; 4],
		body: &[f64; 30],
	) -> Result<[f64; 30], CfdError> {
		super::time_data::validate_fields(self.source, cell, &[0.; 30], body, trace)?;
		let source = self.source;
		let d = source.dimension();
		let scalar = source.local_velocity_dimension() / d;
		if cell >= source.cell_count() {
			return Err(invalid("implicit force cell index"));
		}
		let mut words = [[0.; 30]; 5];
		words[0] = fetch(cell)?;
		for face in 0..=d {
			if let Some((other, _)) = source.partner(cell, face) {
				words[face + 1] = fetch(other)?;
			}
		}
		if words.iter().flatten().any(|v| !v.is_finite()) {
			return Err(invalid("nonfinite implicit broken cell state"));
		}
		let mut force = [0.; 30];
		let pi = cell % source.permutation_count();
		for sample in &self.volume[pi * self.volume_count..(pi + 1) * self.volume_count] {
			let velocity = value(&words[0], scalar, d, &sample.values);
			let grad = gradient(&words[0], scalar, d, &sample.gradients);
			for component in 0..d {
				for node in 0..scalar {
					force[component * scalar + node] += sample.weight
						* (velocity[component] * dot(velocity, sample.gradients[node])
							- self.viscosity * dot(grad[component], sample.gradients[node]));
				}
			}
		}
		for face in 0..=d {
			let has_neighbour = source.partner(cell, face).is_some();
			let normal = source.normal(pi, face);
			let average = if has_neighbour { 0.5 } else { 1. };
			let penalty = 10.
				* if source.order() == 1 { 4. } else { 9. }
				* if face == 0 || face == d {
					1.
				} else {
					2_f64.sqrt()
				}
				/ source.cell_width();
			let offset = (pi * (d + 1) + face) * self.facet_count;
			for sample in &self.facets[offset..offset + self.facet_count] {
				let left = value(&words[0], scalar, d, &sample.left);
				let lg = gradient(&words[0], scalar, d, &sample.left_gradients);
				let right = if has_neighbour {
					value(&words[face + 1], scalar, d, &sample.right)
				} else {
					value(&trace[face], scalar, d, &sample.left)
				};
				let rg = if has_neighbour {
					gradient(&words[face + 1], scalar, d, &sample.right_gradients)
				} else {
					[[0.; 3]; 3]
				};
				// Boundary convective flux uses the actual left trace; on the homogeneous
				// normal chart it is zero. Interior traces are explicitly averaged.
				let normal_velocity = if has_neighbour {
					f64::midpoint(dot(left, normal), dot(right, normal))
				} else {
					dot(left, normal)
				};
				for component in 0..d {
					for node in 0..scalar {
						let jump = left[component] - right[component];
						let phi = sample.left[node];
						force[component * scalar + node] += sample.weight
							* (-normal_velocity * 0.5 * (left[component] + right[component]) * phi
								+ self.viscosity
									* (average * dot(sample.left_gradients[node], normal) * jump
										+ phi
											* average
											* (dot(lg[component], normal)
												+ dot(rg[component], normal))
										- penalty * phi * jump));
					}
				}
			}
		}
		for (i, out) in force
			.iter_mut()
			.enumerate()
			.take(source.local_velocity_dimension())
		{
			for (j, &load) in body
				.iter()
				.enumerate()
				.take(source.local_velocity_dimension())
			{
				*out += source.mass_value(cell, i, j)? * load;
			}
		}
		if force.iter().any(|v| !v.is_finite()) {
			return Err(invalid("implicit complete force overflow"));
		}
		Ok(force)
	}
}
