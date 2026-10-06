//! Complete bounded BDM1/P0 and BDM2/P1 physical spaces on affine simplices.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Bounded full physical dimensions and independently verified finite-element formulas"
)]
mod affine_lifting;
pub use affine_lifting::{CanonicalLiftingLimits, CanonicalLiftingResources, CanonicalLiftings};
mod basis;
mod boundary;
pub use boundary::{
	BoundaryExtractionLimits, BoundaryFacetLayout, BoundaryLimits, BoundaryResources,
	BoundaryTimeCoefficient, ConvectionPower, NaturalTractionTimeCoefficient, PolynomialBoundary,
};
mod chart;
mod constraint_qr;
mod constraint_recipe;
mod force_recipe;
mod time_data;
pub use force_recipe::{BoxForceRecipe, ForceRecipeLimits};
pub use time_data::{
	BoxTimeCoefficient, BoxTimeDataLimits, BoxTimeDataResources, BoxTimeDataShard,
	BoxTimeEvaluation,
};
#[cfg(feature = "distributed")]
mod time_collective;
#[cfg(feature = "distributed")]
pub use time_collective::{
	DistributedTimeBoxDrift, DistributedTimeForceResources, DistributedTimePressure,
	PreparedTimeBoxForce,
};
#[cfg(feature = "distributed")]
mod force_collective;
#[cfg(feature = "distributed")]
pub use force_collective::{
	DistributedBoxDrift, DistributedForceLimits, DistributedForceResources, PreparedBoxForce,
};
#[cfg(feature = "distributed")]
mod constraint_pressure;
#[cfg(feature = "distributed")]
pub use constraint_pressure::{
	DistributedPhysicalPressure, DistributedPressureLimits, DistributedPressureResources,
};
#[cfg(feature = "distributed")]
pub use constraint_recipe::{BoxConstraintOutcome, PreparedBoxConstraints};
pub use constraint_recipe::{BoxConstraintRecipe, ConstraintRecipeLimits, ConstraintSourceCosts};
mod mesh;
mod mesh_geometry;
mod mesh_topology;
mod mixed_mesh;
pub use mesh::{
	AffineMeshView, DirichletFacet, PhysicalGeometryKind, PhysicalMeshAttempt, PhysicalMeshLimits,
	PhysicalMeshQueryResources, PhysicalMeshResources, TranslationalPeriodicFacet,
};
pub use mixed_mesh::{ExteriorCondition, ExteriorFacet, MixedAffineMeshView};
mod observables;
mod pressure_observables;
pub use pressure_observables::{BoxBoundarySide, MechanicalTractionLimits};
mod operators;
mod pressure;
use crate::{
	AssemblyDiagnostics, CfdError,
	simplex::{BoxBoundary, Cell, Face},
};
use basis::{Basis, facet_nodes, quadrature};
use chart::{complete_chart, mass_apply};
pub use pressure::{GeneralPressureRecovery, PhysicalPressureRecovery, PressureNormalization};
type Point = [f64; 3];
fn dot(a: &[f64], b: &[f64]) -> f64 {
	a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn reserved<T>(capacity: usize) -> Result<Vec<T>, CfdError> {
	let mut output = Vec::new();
	output
		.try_reserve_exact(capacity)
		.map_err(|_| CfdError::InvalidInput("physical-space allocation failed"))?;
	Ok(output)
}
#[derive(Clone, Debug)]
struct VolumeSample {
	bary: Vec<f64>,
	point: Point,
	weight: f64,
	values: Vec<f64>,
	gradients: Vec<Point>,
}
#[derive(Clone, Debug)]
struct FacetSample {
	weight: f64,
	left_bary: Vec<f64>,
	left_values: Vec<f64>,
	right_values: Vec<f64>,
	left_gradients: Vec<Point>,
	right_gradients: Vec<Point>,
	prescribed: Point,
}
#[derive(Clone, Debug)]
struct FaceLayout {
	normal_start: Option<usize>,
	dirichlet: Option<usize>,
	natural: Option<usize>,
}
/// Full homogeneous constraint chart of the broken vector polynomial space.
#[derive(Clone, Debug)]
pub struct PhysicalSpace {
	dimension: usize,
	order: usize,
	viscosity: f64,
	boundary: BoxBoundary,
	mesh_metadata: Option<mesh::MeshMetadata>,
	cells: Vec<Cell>,
	faces: Vec<Face>,
	face_layout: Vec<FaceLayout>,
	basis: Basis,
	volume: Vec<Vec<VolumeSample>>,
	facets: Vec<Vec<FacetSample>>,
	constraints: Vec<Vec<f64>>,
	normal_count: usize,
	chart: Vec<Vec<f64>>,
	diagnostics: AssemblyDiagnostics,
	sip: Vec<Vec<f64>>,
	boundary_force: Vec<f64>,
}
fn facet_bary(nodes: &[usize], coordinates: &[f64], dimension: usize) -> Vec<f64> {
	let mut bary = vec![0.; dimension + 1];
	for (&node, &value) in nodes.iter().zip(coordinates) {
		bary[node] = value;
	}
	bary
}
impl PhysicalSpace {
	/// Complete bounded broken momentum force, before mass inversion or chart projection.
	///
	/// This reference uses a unique face's left normal velocity for convection.
	/// Its agreement with an averaged-normal trace requires conformity; arbitrary
	/// broken inputs retain the original reference extension rather than projection.
	/// # Errors
	/// Rejects malformed/nonfinite full broken coefficients or force overflow.
	pub fn momentum_force(&self, coefficients: &[f64]) -> Result<Vec<f64>, CfdError> {
		if coefficients.len() != self.diagnostics.local_velocity_dimension
			|| coefficients.iter().any(|v| !v.is_finite())
		{
			return Err(CfdError::InvalidInput(
				"invalid complete broken momentum state",
			));
		}
		self.force(coefficients, false)
	}
	/// Assemble the complete physical box space at the requested degree.
	/// # Errors
	/// Rejects unsupported degree, mixed geometry, rank ambiguity and oversized dense references.
	#[allow(
		clippy::too_many_lines,
		reason = "Keep the bounded physical assembly and its complete-rank certification together"
	)]
	pub fn box_mesh(
		dimension: usize,
		subdivisions: u32,
		extent: f64,
		viscosity: f64,
		boundary: BoxBoundary,
		order: usize,
	) -> Result<Self, CfdError> {
		if ![2, 3].contains(&dimension)
			|| ![1, 2].contains(&order)
			|| subdivisions == 0
			|| !extent.is_finite()
			|| extent <= 0.
			|| !viscosity.is_finite()
			|| viscosity < 0.
			|| boundary == BoxBoundary::Mixed
		{
			return Err(CfdError::InvalidInput(
				"invalid complete physical box parameters",
			));
		}
		if let BoxBoundary::Cavity { lid_speed } = boundary
			&& !lid_speed.is_finite()
		{
			return Err(CfdError::InvalidInput("nonfinite cavity lid"));
		}
		let simplex_count = usize::try_from(subdivisions)
			.ok()
			.and_then(|n| n.checked_pow(u32::try_from(dimension).ok()?))
			.and_then(|n| n.checked_mul(if dimension == 2 { 2 } else { 6 }))
			.ok_or(CfdError::InvalidInput("physical mesh count overflow"))?;
		let scalar = if order == 1 {
			dimension + 1
		} else {
			(dimension + 1) * (dimension + 2) / 2
		};
		let total = simplex_count
			.checked_mul(dimension)
			.and_then(|n| n.checked_mul(scalar))
			.ok_or(CfdError::InvalidInput("physical dimension overflow"))?;
		if total > 768 {
			return Err(CfdError::Unsupported(
				"bounded complete physical chart limited to 768 broken velocity coefficients"
					.into(),
			));
		}
		let (cells, mut faces) = crate::simplex::mesh(dimension, subdivisions, extent, boundary)?;
		if let BoxBoundary::Cavity { lid_speed } = boundary {
			for face in &mut faces {
				if face.lid {
					face.prescribed = Some(vec![[lid_speed, 0., 0.]; dimension]);
				}
			}
		}
		Self::assemble_cells_faces(dimension, order, viscosity, boundary, cells, faces, None)
	}
	#[allow(
		clippy::too_many_arguments,
		clippy::too_many_lines,
		reason = "Shared bounded physical assembly preserves existing box arithmetic and order"
	)]
	fn assemble_cells_faces(
		dimension: usize,
		order: usize,
		viscosity: f64,
		boundary: BoxBoundary,
		cells: Vec<Cell>,
		faces: Vec<Face>,
		mesh_metadata: Option<mesh::MeshMetadata>,
	) -> Result<Self, CfdError> {
		let scalar = if order == 1 {
			dimension + 1
		} else {
			(dimension + 1) * (dimension + 2) / 2
		};
		let total = cells.len() * dimension * scalar;
		let basis = Basis::new(dimension, order)?;
		let mut volume = reserved(cells.len())?;
		for cell in &cells {
			let mut samples = Vec::new();
			for (bary, weight) in quadrature(dimension) {
				let point = std::array::from_fn(|axis| {
					cell.vertices
						.iter()
						.zip(&bary)
						.map(|(v, l)| v[axis] * l)
						.sum()
				});
				let values = basis.values(&bary)?;
				let gradients = basis.gradients(&bary, &cell.gradients)?;
				samples.push(VolumeSample {
					bary,
					point,
					weight: weight * cell.volume,
					values,
					gradients,
				});
			}
			volume.push(samples);
		}
		let mut face_samples = reserved(faces.len())?;
		for face in &faces {
			let mut samples = Vec::new();
			for (bary, weight) in quadrature(dimension - 1) {
				let left = facet_bary(&face.left_nodes, &bary, dimension);
				let right = facet_bary(&face.right_nodes, &bary, dimension);
				let left_values = basis.values(&left)?;
				let left_gradients = basis.gradients(&left, &cells[face.left].gradients)?;
				let (right_values, right_gradients) = if let Some(ci) = face.right {
					(
						basis.values(&right)?,
						basis.gradients(&right, &cells[ci].gradients)?,
					)
				} else {
					(vec![0.; scalar], vec![[0.; 3]; scalar])
				};
				let prescribed = face.prescribed.as_ref().map_or([0.; 3], |values| {
					std::array::from_fn(|axis| {
						values.iter().zip(&bary).map(|(p, l)| p[axis] * l).sum()
					})
				});
				samples.push(FacetSample {
					weight: weight * face.measure,
					left_bary: left,
					left_values,
					right_values,
					left_gradients,
					right_gradients,
					prescribed,
				});
			}
			face_samples.push(samples);
		}
		let local = dimension * scalar;
		let mut constraints = Vec::new();
		for face in &faces {
			if face.outflow {
				continue;
			}
			for bary in facet_nodes(dimension, order) {
				let left = basis.values(&facet_bary(&face.left_nodes, &bary, dimension))?;
				let right = if face.right.is_some() {
					basis.values(&facet_bary(&face.right_nodes, &bary, dimension))?
				} else {
					vec![0.; scalar]
				};
				let mut row = vec![0.; total];
				for component in 0..dimension {
					for node in 0..scalar {
						row[face.left * local + component * scalar + node] =
							face.normal[component] * left[node];
						if let Some(ci) = face.right {
							row[ci * local + component * scalar + node] -=
								face.normal[component] * right[node];
						}
					}
				}
				constraints.push(row);
			}
		}
		let normal_count = constraints.len();
		let mut face_layout = reserved(faces.len())?;
		let trace_width = basis::facet_nodes(dimension, order).len();
		let (mut normal, mut dirichlet, mut natural) = (0, 0, 0);
		for face in &faces {
			let normal_start = if face.outflow {
				None
			} else {
				let start = normal;
				normal += trace_width;
				Some(start)
			};
			let prescribed = if face.right.is_none() && !face.outflow {
				let i = dirichlet;
				dirichlet += 1;
				Some(i)
			} else {
				None
			};
			let traction = if face.outflow {
				let i = natural;
				natural += 1;
				Some(i)
			} else {
				None
			};
			face_layout.push(FaceLayout {
				normal_start,
				dirichlet: prescribed,
				natural: traction,
			});
		}
		if normal != normal_count {
			return Err(CfdError::Assembly("physical trace layout mismatch"));
		}
		let pressure_modes = if order == 1 { 1 } else { dimension + 1 };
		for (ci, samples) in volume.iter().enumerate() {
			for pressure in 0..pressure_modes {
				let mut row = vec![0.; total];
				for sample in samples {
					let value = if order == 1 {
						1.
					} else {
						sample.bary[pressure]
					};
					for component in 0..dimension {
						for node in 0..scalar {
							row[ci * local + component * scalar + node] +=
								sample.weight * value * sample.gradients[node][component];
						}
					}
				}
				constraints.push(row);
			}
		}
		if constraints.iter().flatten().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("physical constraint assembly overflow"));
		}
		let chart = complete_chart(&constraints, &cells, dimension, &basis)?;
		if chart.iter().flatten().any(|v| !v.is_finite()) {
			return Err(CfdError::Assembly("physical chart overflow"));
		}
		let constraint_residual = constraints
			.iter()
			.flat_map(|row| chart.iter().map(move |q| dot(row, q).abs()))
			.fold(0., f64::max);
		let mut mass_orthogonality_residual = 0_f64;
		for (j, q) in chart.iter().enumerate() {
			let mass = mass_apply(&cells, dimension, &basis, q);
			for (i, p) in chart.iter().enumerate() {
				let defect = (dot(p, &mass) - f64::from(i == j)).abs();
				if !defect.is_finite() {
					return Err(CfdError::Assembly("physical mass certificate overflow"));
				}
				mass_orthogonality_residual = mass_orthogonality_residual.max(defect);
			}
		}
		if constraint_residual > 1e-8 || mass_orthogonality_residual > 1e-10 {
			return Err(CfdError::Assembly(
				"complete physical chart certificate failed",
			));
		}
		let diagnostics = AssemblyDiagnostics {
			local_velocity_dimension: total,
			constraint_rank: total - chart.len(),
			independent_dimension: chart.len(),
			constraint_residual,
			mass_orthogonality_residual,
		};
		let mut space = Self {
			dimension,
			order,
			viscosity,
			boundary,
			mesh_metadata,
			cells,
			faces,
			face_layout,
			basis,
			volume,
			facets: face_samples,
			constraints,
			normal_count,
			chart,
			diagnostics,
			sip: Vec::new(),
			boundary_force: Vec::new(),
		};
		let (sip, boundary_force) = space.assemble_viscosity();
		if sip
			.iter()
			.flatten()
			.chain(&boundary_force)
			.any(|v| !v.is_finite())
		{
			return Err(CfdError::Assembly("physical SIP assembly overflow"));
		}
		space.sip = sip;
		space.boundary_force = boundary_force;
		Ok(space)
	}
	/// Complete independent physical dimension.
	#[must_use]
	pub const fn dimension(&self) -> usize {
		self.chart.len()
	}
	/// Spatial dimension.
	#[must_use]
	pub const fn physical_dimension(&self) -> usize {
		self.dimension
	}
	/// Actual physical polynomial degree.
	#[must_use]
	pub const fn order(&self) -> usize {
		self.order
	}
	/// Complete assembly diagnostics, including actual measured constraint rank.
	#[must_use]
	pub const fn diagnostics(&self) -> &AssemblyDiagnostics {
		&self.diagnostics
	}
	/// Every mass-orthonormal homogeneous chart vector in broken physical coefficients.
	#[must_use]
	pub fn chart(&self) -> &[Vec<f64>] {
		&self.chart
	}
}

