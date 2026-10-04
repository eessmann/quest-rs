//! Explicit polygonal cylinder geometry, stationary boundary liftings and full DG references.
//!
//! Straight BDM1 facets approximate the circular boundary. Reports preserve the
//! geometric deviation; these tiny meshes are not benchmark-convergence evidence.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	clippy::manual_midpoint
)]
use crate::{
	CfdError,
	cases::{CaseManifest, manifest},
	simplex::{BoundaryFacet, PeriodicFacets, SimplexBdm, SimplexPressureRecovery},
};

/// Complete cylinder reference with explicit geometry-approximation evidence.
#[derive(Clone, Debug)]
pub struct CylinderReference {
	/// Frozen physical geometry, boundary and Reynolds contract.
	pub manifest: CaseManifest,
	/// Reynolds number admitted by that contract.
	pub reynolds: u32,
	/// Complete BDM1/P0 DG ODE on the declared polygonal mesh.
	pub model: SimplexBdm,
	/// Full projected initial velocity with stationary normal lifting included.
	pub initial_state: Vec<f64>,
	/// Maximum inward radial deviation of a polygonal cylinder facet from the circle.
	pub maximum_geometry_deviation: f64,
	/// Number of circular boundary segments in each cross section.
	pub cylinder_segments: usize,
}

#[derive(Clone, Debug)]
struct PlanarMesh {
	vertices: Vec<[f64; 3]>,
	cells: Vec<Vec<usize>>,
	boundaries: Vec<BoundaryFacet>,
	deviation: f64,
	segments: usize,
}

fn planar(id: &str, angular: u32, layers: u32) -> Result<PlanarMesh, CfdError> {
	if !(4..=64).contains(&angular) || layers == 0 || layers > 8 {
		return Err(CfdError::InvalidInput(
			"cylinder requires 4..64 angular sectors and 1..8 radial layers",
		));
	}
	let (center, radius, bounds) = if id == "shedding2d" {
		([0.2, 0.2], 0.05, [0., 2.2, 0., 0.41])
	} else {
		([0., 0.], 0.5, [-20., 30., -40., 40.])
	};
	let mut angles: Vec<_> = (0..angular)
		.map(|i| std::f64::consts::TAU * f64::from(i) / f64::from(angular))
		.collect();
	for x in [bounds[0], bounds[1]] {
		for y in [bounds[2], bounds[3]] {
			angles.push(f64::atan2(y - center[1], x - center[0]).rem_euclid(std::f64::consts::TAU));
		}
	}
	angles.sort_by(f64::total_cmp);
	angles.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
	let count = angles.len();
	let mut vertices = Vec::new();
	for layer in 0..=layers {
		let t = f64::from(layer) / f64::from(layers);
		for &angle in &angles {
			let direction = [angle.cos(), angle.sin()];
			let rx = if direction[0] > 1e-14 {
				(bounds[1] - center[0]) / direction[0]
			} else if direction[0] < -1e-14 {
				(bounds[0] - center[0]) / direction[0]
			} else {
				f64::INFINITY
			};
			let ry = if direction[1] > 1e-14 {
				(bounds[3] - center[1]) / direction[1]
			} else if direction[1] < -1e-14 {
				(bounds[2] - center[1]) / direction[1]
			} else {
				f64::INFINITY
			};
			let radial = radius * (1. - t) + rx.min(ry) * t;
			vertices.push([
				center[0] + radial * direction[0],
				center[1] + radial * direction[1],
				0.,
			]);
		}
	}
	let layers =
		usize::try_from(layers).map_err(|_| CfdError::InvalidInput("layer count overflow"))?;
	let mut cells = Vec::new();
	for layer in 0..layers {
		for i in 0..count {
			let next = (i + 1) % count;
			let a = layer * count + i;
			let b = layer * count + next;
			let c = (layer + 1) * count + i;
			let d = (layer + 1) * count + next;
			cells.push(vec![a, c, d]);
			cells.push(vec![a, d, b]);
		}
	}
	let (boundaries, deviation) = planar_boundaries(id, &vertices, &angles, layers, radius, bounds);
	Ok(PlanarMesh {
		vertices,
		cells,
		boundaries,
		deviation,
		segments: count,
	})
}

