//! Typed, nonperiodic mixed closure over the shared admitted affine pipeline.
use super::mesh::{add, invalid, mul};
use super::{
	AffineMeshView, DirichletFacet, PhysicalMeshAttempt, PhysicalMeshLimits, PhysicalSpace,
};
use crate::CfdError;
/// Natural data are `nu grad(u)n - p n`, with outward fluid-domain normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExteriorCondition {
	Dirichlet,
	NaturalMechanicalTraction,
}
/// Explicit exterior kind and stable label; no values are inferred from absence.
#[derive(Clone, Copy, Debug)]
pub struct ExteriorFacet<'a> {
	pub vertices: &'a [usize],
	pub label: &'a str,
	pub condition: ExteriorCondition,
}
/// Connected nonperiodic affine manifold with both boundary kinds present.
#[derive(Clone, Copy, Debug)]
pub struct MixedAffineMeshView<'a> {
	pub dimension: usize,
	pub vertices: &'a [[f64; 3]],
	pub cells: &'a [&'a [usize]],
	pub exterior: &'a [ExteriorFacet<'a>],
}
/// Admission for the fixed borrowed adapter, in addition to source slice accounting.
pub(super) const ADAPTER_BYTES: usize = 32 * 1024;
impl PhysicalSpace {
	/// Assemble all velocity coordinates for an explicit Dirichlet/traction closure.
	/// # Errors
	/// Rejects unsupported shape/topology, rank ambiguity, geometry or declared limits.
	pub fn from_mixed_mesh(
		view: MixedAffineMeshView<'_>,
		viscosity: f64,
		order: usize,
		limits: PhysicalMeshLimits,
	) -> Result<Self, CfdError> {
		Self::from_mixed_mesh_with_receipt(view, viscosity, order, limits).outcome
	}
	/// Preserve attempted phase evidence when a later construction stage rejects.
	#[must_use]
	pub fn from_mixed_mesh_with_receipt(
		view: MixedAffineMeshView<'_>,
		viscosity: f64,
		order: usize,
		limits: PhysicalMeshLimits,
	) -> PhysicalMeshAttempt {
		let prepare = || -> Result<usize, CfdError> {
			if view.exterior.len() > 512
				|| !view
					.exterior
					.iter()
					.any(|f| f.condition == ExteriorCondition::Dirichlet)
				|| !view
					.exterior
					.iter()
					.any(|f| f.condition == ExteriorCondition::NaturalMechanicalTraction)
			{
				return Err(invalid());
			}
			let extra = mul(
				view.exterior.len(),
				size_of::<ExteriorFacet<'_>>()
					.checked_sub(size_of::<DirichletFacet<'_>>())
					.ok_or_else(invalid)?,
			)?;
			if add(
				add(80 * 1024 * 1024, ADAPTER_BYTES)?,
				limits.external_retained_bytes,
			)? > limits.max_bytes
			{
				return Err(invalid());
			}
			Ok(extra)
		};
		let extra = match prepare() {
			Ok(n) => n,
			Err(e) => {
				return PhysicalMeshAttempt {
					outcome: Err(e),
					resources: None,
				};
			}
		};
		let mut facets = [DirichletFacet {
			vertices: &[],
			label: "",
		}; 512];
		let mut natural = [false; 512];
		for (i, f) in view.exterior.iter().enumerate() {
			facets[i] = DirichletFacet {
				vertices: f.vertices,
				label: f.label,
			};
			natural[i] = f.condition == ExteriorCondition::NaturalMechanicalTraction;
		}
		Self::from_mesh_parts(
			AffineMeshView {
				dimension: view.dimension,
				vertices: view.vertices,
				cells: view.cells,
				dirichlet: &facets[..view.exterior.len()],
				periodic: &[],
			},
			viscosity,
			order,
			limits,
			&natural[..view.exterior.len()],
			extra,
			ADAPTER_BYTES,
		)
	}
}