impl PhysicalSpace {
	/// Number of affine simplex cells.
	#[must_use]
	pub const fn cell_count(&self) -> usize {
		self.cells.len()
	}
	/// Number of broken vector coefficients per cell, in component-major nodal order.
	#[must_use]
	pub const fn local_velocity_per_cell(&self) -> usize {
		self.dimension * self.basis.nodes.len()
	}
	/// Interior/Dirichlet normal rows followed by all cell divergence rows.
	/// Closed owners retain their pressure dependency; mixed natural owners have full row rank.
	#[must_use]
	pub const fn constraint_count(&self) -> usize {
		self.constraints.len()
	}
	/// Number of pressure coefficients on each cell.
	#[must_use]
	pub const fn pressure_modes_per_cell(&self) -> usize {
		if self.order == 1 {
			1
		} else {
			self.dimension + 1
		}
	}
	/// Boundary condition used in this physical space.
	#[must_use]
	pub const fn boundary(&self) -> BoxBoundary {
		self.boundary
	}
	/// Shared-MathCore exact nodal basis in independent barycentric symbols.
	#[must_use]
	pub fn scalar_basis_polynomials(&self) -> &[mathcore::multivariate::SparsePolynomial] {
		&self.basis.exact
	}
	/// Dense reference cell mass, row-major; production distributed sources must construct locally.
	/// # Errors
	/// Rejects an invalid cell index.
	pub fn cell_mass_matrix(&self, cell: usize) -> Result<Vec<f64>, CfdError> {
		let Some(geometry) = self.cells.get(cell) else {
			return Err(CfdError::InvalidInput("physical mass cell out of bounds"));
		};
		let scalar = self.basis.nodes.len();
		let local = self.local_velocity_per_cell();
		let mut matrix = vec![0.; local * local];
		for component in 0..self.dimension {
			for i in 0..scalar {
				for j in 0..scalar {
					matrix[(component * scalar + i) * local + component * scalar + j] =
						geometry.volume * self.basis.mass[i][j];
				}
			}
		}
		Ok(matrix)
	}
	/// Dense reference C-transpose block, row-major local coefficient by global constraint.
	/// # Errors
	/// Rejects an invalid cell index.
	pub fn cell_constraint_transpose(&self, cell: usize) -> Result<Vec<f64>, CfdError> {
		if cell >= self.cells.len() {
			return Err(CfdError::InvalidInput(
				"physical constraint cell out of bounds",
			));
		}
		let local = self.local_velocity_per_cell();
		let mut block = vec![0.; local * self.constraint_count()];
		for i in 0..local {
			for (j, row) in self.constraints.iter().enumerate() {
				block[i * self.constraint_count() + j] = row[cell * local + i];
			}
		}
		Ok(block)
	}
}