type ExtrudedMesh = (
	Vec<[f64; 3]>,
	Vec<Vec<usize>>,
	Vec<BoundaryFacet>,
	Vec<PeriodicFacets>,
);

fn extrude(mesh: &PlanarMesh, layers: u32) -> Result<ExtrudedMesh, CfdError> {
	if layers == 0 || layers > 4 {
		return Err(CfdError::InvalidInput("spanwise layers must be 1..4"));
	}
	let count = mesh.vertices.len();
	let mut vertices = Vec::new();
	for layer in 0..=layers {
		for &p in &mesh.vertices {
			vertices.push([p[0], p[1], -2. + 4. * f64::from(layer) / f64::from(layers)]);
		}
	}
	let layers =
		usize::try_from(layers).map_err(|_| CfdError::InvalidInput("span count overflow"))?;
	let mut cells = Vec::new();
	let mut boundaries = Vec::new();
	for layer in 0..layers {
		for triangle in &mesh.cells {
			let mut t = triangle.clone();
			t.sort_unstable();
			let [a, b, c] = [
				t[0] + layer * count,
				t[1] + layer * count,
				t[2] + layer * count,
			];
			cells.extend([
				vec![a, b, c, c + count],
				vec![a, b, b + count, c + count],
				vec![a, a + count, b + count, c + count],
			]);
		}
		for edge in &mesh.boundaries {
			let mut e = edge.vertices.clone();
			e.sort_unstable();
			let [a, b] = [e[0] + layer * count, e[1] + layer * count];
			for facet in [vec![a, b, b + count], vec![a, a + count, b + count]] {
				let velocity = edge.velocity.as_ref().map(|values| {
					facet
						.iter()
						.map(|&id| {
							if id % count == edge.vertices[0] {
								values[0]
							} else {
								values[1]
							}
						})
						.collect()
				});
				boundaries.push(BoundaryFacet {
					vertices: facet,
					velocity,
					label: edge.label.clone(),
				});
			}
		}
	}
	let periodic = mesh
		.cells
		.iter()
		.map(|triangle| PeriodicFacets {
			left: triangle.clone(),
			right: triangle.iter().map(|&v| v + layers * count).collect(),
		})
		.collect();
	Ok((vertices, cells, boundaries, periodic))
}

/// Assemble the full DFG2D2 reference; reject unsupported stationary-cylinder3D boundary dynamics.
/// The circle is explicitly approximated by an inscribed polygon with corner-aligned rays.
///
/// # Errors
/// Rejects mismatched cases/Reynolds numbers, invalid mesh controls, unbudgeted full
/// dimensions or failed boundary/rank certificates. No physical modes are removed.
pub fn reference(
	id: &str,
	reynolds: u32,
	angular_sectors: u32,
	radial_layers: u32,
	span_layers: u32,
) -> Result<CylinderReference, CfdError> {
	if !["shedding2d", "shedding3d"].contains(&id) {
		return Err(CfdError::InvalidInput("not a cylinder case"));
	}
	if id == "shedding3d" {
		return Err(CfdError::Unsupported("approved stationary-cylinder3D requires Neumann far-field and convective outflow; this backend only implements prescribed velocity and natural traction".into()));
	}
	let manifest = manifest(id)?;
	let viscosity = manifest.viscosity(reynolds)?;
	let planar = planar(id, angular_sectors, radial_layers)?;
	if span_layers != 1 {
		return Err(CfdError::InvalidInput("2D cylinder has no span layers"));
	}
	let model = SimplexBdm::from_mesh(
		2,
		&planar.vertices,
		&planar.cells,
		&planar.boundaries,
		&[],
		viscosity,
	)?;
	let initial_state = model.project_velocity(|_| [0.; 3])?;

	Ok(CylinderReference {
		manifest,
		reynolds,
		model,
		initial_state,
		maximum_geometry_deviation: planar.deviation,
		cylinder_segments: planar.segments,
	})
}

