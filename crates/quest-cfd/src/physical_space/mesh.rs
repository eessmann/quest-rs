//! Bounded complete-coordinate affine mesh owners and phase admission.
#![allow(
	clippy::many_single_char_names,
	reason = "Admission formulas use the documented d,p,C,N,R finite-element counts"
)]
use super::{BoxBoundary, Cell, CfdError, Face, FacetSample, PhysicalSpace, VolumeSample};

const BASIS_BYTES: usize = 80 * 1024 * 1024;
pub(super) const fn invalid() -> CfdError {
	CfdError::InvalidInput("closed affine mesh shape, topology, geometry or admission")
}
pub(super) fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
pub(super) fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
/// Borrowed, explicitly labeled uniform-order affine mesh. Input backing-capacity
/// excess belongs in `external_retained_bytes`; accessible slice bytes are counted.
#[derive(Clone, Copy, Debug)]
pub struct AffineMeshView<'a> {
	pub dimension: usize,
	pub vertices: &'a [[f64; 3]],
	pub cells: &'a [&'a [usize]],
	pub dirichlet: &'a [DirichletFacet<'a>],
	pub periodic: &'a [TranslationalPeriodicFacet<'a>],
}
/// Exterior homogeneous Dirichlet facet; the label is retained verbatim.
#[derive(Clone, Copy, Debug)]
pub struct DirichletFacet<'a> {
	pub vertices: &'a [usize],
	pub label: &'a str,
}
/// Ordered vertex correspondence for opposite sides of an axis-aligned box.
#[derive(Clone, Copy, Debug)]
pub struct TranslationalPeriodicFacet<'a> {
	pub left: &'a [usize],
	pub right: &'a [usize],
}
/// Geometry provenance; arbitrary affine meshes are never implicit box selectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum PhysicalGeometryKind {
	Box,
	ClosedAffineMesh,
	MixedAffineMesh,
}
/// Separate fixed source, exact-geometry and original-pressure query ceilings.
///
/// Managed payload/work admission assumes the supported immutable `MathCore`
/// host arithmetic profile. Environment discovery is outside this model: an
/// oversized unsupported host value can allocate before being rejected. These
/// limits are not an operating-system process-memory limit.
#[derive(Clone, Copy, Debug)]
pub struct PhysicalMeshLimits {
	pub max_vertices: usize,
	pub max_cells: usize,
	pub max_facets: usize,
	pub max_label_bytes: usize,
	pub max_input_bytes: usize,
	pub external_retained_bytes: usize,
	pub max_bytes: usize,
	pub max_work: usize,
	/// Per-predicate `MathCore` arithmetic payload ceiling. The whole geometry
	/// phase (input, external owner, facet plan and conservative stack allowance)
	/// is reported by `geometry_peak_bytes` and checked against `max_bytes`.
	pub max_geometry_bytes: usize,
	pub max_geometry_work: usize,
	pub max_coordinate_bits: usize,
	pub max_coefficient_bits: usize,
	pub minimum_scaled_quality: f64,
	pub max_pressure_bytes: usize,
	pub max_pressure_work: usize,
}
impl Default for PhysicalMeshLimits {
	fn default() -> Self {
		Self {
			max_vertices: 512,
			max_cells: 128,
			max_facets: 512,
			max_label_bytes: 32768,
			max_input_bytes: 1024 * 1024,
			external_retained_bytes: 0,
			max_bytes: 256 * 1024 * 1024,
			max_work: 1_000_000_000,
			max_geometry_bytes: 8 * 1024 * 1024,
			max_geometry_work: 1_000_000_000,
			max_coordinate_bits: 64,
			max_coefficient_bits: 4096,
			minimum_scaled_quality: 1e-10,
			max_pressure_bytes: 256 * 1024 * 1024,
			max_pressure_work: 1_000_000_000,
		}
	}
}
/// Conservative phase allowances, actual Vec/String capacities and admitted `MathCore` payloads. These are
/// managed arithmetic/payload models, not measured RSS or processor instructions.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PhysicalMeshResources {
	pub vertices: usize,
	pub cells: usize,
	pub incidences: usize,
	pub logical_facets: usize,
	pub constraint_rows: usize,
	pub broken_dimension: usize,
	pub input_bytes: usize,
	pub external_retained_bytes: usize,
	pub geometry_work: usize,
	pub geometry_peak_bytes: usize,
	pub construction_work: usize,
	pub constructor_peak_bytes: usize,
	pub retained_bytes: Option<usize>,
	pub constraint_rank: Option<usize>,
	pub independent_dimension: Option<usize>,
	pub minimum_quality_lower: Option<f64>,
	/// Framed fixed-u64 geometry-only provenance; excludes viscosity and order.
	pub source_identity: u64,
	pub completed_phase: &'static str,
}
/// Pressure has its own live-source/query envelope, without the basis reserve.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PhysicalMeshQueryResources {
	pub source_retained_bytes: usize,
	pub external_retained_bytes: usize,
	pub additional_owner_bytes: usize,
	pub scratch_bytes: usize,
	pub peak_bytes: usize,
	pub work: usize,
}
/// Optional attempt receipt, including the admitted envelope on later rejection.
#[derive(Debug)]
pub struct PhysicalMeshAttempt {
	pub outcome: Result<PhysicalSpace, CfdError>,
	pub resources: Option<PhysicalMeshResources>,
}
#[derive(Clone, Debug)]
pub(super) enum ClosedTopologyProof {
	DirichletManifold,
	PeriodicBox,
	MixedManifold,
}
#[derive(Clone, Debug)]
pub(super) struct MeshMetadata {
	pub proof: ClosedTopologyProof,
	pub limits: PhysicalMeshLimits,
	pub resources: PhysicalMeshResources,
}
#[derive(Clone, Copy)]
pub(super) struct Counts {
	pub d: usize,
	pub p: usize,
	pub scalar: usize,
	pub local: usize,
	pub n: usize,
	pub i: usize,
	pub input: usize,
}
impl Counts {
	pub fn new(
		view: AffineMeshView<'_>,
		viscosity: f64,
		p: usize,
		l: PhysicalMeshLimits,
	) -> Result<Self, CfdError> {
		let d = view.dimension;
		if ![2, 3].contains(&d)
			|| ![1, 2].contains(&p)
			|| !viscosity.is_finite()
			|| viscosity < 0.
			|| view.cells.is_empty()
			|| view.vertices.is_empty()
			|| view.cells.len() > l.max_cells.min(128)
			|| view.vertices.len() > l.max_vertices.min(512)
			|| l.max_work == 0
			|| !l.minimum_scaled_quality.is_finite()
			|| l.minimum_scaled_quality <= 0.
			|| l.minimum_scaled_quality >= 1.
			|| (!view.dirichlet.is_empty() && !view.periodic.is_empty())
		{
			return Err(invalid());
		}
		let scalar = if p == 1 { d + 1 } else { (d + 1) * (d + 2) / 2 };
		let local = mul(d, scalar)?;
		let n = mul(view.cells.len(), local)?;
		let i = mul(view.cells.len(), d + 1)?;
		if n > 768 || i > 512 || view.dirichlet.len() > 512 || view.periodic.len() > 256 {
			return Err(invalid());
		}
		let mut input = add(
			mul(view.vertices.len(), size_of::<[f64; 3]>())?,
			mul(view.cells.len(), size_of::<&[usize]>())?,
		)?;
		input = add(
			input,
			mul(view.dirichlet.len(), size_of::<DirichletFacet<'_>>())?,
		)?;
		input = add(
			input,
			mul(
				view.periodic.len(),
				size_of::<TranslationalPeriodicFacet<'_>>(),
			)?,
		)?;
		let mut labels = 0;
		for cell in view.cells {
			if cell.len() != d + 1 {
				return Err(invalid());
			}
			input = add(input, mul(cell.len(), size_of::<usize>())?)?;
		}
		for f in view.dirichlet {
			if f.vertices.len() != d || f.label.is_empty() || f.label.len() > 64 {
				return Err(invalid());
			}
			labels = add(labels, f.label.len())?;
			input = add(input, add(mul(d, size_of::<usize>())?, f.label.len())?)?;
		}
		for f in view.periodic {
			if f.left.len() != d || f.right.len() != d {
				return Err(invalid());
			}
			input = add(input, mul(2 * d, size_of::<usize>())?)?;
		}
		if input > l.max_input_bytes
			|| labels > l.max_label_bytes
			|| add(add(input, l.external_retained_bytes)?, BASIS_BYTES)? > l.max_bytes
		{
			return Err(invalid());
		}
		let count = Self {
			d,
			p,
			scalar,
			local,
			n,
			i,
			input,
		};
		// The byte preflight above reserves 80 MiB, exceeding the 128 KiB fixed
		// nested topology frames that execute before basis construction. These
		// are sequential reservations, not an assertion they are simultaneously live.
		// The shared bounded basis phase alone has this declared arithmetic allowance.
		if l.max_work < 160 * 1024 * 1024 {
			return Err(invalid());
		}
		Ok(count)
	}
	pub fn receipt(
		self,
		view: AffineMeshView<'_>,
		f: usize,
		l: PhysicalMeshLimits,
		natural: usize,
	) -> Result<PhysicalMeshResources, CfdError> {
		let c = view.cells.len();
		let q = if self.p == 1 { 1 } else { self.d + 1 };
		let t = if self.p == 1 {
			self.d
		} else {
			self.d * (self.d + 1) / 2
		};
		let r = add(
			mul(f.checked_sub(natural).ok_or_else(invalid)?, t)?,
			mul(c, q)?,
		)?;
		let volume = mul(c, 4usize.pow(u32::try_from(self.d).map_err(|_| invalid())?))?;
		let face = mul(
			f,
			4usize.pow(u32::try_from(self.d - 1).map_err(|_| invalid())?),
		)?;
		let samples = add(volume, face)?;
		let mut work = 160 * 1024 * 1024 + 1_000_000;
		for w in [
			mul(64, mul(self.i, self.i)?)?,
			mul(128, mul(self.i, mul(c, c)?)?)?,
			mul(32, mul(mul(r, r)?, self.n)?)?,
			mul(12, mul(mul(self.n, self.n)?, self.n)?)?,
			mul(256, mul(samples, mul(self.local, self.local)?)?)?,
			mul(64, mul(r, self.n)?)?,
			mul(64, mul(mul(self.n, self.n)?, self.scalar)?)?,
		] {
			work = add(work, w)?;
		}
		let geometry = add(
			mul(f, size_of::<super::FaceLayout>())?,
			add(
				mul(c, add(size_of::<Cell>(), mul(60, self.d + 1)?)?)?,
				mul(f, add(size_of::<Face>(), add(mul(40, self.d)?, 64)?)?)?,
			)?,
		)?;
		let tables = add(
			mul(
				volume,
				add(
					size_of::<VolumeSample>(),
					add(mul(8, self.d + 1)?, mul(32, self.scalar)?)?,
				)?,
			)?,
			mul(
				face,
				add(
					size_of::<FacetSample>(),
					add(mul(8, self.d + 1)?, mul(64, self.scalar)?)?,
				)?,
			)?,
		)?;
		// Header slack includes geometric plans, sample rows and every chart stage.
		let headers = mul(
			size_of::<Vec<f64>>(),
			add(mul(4, r)?, add(mul(8, self.n)?, mul(8, add(c, f)?)?)?)?,
		)?;
		let dense = mul(
			8,
			add(
				mul(3, mul(r, self.n)?)?,
				add(mul(4, mul(self.n, self.n)?)?, mul(32, self.n)?)?,
			)?,
		)?;
		let peak = add(
			add(self.input, l.external_retained_bytes)?,
			add(
				BASIS_BYTES,
				add(
					131_072,
					add(
						mul(self.i, 128)?,
						add(geometry, add(tables, add(headers, dense)?)?)?,
					)?,
				)?,
			)?,
		)?;
		Ok(PhysicalMeshResources {
			vertices: view.vertices.len(),
			cells: c,
			incidences: self.i,
			logical_facets: f,
			constraint_rows: r,
			broken_dimension: self.n,
			input_bytes: self.input,
			external_retained_bytes: l.external_retained_bytes,
			geometry_work: 0,
			geometry_peak_bytes: 0,
			construction_work: work,
			constructor_peak_bytes: peak,
			retained_bytes: None,
			constraint_rank: None,
			independent_dimension: None,
			minimum_quality_lower: None,
			source_identity: 0,
			completed_phase: "topology",
		})
	}
}
impl PhysicalSpace {
	/// Construct a complete bounded closed affine source. Mixed closure and general
	/// periodic seams are unsupported; periodic meshes must tile an axis-aligned box.
	/// # Errors
	/// Rejects malformed/nonmanifold/intersecting geometry and any declared ceiling.
	pub fn from_mesh(
		view: AffineMeshView<'_>,
		viscosity: f64,
		order: usize,
		limits: PhysicalMeshLimits,
	) -> Result<Self, CfdError> {
		Self::from_mesh_with_receipt(view, viscosity, order, limits).outcome
	}
	/// Same constructor retaining the latest attempted/planned phase and available
	/// numerical evidence on failure; the phase distinguishes plans from admission.
	#[must_use]
	pub fn from_mesh_with_receipt(
		view: AffineMeshView<'_>,
		viscosity: f64,
		order: usize,
		limits: PhysicalMeshLimits,
	) -> PhysicalMeshAttempt {
		Self::from_mesh_parts(view, viscosity, order, limits, &[], 0, 0)
	}
	#[allow(
		clippy::too_many_arguments,
		reason = "One admitted construction pipeline serves closed and typed mixed views"
	)]
	pub(super) fn from_mesh_parts(
		view: AffineMeshView<'_>,
		viscosity: f64,
		order: usize,
		limits: PhysicalMeshLimits,
		natural: &[bool],
		extra_input: usize,
		adapter_bytes: usize,
	) -> PhysicalMeshAttempt {
		let mut receipt = None;
		let outcome = (|| {
			let mut count = Counts::new(view, viscosity, order, limits)?;
			count.input = add(count.input, extra_input)?;
			if count.input > limits.max_input_bytes
				|| add(
					add(
						add(count.input, limits.external_retained_bytes)?,
						BASIS_BYTES,
					)?,
					adapter_bytes,
				)? > limits.max_bytes
			{
				return Err(invalid());
			}
			let mut plan = super::mesh_topology::validate(view, count, limits)?;
			for face in &mut plan {
				face.natural = face
					.label
					.and_then(|i| natural.get(i))
					.copied()
					.unwrap_or(false);
			}
			let mut r = count.receipt(
				view,
				plan.len(),
				limits,
				plan.iter().filter(|f| f.natural).count(),
			)?;
			r.constructor_peak_bytes = add(r.constructor_peak_bytes, adapter_bytes)?;
			receipt = Some(r.clone());
			if r.construction_work > limits.max_work || r.constructor_peak_bytes > limits.max_bytes
			{
				return Err(invalid());
			}
			let geometry =
				super::mesh_geometry::validate(view, count, &plan, limits, &mut r, adapter_bytes);
			receipt = Some(r.clone());
			geometry?;
			let (cells, faces) = super::mesh_topology::materialize(view, count, &plan)?;
			let metadata = MeshMetadata {
				proof: if !natural.is_empty() {
					ClosedTopologyProof::MixedManifold
				} else if view.periodic.is_empty() {
					ClosedTopologyProof::DirichletManifold
				} else {
					ClosedTopologyProof::PeriodicBox
				},
				limits,
				resources: r.clone(),
			};
			let mut space = Self::assemble_cells_faces(
				count.d,
				order,
				viscosity,
				BoxBoundary::Mixed,
				cells,
				faces,
				Some(metadata),
			)?;
			if !natural.is_empty() && space.diagnostics.constraint_rank != space.constraints.len() {
				return Err(CfdError::Assembly(
					"mixed traction requires independent complete constraint rows",
				));
			}
			let retained = space.retained_bytes()?;
			r.retained_bytes = Some(retained);
			r.constraint_rank = Some(space.diagnostics.constraint_rank);
			r.independent_dimension = Some(space.dimension());
			r.completed_phase = "assembly evaluated";
			receipt = Some(r.clone());
			if add(add(retained, count.input)?, limits.external_retained_bytes)?
				> r.constructor_peak_bytes
			{
				return Err(invalid());
			}
			r.completed_phase = "complete";
			if let Some(meta) = &mut space.mesh_metadata {
				meta.resources = r.clone();
			}
			receipt = Some(r);
			Ok(space)
		})();
		PhysicalMeshAttempt {
			outcome,
			resources: receipt,
		}
	}
	/// Explicit provenance for geometry-specific physical observation selectors.
	#[must_use]
	pub const fn geometry_kind(&self) -> PhysicalGeometryKind {
		if matches!(&self.mesh_metadata,Some(m) if matches!(m.proof,ClosedTopologyProof::MixedManifold))
		{
			PhysicalGeometryKind::MixedAffineMesh
		} else if self.mesh_metadata.is_some() {
			PhysicalGeometryKind::ClosedAffineMesh
		} else {
			PhysicalGeometryKind::Box
		}
	}
	/// Successful actual owner receipt, absent for the historical box constructor.
	#[must_use]
	pub fn mesh_resources(&self) -> Option<&PhysicalMeshResources> {
		self.mesh_metadata.as_ref().map(|m| &m.resources)
	}
	pub(super) fn has_supported_boundary(&self) -> bool {
		self.has_closed_proof() || self.geometry_kind() == PhysicalGeometryKind::MixedAffineMesh
	}
	pub(super) fn has_closed_proof(&self) -> bool {
		self.mesh_metadata.as_ref().is_some_and(|m| {
			matches!(
				m.proof,
				ClosedTopologyProof::DirichletManifold | ClosedTopologyProof::PeriodicBox
			)
		}) || self.boundary != BoxBoundary::Mixed
	}
	pub(super) fn admit_mesh_pressure(
		&self,
	) -> Result<Option<PhysicalMeshQueryResources>, CfdError> {
		let Some(meta) = &self.mesh_metadata else {
			return Ok(None);
		};
		let n = self.diagnostics.local_velocity_dimension;
		let r = self.constraints.len();
		let samples = add(
			self.volume.iter().map(Vec::len).sum(),
			self.facets.iter().map(Vec::len).sum(),
		)?;
		let drift_work = add(
			mul(64, mul(n, n)?)?,
			mul(512, mul(samples, self.local_velocity_per_cell())?)?,
		)?;
		let work = add(mul(128, mul(mul(r, r)?, n)?)?, mul(2, drift_work)?)?;
		let words = add(
			add(mul(2, mul(r, n)?)?, mul(r, r)?)?,
			add(mul(12, n)?, mul(8, r)?)?,
		)?;
		let scratch = add(
			mul(8, words)?,
			add(
				mul(size_of::<Vec<f64>>(), add(mul(4, r)?, mul(4, n)?)?)?,
				4096,
			)?,
		)?;
		let source = self.retained_bytes()?;
		let peak = add(add(source, meta.limits.external_retained_bytes)?, scratch)?;
		if work > meta.limits.max_pressure_work || peak > meta.limits.max_pressure_bytes {
			return Err(invalid());
		}
		Ok(Some(PhysicalMeshQueryResources {
			source_retained_bytes: source,
			external_retained_bytes: meta.limits.external_retained_bytes,
			additional_owner_bytes: 0,
			scratch_bytes: scratch,
			peak_bytes: peak,
			work,
		}))
	}
	pub(super) fn admit_mesh_pressure_extra(
		&self,
		owner_bytes: usize,
		scratch_bytes: usize,
		extra_work: usize,
	) -> Result<Option<PhysicalMeshQueryResources>, CfdError> {
		let Some(mut receipt) = self.admit_mesh_pressure()? else {
			return Ok(None);
		};
		let metadata = self.mesh_metadata.as_ref().ok_or_else(invalid)?;
		receipt.additional_owner_bytes = owner_bytes;
		receipt.scratch_bytes = add(receipt.scratch_bytes, scratch_bytes)?;
		receipt.peak_bytes = add(receipt.peak_bytes, add(owner_bytes, scratch_bytes)?)?;
		receipt.work = add(receipt.work, extra_work)?;
		if receipt.work > metadata.limits.max_pressure_work
			|| receipt.peak_bytes > metadata.limits.max_pressure_bytes
		{
			return Err(invalid());
		}
		Ok(Some(receipt))
	}
}
