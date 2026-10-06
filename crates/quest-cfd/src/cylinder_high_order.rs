//! Bounded P2 DFG2D2 supplied-state snapshots; no evolution or developed-cycle claim.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Validated finite P2 source dimensions and explicit checked workflow admission bound every indexed operation"
)]
#[path = "cylinder_evolution.rs"]
mod evolution;
pub use crate::reference_rk4::Rk4Progress;
use crate::{
	CfdError,
	cases::{CaseManifest, manifest},
	cylinder::{ExplicitGeometryEvidence, PlanarMesh, explicit_planar},
	physical_space::{
		BoundaryLimits, BoundaryTimeCoefficient, CanonicalLiftingLimits, ExteriorCondition,
		ExteriorFacet, GeneralPressureRecovery, MechanicalTractionLimits, MixedAffineMeshView,
		PhysicalMeshLimits, PhysicalSpace, PolynomialBoundary,
	},
};
pub use evolution::{
	CylinderEnergySample, CylinderEvolutionAttempt, CylinderEvolutionReport,
	CylinderEvolutionRequest, CylinderInitialCondition,
};
use serde::{
	Serialize,
	ser::{SerializeSeq, SerializeStruct},
};
const SOURCE_RESERVE: usize = 128 * 1024;
const fn invalid() -> CfdError {
	CfdError::InvalidInput("bounded P2 cylinder workflow admission")
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or_else(invalid)
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or_else(invalid)
}
/// Component caps are never raised to satisfy this aggregate workflow allowance.
#[derive(Clone, Copy, Debug)]
pub struct CylinderPhysicalLimits {
	pub max_work: usize,
	pub max_bytes: usize,
	pub mesh: PhysicalMeshLimits,
	pub lifting: CanonicalLiftingLimits,
	pub boundary: BoundaryLimits,
}
impl Default for CylinderPhysicalLimits {
	fn default() -> Self {
		Self {
			max_work: 1_000_000_000,
			max_bytes: 256 * 1024 * 1024,
			mesh: PhysicalMeshLimits::default(),
			lifting: CanonicalLiftingLimits::default(),
			boundary: BoundaryLimits::default(),
		}
	}
}
/// Cumulative conservative arithmetic and managed live-payload envelope.
#[derive(Clone, Debug, Serialize)]
pub struct CylinderWorkflowResources {
	pub cumulative_work: usize,
	pub peak_bytes: usize,
	pub last_stage: &'static str,
	pub attempted_stage_work: Option<usize>,
	pub max_work: usize,
	pub max_bytes: usize,
}
impl CylinderWorkflowResources {
	const fn new(limits: CylinderPhysicalLimits) -> Self {
		Self {
			cumulative_work: 0,
			peak_bytes: 0,
			last_stage: "unstarted",
			attempted_stage_work: None,
			max_work: limits.max_work,
			max_bytes: limits.max_bytes,
		}
	}
	fn remaining(&self) -> Result<usize, CfdError> {
		self.max_work
			.checked_sub(self.cumulative_work)
			.ok_or_else(invalid)
	}
	fn plan(&mut self, stage: &'static str, work: usize, peak: usize) -> Result<(), CfdError> {
		self.last_stage = stage;
		self.attempted_stage_work = Some(work);
		self.peak_bytes = self.peak_bytes.max(peak);
		if work > self.remaining()? || self.peak_bytes > self.max_bytes {
			return Err(invalid());
		}
		Ok(())
	}
	fn complete(&mut self, work: usize) -> Result<(), CfdError> {
		self.cumulative_work = add(self.cumulative_work, work)?;
		if self.cumulative_work > self.max_work {
			return Err(invalid());
		}
		self.attempted_stage_work = None;
		Ok(())
	}
}
/// Partial admission evidence is retained on a later numerical or budget rejection.
#[derive(Debug)]
pub struct CylinderPhysicalAttempt<T> {
	pub outcome: Result<T, CfdError>,
	pub resources: CylinderWorkflowResources,
}
/// Complete versioned geometry and homogeneous mass chart. No reduced modes.
#[derive(Debug)]
pub struct CylinderPhysicalSource {
	model: PhysicalSpace,
	geometry: PlanarMesh,
	evidence: ExplicitGeometryEvidence,
	fingerprint: u64,
	manifest: CaseManifest,
	reynolds: u32,
	limits: CylinderPhysicalLimits,
	resources: CylinderWorkflowResources,
	geometry_owner_bytes: usize,
	volume: f64,
}
/// Prepared full lifting borrows its immutable complete source; repeated snapshots share one work ledger.
#[derive(Debug)]
pub struct PreparedCylinderPhysical<'a> {
	source: &'a CylinderPhysicalSource,
	flow: PolynomialBoundary<'a>,
	initial: Vec<f64>,
	resources: CylinderWorkflowResources,
}
/// Actual represented source export. Fractions reconstructed from these binary64 values
/// certify this geometry, not the rounded numerical constraint assembly.
pub struct CylinderGeometryExport<'a> {
	source: &'a CylinderPhysicalSource,
}
struct ExteriorExport<'a>(&'a [crate::simplex::BoundaryFacet]);
impl Serialize for ExteriorExport<'_> {
	fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
		let mut seq = s.serialize_seq(Some(self.0.len()))?;
		for f in self.0 {
			#[derive(Serialize)]
			struct Face<'a> {
				vertices: &'a [usize],
				label: &'a str,
				condition: &'static str,
			}
			seq.serialize_element(&Face {
				vertices: &f.vertices,
				label: &f.label,
				condition: if f.velocity.is_none() {
					"NaturalMechanicalTraction"
				} else {
					"Dirichlet"
				},
			})?;
		}
		seq.end()
	}
}
impl Serialize for CylinderGeometryExport<'_> {
	fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
		let source = self.source;
		let mut out = s.serialize_struct("CylinderGeometryExport", 5)?;
		out.serialize_field("source_policy", source.evidence.source_policy)?;
		out.serialize_field("fingerprint", &source.fingerprint)?;
		out.serialize_field("vertices", &source.geometry.vertices)?;
		out.serialize_field("cells", &source.geometry.cells)?;
		out.serialize_field("exterior", &ExteriorExport(&source.geometry.boundaries))?;
		out.end()
	}
}
/// A supplied complete state at a declared time; no integrator was run.
#[derive(Debug, Serialize)]
pub struct CylinderPhysicalSnapshot {
	pub method: &'static str,
	pub case: &'static str,
	pub time: f64,
	pub reynolds: u32,
	pub independent_dimension: usize,
	pub geometry: ExplicitGeometryEvidence,
	pub geometry_fingerprint: u64,
	pub maximum_geometry_deviation: f64,
	pub pressure: GeneralPressureRecovery,
	pub mean_kinetic_energy: f64,
	pub enstrophy: f64,
	pub cylinder_force: [f64; 3],
	pub drag_coefficient: f64,
	pub lift_coefficient: f64,
	pub pressure_difference: f64,
	pub resources: CylinderWorkflowResources,
}
fn geometry_bytes(g: &PlanarMesh) -> Result<usize, CfdError> {
	let mut bytes = add(size_of::<PlanarMesh>(), mul(g.vertices.capacity(), 24)?)?;
	bytes = add(bytes, mul(g.cells.capacity(), size_of::<Vec<usize>>())?)?;
	for c in &g.cells {
		bytes = add(bytes, mul(c.capacity(), size_of::<usize>())?)?;
	}
	bytes = add(
		bytes,
		mul(
			g.boundaries.capacity(),
			size_of::<crate::simplex::BoundaryFacet>(),
		)?,
	)?;
	for f in &g.boundaries {
		bytes = add(
			bytes,
			add(
				mul(f.vertices.capacity(), size_of::<usize>())?,
				f.label.capacity(),
			)?,
		)?;
		if let Some(v) = &f.velocity {
			bytes = add(bytes, mul(v.capacity(), 24)?)?;
		}
	}
	Ok(bytes)
}
fn manifest_bytes(m: &CaseManifest) -> Result<usize, CfdError> {
	let mut bytes = add(size_of::<CaseManifest>(), mul(m.reynolds.capacity(), 4)?)?;
	for text in [&m.id, &m.geometry, &m.initial_condition, &m.provenance] {
		bytes = add(bytes, text.capacity())?;
	}
	for table in [&m.boundaries, &m.observables] {
		bytes = add(bytes, mul(table.capacity(), size_of::<String>())?)?;
		for text in table {
			bytes = add(bytes, text.capacity())?;
		}
	}
	Ok(bytes)
}
pub(crate) fn fingerprint(g: &PlanarMesh, policy: &str) -> Result<u64, CfdError> {
	fn bytes(h: &mut u64, v: &[u8]) {
		for b in v {
			*h = (*h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3);
		}
	}
	fn word(h: &mut u64, n: usize) -> Result<(), CfdError> {
		bytes(h, &u64::try_from(n).map_err(|_| invalid())?.to_le_bytes());
		Ok(())
	}
	let mut h = 0xcbf2_9ce4_8422_2325;
	bytes(&mut h, b"quest-cylinder-geometry-v1\0");
	word(&mut h, policy.len())?;
	bytes(&mut h, policy.as_bytes());
	word(&mut h, g.vertices.len())?;
	for p in &g.vertices {
		for x in p {
			bytes(&mut h, &x.to_bits().to_le_bytes());
		}
	}
	word(&mut h, g.cells.len())?;
	for c in &g.cells {
		word(&mut h, c.len())?;
		for &i in c {
			word(&mut h, i)?;
		}
	}
	word(&mut h, g.boundaries.len())?;
	for f in &g.boundaries {
		word(&mut h, f.vertices.len())?;
		for &i in &f.vertices {
			word(&mut h, i)?;
		}
		word(&mut h, f.label.len())?;
		bytes(&mut h, f.label.as_bytes());
		bytes(&mut h, &[u8::from(f.velocity.is_none())]);
	}
	Ok(h)
}
impl CylinderPhysicalSource {
	/// Construct only the frozen 2D Re100 polygonal P2 source under aggregate and component caps.
	/// # Errors
	/// Rejects invalid controls/Reynolds, source geometry, full chart or resource admission.
	pub fn new(
		angular: u32,
		layers: u32,
		reynolds: u32,
		limits: CylinderPhysicalLimits,
	) -> Result<Self, CfdError> {
		Self::new_with_receipt(angular, layers, reynolds, limits).outcome
	}
	/// Same construction retaining attempted phase evidence on rejection.
	#[must_use]
	#[allow(
		clippy::too_many_lines,
		reason = "Sequential source and common assembly admissions retain one partial workflow receipt"
	)]
	pub fn new_with_receipt(
		angular: u32,
		layers: u32,
		reynolds: u32,
		limits: CylinderPhysicalLimits,
	) -> CylinderPhysicalAttempt<Self> {
		let mut resources = CylinderWorkflowResources::new(limits);
		let outcome = (|| {
			if !(4..=64).contains(&angular) || layers == 0 || layers > 8 || reynolds != 100 {
				return Err(invalid());
			}
			let sectors = usize::try_from(angular).map_err(|_| invalid())? + 4;
			let radial = usize::try_from(layers).map_err(|_| invalid())?;
			let cells = mul(mul(2, sectors)?, radial)?;
			if mul(cells, 12)? > 768 {
				return Err(invalid());
			}
			let vertices = mul(sectors, add(radial, 1)?)?;
			let work = add(
				1_000_000,
				mul(10_000, add(add(vertices, cells)?, mul(4, sectors)?)?)?,
			)?;
			resources.plan(
				"generator",
				work,
				add(SOURCE_RESERVE, limits.mesh.external_retained_bytes)?,
			)?;
			let manifest = manifest("shedding2d")?;
			let viscosity = manifest.viscosity(reynolds)?;
			let (geometry, evidence) = explicit_planar(angular, layers)?;
			let owner = add(
				add(geometry_bytes(&geometry)?, manifest_bytes(&manifest)?)?,
				add(size_of::<Self>(), size_of::<PreparedCylinderPhysical<'_>>())?,
			)?;
			let adapters = add(
				mul(geometry.cells.len(), size_of::<&[usize]>())?,
				mul(geometry.boundaries.len(), size_of::<ExteriorFacet<'_>>())?,
			)?;
			if add(owner, adapters)? > SOURCE_RESERVE {
				return Err(invalid());
			}
			let fingerprint = fingerprint(&geometry, evidence.source_policy)?;
			let volume = geometry
				.cells
				.iter()
				.map(|c| {
					let a = geometry.vertices[c[0]];
					let b = geometry.vertices[c[1]];
					let v = geometry.vertices[c[2]];
					((b[0] - a[0]) * (v[1] - a[1]) - (b[1] - a[1]) * (v[0] - a[0])).abs() * 0.5
				})
				.sum::<f64>();
			if !volume.is_finite() || volume <= 0. {
				return Err(invalid());
			}
			resources.complete(work)?;
			let refs: Vec<_> = geometry.cells.iter().map(Vec::as_slice).collect();
			let faces: Vec<_> = geometry
				.boundaries
				.iter()
				.map(|f| ExteriorFacet {
					vertices: &f.vertices,
					label: &f.label,
					condition: if f.velocity.is_none() {
						ExteriorCondition::NaturalMechanicalTraction
					} else {
						ExteriorCondition::Dirichlet
					},
				})
				.collect();
			let actual = add(
				owner,
				add(
					mul(refs.capacity(), size_of::<&[usize]>())?,
					mul(faces.capacity(), size_of::<ExteriorFacet<'_>>())?,
				)?,
			)?;
			if actual > SOURCE_RESERVE {
				return Err(invalid());
			}
			let mut mesh_limits = limits.mesh;
			mesh_limits.max_work = mesh_limits.max_work.min(resources.remaining()?);
			mesh_limits.max_bytes = mesh_limits.max_bytes.min(limits.max_bytes);
			mesh_limits.external_retained_bytes =
				add(mesh_limits.external_retained_bytes, SOURCE_RESERVE)?;
			resources.plan(
				"physical construction",
				mesh_limits.max_work,
				mesh_limits.max_bytes,
			)?;
			let attempt = PhysicalSpace::from_mixed_mesh_with_receipt(
				MixedAffineMeshView {
					dimension: 2,
					vertices: &geometry.vertices,
					cells: &refs,
					exterior: &faces,
				},
				viscosity,
				2,
				mesh_limits,
			);
			if let Some(r) = attempt.resources {
				resources.attempted_stage_work = Some(r.construction_work);
				resources.peak_bytes = r.constructor_peak_bytes.max(SOURCE_RESERVE);
			}
			let model = attempt.outcome?;
			resources.complete(
				model
					.mesh_resources()
					.ok_or_else(invalid)?
					.construction_work,
			)?;
			drop(faces);
			drop(refs);
			Ok(Self {
				model,
				geometry,
				evidence,
				fingerprint,
				manifest,
				reynolds,
				limits,
				resources: resources.clone(),
				geometry_owner_bytes: owner,
				volume,
			})
		})();
		CylinderPhysicalAttempt { outcome, resources }
	}
	/// Full chart, never a reduced model.
	#[must_use]
	pub const fn model(&self) -> &PhysicalSpace {
		&self.model
	}
	/// Borrowed exact source data; a caller serializing it must separately bound its output buffer.
	#[must_use]
	pub const fn geometry_export(&self) -> CylinderGeometryExport<'_> {
		CylinderGeometryExport { source: self }
	}
	#[must_use]
	pub const fn resources(&self) -> &CylinderWorkflowResources {
		&self.resources
	}
	/// Prepare the complete stationary inlet lifting; keeps every homogeneous coordinate.
	/// # Errors
	/// Rejects aggregate/component work, storage or original constraint errors.
	pub fn prepare(&self) -> Result<PreparedCylinderPhysical<'_>, CfdError> {
		self.prepare_with_receipt().outcome
	}
	/// Same preparation retaining attempted phase costs on rejection.
	#[must_use]
	pub fn prepare_with_receipt(&self) -> CylinderPhysicalAttempt<PreparedCylinderPhysical<'_>> {
		let mut resources = self.resources.clone();
		let outcome = (|| {
			let setup = mul(self.model.diagnostics().local_velocity_dimension, 10_000)?;
			resources.plan(
				"complete trace setup",
				setup,
				add(self.model.retained_bytes()?, SOURCE_RESERVE)?,
			)?;
			let layouts = self.model.dirichlet_facets()?;
			let n = self.model.diagnostics().local_velocity_dimension;
			let mut coefficient = BoundaryTimeCoefficient {
				lifting: vec![0.; n],
				body_force: vec![0.; n],
				prescribed: layouts
					.iter()
					.map(|f| vec![[0.; 3]; f.nodes.len()])
					.collect(),
			};
			for (values, face) in coefficient.prescribed.iter_mut().zip(&layouts) {
				if self.model.boundary_label(face.face)? == "inlet" {
					for (value, p) in values.iter_mut().zip(&face.nodes) {
						*value = [6. * p[1] * (0.41 - p[1]) / 0.41_f64.powi(2), 0., 0.];
					}
				}
			}
			let traces = vec![std::mem::take(&mut coefficient.prescribed)];
			let transient = add(
				mul(
					add(
						coefficient.lifting.capacity(),
						coefficient.body_force.capacity(),
					)?,
					8,
				)?,
				traces.iter().try_fold(
					mul(traces.capacity(), size_of::<Vec<Vec<[f64; 3]>>>())?,
					|n, mode| {
						mode.iter().try_fold(
							add(n, mul(mode.capacity(), size_of::<Vec<[f64; 3]>>())?)?,
							|n, row| add(n, mul(row.capacity(), 24)?),
						)
					},
				)?,
			)?;
			let layout_bytes = layouts.iter().try_fold(
				mul(
					layouts.capacity(),
					size_of::<crate::physical_space::BoundaryFacetLayout>(),
				)?,
				|bytes, face| add(bytes, mul(face.nodes.capacity(), 24)?),
			)?;
			if add(self.geometry_owner_bytes, add(transient, layout_bytes)?)? > SOURCE_RESERVE {
				return Err(invalid());
			}
			drop(layouts);
			resources.complete(setup)?;
			let mut lifting_limits = self.limits.lifting;
			lifting_limits.max_work = lifting_limits.max_work.min(resources.remaining()?);
			lifting_limits.max_bytes = lifting_limits.max_bytes.min(self.limits.max_bytes);
			let previous_peak = resources.peak_bytes;
			resources.plan(
				"canonical lifting",
				lifting_limits.max_work,
				self.limits.max_bytes,
			)?;
			let mut lifting = self.model.canonical_liftings(&traces, lifting_limits)?;
			resources.peak_bytes = previous_peak.max(lifting.resources.peak_bytes);
			resources.complete(lifting.resources.work)?;
			coefficient.lifting = lifting.coefficients.remove(0);
			coefficient.prescribed = traces.into_iter().next().ok_or_else(invalid)?;
			let mut boundary_limits = self.limits.boundary;
			boundary_limits.max_construction_work = boundary_limits
				.max_construction_work
				.min(resources.remaining()?);
			boundary_limits.max_bytes = boundary_limits.max_bytes.min(self.limits.max_bytes);
			let previous_peak = resources.peak_bytes;
			resources.plan(
				"time source preparation",
				boundary_limits.max_construction_work,
				boundary_limits.max_bytes,
			)?;
			let flow = PolynomialBoundary::new(&self.model, vec![coefficient], boundary_limits)?;
			resources.peak_bytes = previous_peak.max(flow.resources().peak_bytes);
			resources.complete(flow.resources().construction_work)?;
			let initial = vec![0.; self.model.dimension()];
			if add(self.geometry_owner_bytes, mul(initial.capacity(), 8)?)? > SOURCE_RESERVE {
				return Err(invalid());
			}
			Ok(PreparedCylinderPhysical {
				source: self,
				flow,
				initial,
				resources: resources.clone(),
			})
		})();
		CylinderPhysicalAttempt { outcome, resources }
	}
}
impl PreparedCylinderPhysical<'_> {
	#[must_use]
	pub fn initial_state(&self) -> &[f64] {
		&self.initial
	}
	#[must_use]
	pub const fn resources(&self) -> &CylinderWorkflowResources {
		&self.resources
	}
	/// Evaluate an explicitly supplied full state, consuming the shared workflow allowance.
	/// # Errors
	/// Rejects shape/time, aggregate admission, original pressure or finite diagnostics.
	pub fn snapshot_at(
		&mut self,
		state: &[f64],
		time: f64,
	) -> Result<CylinderPhysicalSnapshot, CfdError> {
		if !time.is_finite()
			|| time < self.source.manifest.time_window[0]
			|| time > self.source.manifest.time_window[1]
			|| state.len() != self.source.model.dimension()
			|| state.iter().any(|x| !x.is_finite())
		{
			return Err(invalid());
		}
		let pressure_resources = self.flow.pressure_query_resources()?.ok_or_else(invalid)?;
		self.resources.plan(
			"original pressure",
			pressure_resources.work,
			add(pressure_resources.peak_bytes, mul(state.len(), 8)?)?,
		)?;
		self.resources.complete(pressure_resources.work)?;
		let pressure = self.flow.reconstruct_pressure_general(time, state)?;
		let pressure_bytes = pressure.pressure_coefficients.iter().try_fold(
			mul(
				pressure.pressure_coefficients.capacity(),
				size_of::<Vec<f64>>(),
			)?,
			|n, p| add(n, mul(p.capacity(), 8)?),
		)?;
		let pressure_bytes = add(
			pressure_bytes,
			mul(pressure.normal_multipliers.capacity(), 8)?,
		)?;
		let work = add(
			mul(8, self.flow.resources().drift_work)?,
			mul(self.source.model.cell_count(), 1024)?,
		)?;
		let peak = add(
			self.flow.resources().peak_bytes,
			add(pressure_bytes, add(mul(state.len(), 8)?, SOURCE_RESERVE)?)?,
		)?;
		self.resources.plan("physical observations", work, peak)?;
		self.resources.complete(work)?;
		let cylinder_force = self.flow.boundary_force_on_label_with_limits(
			time,
			state,
			&pressure.pressure_coefficients,
			"cylinder",
			MechanicalTractionLimits::default(),
		)?;
		let mean_kinetic_energy = self.flow.energy(time, state)? / self.source.volume;
		let enstrophy = self.flow.enstrophy(time, state)?;
		let probes = self.source.model.sample_pressures(
			&pressure.pressure_coefficients,
			&[[0.15, 0.2, 0.], [0.25, 0.2, 0.]],
		)?;
		let normalization =
			self.source.manifest.reference_velocity.powi(2) * self.source.manifest.reference_length;
		let drag = 2. * cylinder_force[0] / normalization;
		let lift = 2. * cylinder_force[1] / normalization;
		let difference = probes[0] - probes[1];
		if [mean_kinetic_energy, enstrophy, drag, lift, difference]
			.iter()
			.any(|v| !v.is_finite())
		{
			return Err(invalid());
		}
		self.resources.last_stage = "snapshot complete";
		Ok(CylinderPhysicalSnapshot {
			method: "classical supplied-state P2 snapshot; no time integration",
			case: "shedding2d",
			time,
			reynolds: self.source.reynolds,
			independent_dimension: self.source.model.dimension(),
			geometry: self.source.evidence,
			geometry_fingerprint: self.source.fingerprint,
			maximum_geometry_deviation: self.source.geometry.deviation,
			pressure,
			mean_kinetic_energy,
			enstrophy,
			cylinder_force,
			drag_coefficient: drag,
			lift_coefficient: lift,
			pressure_difference: difference,
			resources: self.resources.clone(),
		})
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		clippy::many_single_char_names,
		reason = "Independent exact polynomial edge integrals test the complete P2 inlet/outlet traces"
	)]
	fn p2_midpoints_and_free_outlet_balance_full_inlet_flux() -> Result<(), CfdError> {
		let source = CylinderPhysicalSource::new(
			4,
			1,
			100,
			CylinderPhysicalLimits {
				max_work: 2_000_000_000,
				..Default::default()
			},
		)?;
		let prepared = source.prepare()?;
		let u = prepared
			.flow
			.coefficients_at(0., prepared.initial_state())?;
		let mut flux = [0.; 2];
		let mut curved = false;
		for face in &source.geometry.boundaries {
			let slot = match face.label.as_str() {
				"inlet" => 0,
				"outlet" => 1,
				_ => continue,
			};
			let (ci, cell) = source
				.geometry
				.cells
				.iter()
				.enumerate()
				.find(|(_, c)| face.vertices.iter().all(|v| c.contains(v)))
				.ok_or_else(invalid)?;
			let i = cell
				.iter()
				.position(|x| *x == face.vertices[0])
				.ok_or_else(invalid)?;
			let j = cell
				.iter()
				.position(|x| *x == face.vertices[1])
				.ok_or_else(invalid)?;
			let edge = [(0, 1), (0, 2), (1, 2)]
				.iter()
				.position(|&(a, b)| a == i.min(j) && b == i.max(j))
				.ok_or_else(invalid)?;
			let start = ci * 12;
			let a = u[start + i];
			let b = u[start + j];
			let middle = u[start + 3 + edge];
			let y0 = source.geometry.vertices[face.vertices[0]][1];
			let y1 = source.geometry.vertices[face.vertices[1]][1];
			let y = y0.midpoint(y1);
			if slot == 0 {
				assert!((middle - 6. * y * (0.41 - y) / 0.41_f64.powi(2)).abs() < 1e-10);
				curved |= (middle - a.midpoint(b)).abs() > 0.01;
			}
			flux[slot] += (y1 - y0).abs() * (a + 4. * middle + b) / 6.;
		}
		assert!(curved);
		assert!((flux[0] - 0.41).abs() < 1e-10);
		assert!((flux[1] - 0.41).abs() < 1e-10);
		Ok(())
	}
}