/// A classical reference sample, with mesh geometry and traction conventions explicit.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CylinderSnapshot {
	/// Classical execution, never QSVT solution evidence.
	pub method: String,
	/// Frozen case name.
	pub case: String,
	/// Physical time.
	pub time: f64,
	/// Full independent DG dimension.
	pub independent_dimension: usize,
	/// Full-rank pressure, momentum and continuity recovery.
	pub pressure: SimplexPressureRecovery,
	/// Mass-normalized kinetic energy over the actual polygonal domain.
	pub mean_kinetic_energy: f64,
	/// Full pressure plus viscous force on the polygonal cylinder.
	pub cylinder_force: [f64; 3],
	/// Drag coefficient using the manifest `U_ref`, diameter and span.
	pub drag_coefficient: f64,
	/// Lift coefficient using the same normalization.
	pub lift_coefficient: f64,
	/// Geometric error of the circle approximation.
	pub maximum_geometry_deviation: f64,
	/// Full prescribed normal and divergence residual.
	pub boundary_residual: f64,
}

impl CylinderReference {
	/// Run the exact same full DG ODE by classical RK4 for a bounded reference interval.
	///
	/// # Errors
	/// Rejects invalid steps, frozen-window violations or any numerical failure.
	pub fn reference(&self, dt: f64, steps: u32) -> Result<CylinderSnapshot, CfdError> {
		let time = dt * f64::from(steps);
		if !time.is_finite() || time > self.manifest.time_window[1] {
			return Err(CfdError::InvalidInput(
				"cylinder reference time exceeds frozen window",
			));
		}
		let state = self.model.integrate_rk4(
			&self.initial_state,
			dt,
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("step count overflow"))?,
		)?;
		let pressure = self.model.reconstruct_pressure(&state)?;
		let cylinder_force =
			self.model
				.boundary_force(&state, &pressure.cell_pressure, "cylinder")?;
		let span = if self.manifest.dimension == 3 { 4. } else { 1. };
		let normalization =
			0.5 * self.manifest.reference_velocity.powi(2) * self.manifest.reference_length * span;
		Ok(CylinderSnapshot {
			method: "classical RK4; full BDM1/P0 on an explicitly polygonal cylinder mesh".into(),
			case: self.manifest.id.clone(),
			time,
			independent_dimension: self.model.dimension(),
			pressure,
			mean_kinetic_energy: self.model.energy(&state)? / self.model.volume(),
			cylinder_force,
			drag_coefficient: cylinder_force[0] / normalization,
			lift_coefficient: cylinder_force[1] / normalization,
			maximum_geometry_deviation: self.maximum_geometry_deviation,
			boundary_residual: self.model.boundary_residual(&state)?,
		})
	}
}

/// Count all local velocity coefficients and algebraic normal/divergence constraint rows
/// from cylinder mesh topology without allocating any dense matrix or full chart.
///
/// For the approved 3D case this is an estimate-only algebraic count: Neumann far-field
/// and convective-outlet dynamics remain unsupported, and this does not admit execution.
///
/// # Errors
/// Rejects unknown cases, invalid mesh controls or overflowing topology counts.
pub fn cylinder_chart_dimensions(
	id: &str,
	angular_sectors: u32,
	radial_layers: u32,
	span_layers: u32,
) -> Result<(usize, usize), CfdError> {
	use std::collections::BTreeMap;
	if !["shedding2d", "shedding3d"].contains(&id) {
		return Err(CfdError::InvalidInput("not a cylinder case"));
	}
	let mut planar = planar(id, angular_sectors, radial_layers)?;
	if id == "shedding3d" {
		for boundary in &mut planar.boundaries {
			if boundary.label == "far-wall" {
				boundary.velocity = None;
			}
		}
	}
	let (cells, boundaries, periodic, dimension) = if id == "shedding2d" {
		if span_layers != 1 {
			return Err(CfdError::InvalidInput("2D cylinder has no span layers"));
		}
		(planar.cells, planar.boundaries, Vec::new(), 2)
	} else {
		let (_, cells, boundaries, periodic) = extrude(&planar, span_layers)?;
		(cells, boundaries, periodic, 3)
	};
	let mut incidence: BTreeMap<Vec<usize>, usize> = BTreeMap::new();
	for cell in &cells {
		for omitted in 0..=dimension {
			let mut face: Vec<_> = cell
				.iter()
				.enumerate()
				.filter(|&(i, _)| i != omitted)
				.map(|(_, v)| *v)
				.collect();
			face.sort_unstable();
			*incidence.entry(face).or_default() += 1;
		}
	}
	let interior = incidence.values().filter(|&&v| v == 2).count();
	let prescribed = boundaries.iter().filter(|b| b.velocity.is_some()).count();
	let trace_constraints = interior
		.checked_add(periodic.len())
		.and_then(|v| v.checked_add(prescribed))
		.and_then(|v| v.checked_mul(dimension))
		.ok_or(CfdError::InvalidInput("trace count overflow"))?;
	let rank = trace_constraints
		.checked_add(cells.len())
		.ok_or(CfdError::InvalidInput("constraint count overflow"))?;
	let local = cells
		.len()
		.checked_mul(dimension * (dimension + 1))
		.ok_or(CfdError::InvalidInput("velocity count overflow"))?;
	if rank >= local {
		return Err(CfdError::Assembly("invalid cylinder topology rank"));
	}
	Ok((local, rank))
}

fn planar_boundaries(
	id: &str,
	vertices: &[[f64; 3]],
	angles: &[f64],
	layers: usize,
	radius: f64,
	bounds: [f64; 4],
) -> (Vec<BoundaryFacet>, f64) {
	let count = angles.len();
	let mut boundaries = Vec::new();
	let mut deviation = 0_f64;
	for i in 0..count {
		let next = (i + 1) % count;
		let delta = (angles[next] - angles[i]).rem_euclid(std::f64::consts::TAU);
		deviation = deviation.max(radius * (1. - (0.5 * delta).cos()));
		boundaries.push(BoundaryFacet {
			vertices: vec![i, next],
			velocity: Some(vec![[0.; 3]; 2]),
			label: "cylinder".into(),
		});
		let edge = vec![layers * count + i, layers * count + next];
		let midpoint = [
			0.5 * (vertices[edge[0]][0] + vertices[edge[1]][0]),
			0.5 * (vertices[edge[0]][1] + vertices[edge[1]][1]),
		];
		let (label, velocity) = if (midpoint[0] - bounds[1]).abs() < 1e-10 {
			("outlet", None)
		} else if (midpoint[0] - bounds[0]).abs() < 1e-10 {
			let values = if id == "shedding2d" {
				let y0 = vertices[edge[0]][1];
				let y1 = vertices[edge[1]][1];
				let quadratic = -6. * (y1 - y0).powi(2) / 0.41_f64.powi(2);
				[y0, y1]
					.map(|y| {
						[
							6. * y * (0.41 - y) / 0.41_f64.powi(2) - quadratic / 6.,
							0.,
							0.,
						]
					})
					.to_vec()
			} else {
				vec![[1., 0., 0.]; 2]
			};
			("inlet", Some(values))
		} else {
			(
				"far-wall",
				Some(vec![
					if id == "shedding2d" {
						[0.; 3]
					} else {
						[1., 0., 0.]
					};
					2
				]),
			)
		};
		boundaries.push(BoundaryFacet {
			vertices: edge,
			velocity,
			label: label.into(),
		});
	}
	(boundaries, deviation)
}
