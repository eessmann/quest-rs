//! Frozen physical benchmark contracts, independent of execution admission.

use crate::CfdError;

/// A benchmark family; each Reynolds entry denotes a separate configuration.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseManifest {
	/// Stable family identifier.
	pub id: String,
	/// Physical space dimension, either two or three.
	pub dimension: usize,
	/// Explicit nondimensional domain geometry.
	pub geometry: String,
	/// Frozen Reynolds numbers, `U_ref L_ref / nu`.
	pub reynolds: Vec<u32>,
	/// Reference velocity in the stated nondimensional coordinates.
	pub reference_velocity: f64,
	/// Reference length; must not be confused with box extent.
	pub reference_length: f64,
	/// Boundary conditions, including outflow and periodic directions.
	pub boundaries: Vec<String>,
	/// Initial velocity and pressure/gauge convention.
	pub initial_condition: String,
	/// Simulation interval in nondimensional physical time.
	pub time_window: [f64; 2],
	/// Measurement interval, distinct from spin-up.
	pub measurement_window: [f64; 2],
	/// Minimum observed shedding cycles in the measurement window, when required.
	#[serde(default)]
	pub measurement_minimum_cycles: Option<u32>,
	/// Physical observables with their normalization conventions.
	pub observables: Vec<String>,
	/// Source or explicit project-defined benchmark convention.
	pub provenance: String,
}

/// Names of all six frozen benchmark families.
#[must_use]
pub const fn family_names() -> &'static [&'static str] {
	&[
		"tgv2d",
		"tgv3d",
		"cavity2d",
		"cavity3d",
		"shedding2d",
		"shedding3d",
	]
}

/// Load a checked-in frozen manifest without network access.
///
/// # Errors
/// Rejects unknown case identifiers or invalid embedded manifests.
pub fn manifest(id: &str) -> Result<CaseManifest, CfdError> {
	let source = match id {
		"tgv2d" => include_str!("../cases/tgv2d.json"),
		"tgv3d" => include_str!("../cases/tgv3d.json"),
		"cavity2d" => include_str!("../cases/cavity2d.json"),
		"cavity3d" => include_str!("../cases/cavity3d.json"),
		"shedding2d" => include_str!("../cases/shedding2d.json"),
		"shedding3d" => include_str!("../cases/shedding3d.json"),
		_ => return Err(CfdError::Unsupported(format!("unknown case {id}"))),
	};
	let case: CaseManifest = serde_json::from_str(source)
		.map_err(|_| CfdError::Assembly("invalid embedded case manifest"))?;
	case.validate()?;
	Ok(case)
}

impl CaseManifest {
	/// Validate dimensional and time-window consistency.
	///
	/// # Errors
	/// Rejects malformed geometry-independent physical metadata.
	pub fn validate(&self) -> Result<(), CfdError> {
		if ![2, 3].contains(&self.dimension)
			|| self.reynolds.is_empty()
			|| self.reynolds.contains(&0)
		{
			return Err(CfdError::InvalidInput(
				"invalid case dimension or Reynolds number",
			));
		}
		if !self.reference_velocity.is_finite()
			|| self.reference_velocity <= 0.
			|| !self.reference_length.is_finite()
			|| self.reference_length <= 0.
		{
			return Err(CfdError::InvalidInput("invalid Reynolds reference scales"));
		}
		let [start, end] = self.time_window;
		let [measure_start, measure_end] = self.measurement_window;
		if ![start, end, measure_start, measure_end]
			.iter()
			.all(|x| x.is_finite())
			|| start < 0.
			|| end <= start
			|| measure_start < start
			|| measure_end > end
			|| measure_end <= measure_start
		{
			return Err(CfdError::InvalidInput(
				"invalid simulation or measurement interval",
			));
		}
		if self.geometry.is_empty() || self.boundaries.is_empty() || self.observables.is_empty() {
			return Err(CfdError::InvalidInput("incomplete physical case contract"));
		}
		Ok(())
	}

	/// Kinematic viscosity with the manifest's precise Reynolds convention.
	///
	/// # Errors
	/// Rejects a Reynolds number not frozen for this family or invalid scales.
	#[allow(clippy::arithmetic_side_effects)]
	pub fn viscosity(&self, reynolds: u32) -> Result<f64, CfdError> {
		self.validate()?;
		if !self.reynolds.contains(&reynolds) {
			return Err(CfdError::InvalidInput(
				"Reynolds number is not a frozen configuration",
			));
		}
		let nu = self.reference_velocity * self.reference_length / f64::from(reynolds);
		if !nu.is_finite() || nu <= 0. {
			return Err(CfdError::InvalidInput(
				"invalid viscosity from reference scales",
			));
		}
		Ok(nu)
	}
}

/// A physical box case with the complete DG state and its frozen contract.
#[derive(Clone, Debug)]
pub struct BoxReference {
	/// Frozen physical definition.
	pub manifest: CaseManifest,
	/// Selected Reynolds number.
	pub reynolds: u32,
	/// Complete BDM1/P0 discretization of the physical box.
	pub model: crate::simplex::SimplexBdm,
	/// Complete projected initial mass coordinates.
	pub initial_state: Vec<f64>,
}

/// Assemble a real full-DG Taylor-Green or cavity case and project its initial data.
///
/// # Errors
/// Rejects unsupported geometry, non-frozen Reynolds choices, or unbudgeted full meshes.
pub fn box_reference(id: &str, reynolds: u32, subdivisions: u32) -> Result<BoxReference, CfdError> {
	use crate::simplex::{BoxBoundary, SimplexBdm};
	let manifest = manifest(id)?;
	let viscosity = manifest.viscosity(reynolds)?;
	let (extent,boundary)=match id {
        "tgv2d"|"tgv3d" => (std::f64::consts::TAU,BoxBoundary::Periodic),
        "cavity2d"|"cavity3d" => (1.,BoxBoundary::Cavity {lid_speed:1.}),
        _=>return Err(CfdError::Unsupported("cylinder geometry, boundary lifting and traction observables are not implemented by the box reference".into())),
    };
	let model = SimplexBdm::box_mesh(
		manifest.dimension,
		subdivisions,
		extent,
		viscosity,
		boundary,
	)?;
	let initial_state = match id {
		"tgv2d" => model.project_velocity(tgv2d_initial)?,
		"tgv3d" => model.project_velocity(tgv3d_initial)?,
		_ => vec![0.; model.dimension()],
	};
	Ok(BoxReference {
		manifest,
		reynolds,
		model,
		initial_state,
	})
}

#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn tgv2d_initial([x, y, _]: [f64; 3]) -> [f64; 3] {
	[x.sin() * y.cos(), -x.cos() * y.sin(), 0.]
}
#[allow(clippy::arithmetic_side_effects)]
pub(crate) fn tgv3d_initial([x, y, z]: [f64; 3]) -> [f64; 3] {
	[
		x.sin() * y.cos() * z.cos(),
		-x.cos() * y.sin() * z.cos(),
		0.,
	]
}

/// Classical full-DG reference observables at a requested physical time.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ReferenceSnapshot<P = crate::simplex::SimplexPressureRecovery> {
	/// Explicit execution provenance; never quantum/QSVT evidence.
	pub method: String,
	/// Physical case family.
	pub case: String,
	/// Selected Reynolds number.
	pub reynolds: u32,
	/// Physical time of this snapshot.
	pub time: f64,
	/// Complete independent coefficient count.
	pub independent_dimension: usize,
	/// Kinetic energy averaged over physical volume.
	pub mean_kinetic_energy: f64,
	/// Enstrophy averaged over physical volume.
	pub mean_enstrophy: f64,
	/// Full momentum, continuity and mean-zero pressure recovery evidence.
	pub pressure: P,
	/// Velocity at the geometric center, using the first-cell DG trace convention.
	pub center_velocity: [f64; 3],
	/// Analytic velocity error for 2D Taylor-Green only; absent for other cases.
	pub analytic_velocity_error_l2: Option<f64>,
	/// Analytic mean-zero pressure error, available only for 2D Taylor-Green.
	pub analytic_pressure_error_l2: Option<f64>,
	/// Norm of the complete mass-coordinate drift; a steady-state diagnostic, not certification.
	pub steady_residual_l2: f64,
	/// Physical elementwise gradient dissipation divided by domain volume.
	pub mean_gradient_dissipation: f64,
	/// Centerline x/y velocity profiles for cavity families.
	pub centerline_profiles: Vec<VelocityProbe>,
	/// Sampled cavity midplane velocity field (z=0 in 2D and z=L/2 in 3D).
	pub midplane_samples: Vec<VelocityProbe>,
	/// 2D cavity candidate from the largest absolute sampled streamfunction; coarse, not certified.
	pub primary_vortex_candidate: Option<[f64; 3]>,
	/// Fixed transverse-plane samples and spanwise reflection diagnostics for the 3D cavity.
	pub cavity_3d: Option<crate::cavity_observations::Cavity3dObservations>,
}

impl BoxReference {
	/// Integrate the same complete physical DG ODE by classical RK4 and report its observables.
	///
	/// # Errors
	/// Rejects invalid time steps, integration extending beyond the frozen window,
	/// singular pressure recovery, or numerical overflow.
	#[allow(clippy::arithmetic_side_effects)]
	pub fn reference(&self, dt: f64, steps: u32) -> Result<ReferenceSnapshot, CfdError> {
		let time = dt * f64::from(steps);
		if !time.is_finite() || time > self.manifest.time_window[1] {
			return Err(CfdError::InvalidInput(
				"reference time exceeds frozen case window",
			));
		}
		let state = self.model.integrate_rk4(
			&self.initial_state,
			dt,
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("step count overflow"))?,
		)?;
		let extent = if self.manifest.id.starts_with("tgv") {
			std::f64::consts::TAU
		} else {
			1.
		};
		let volume = extent.powi(
			i32::try_from(self.manifest.dimension)
				.map_err(|_| CfdError::InvalidInput("dimension overflow"))?,
		);
		let center = [
			extent * 0.5,
			extent * 0.5,
			if self.manifest.dimension == 3 {
				extent * 0.5
			} else {
				0.
			},
		];
		let analytic_velocity_error_l2 = if self.manifest.id == "tgv2d" {
			let decay = (-2. * self.manifest.viscosity(self.reynolds)? * time).exp();
			Some(
				self.model
					.velocity_error_l2(&state, |p| tgv2d_initial(p).map(|v| v * decay))?,
			)
		} else {
			None
		};
		let pressure = self.model.reconstruct_pressure(&state)?;
		let analytic_pressure_error_l2 = if self.manifest.id == "tgv2d" {
			let decay = (-4. * self.manifest.viscosity(self.reynolds)? * time).exp();
			Some(
				self.model
					.pressure_error_l2(&pressure.cell_pressure, |[x, y, _]| {
						0.25 * ((2. * x).cos() + (2. * y).cos()) * decay
					})?,
			)
		} else {
			None
		};
		let drift = self.model.drift(&state)?;
		let steady_residual_l2 = drift.iter().map(|v| v * v).sum::<f64>().sqrt();
		let (centerline_profiles, midplane_samples, primary_vortex_candidate) =
			if self.manifest.id.starts_with("cavity") {
				cavity_probes(self.manifest.dimension, |points| {
					self.model.sample_velocities(&state, points)
				})?
			} else {
				(Vec::new(), Vec::new(), None)
			};
		let cavity_3d = if self.manifest.id == "cavity3d" {
			Some(crate::cavity_observations::cavity_3d_probes(|points| {
				self.model.sample_velocities(&state, points)
			})?)
		} else {
			None
		};
		Ok(ReferenceSnapshot {
			method: "classical RK4 of the complete BDM1/P0 DG ODE".into(),
			case: self.manifest.id.clone(),
			reynolds: self.reynolds,
			time,
			independent_dimension: self.model.dimension(),
			mean_kinetic_energy: self.model.energy(&state)? / volume,
			mean_enstrophy: self.model.enstrophy(&state)? / volume,
			pressure,
			center_velocity: self.model.sample_velocity(&state, center)?,
			analytic_velocity_error_l2,
			analytic_pressure_error_l2,
			steady_residual_l2,
			mean_gradient_dissipation: self.model.gradient_dissipation(&state)? / volume,
			centerline_profiles,
			midplane_samples,
			primary_vortex_candidate,
			cavity_3d,
		})
	}
}

/// A physical velocity probe with an explicit DG trace convention.
#[derive(Clone, Debug, serde::Serialize)]
pub struct VelocityProbe {
	/// Physical coordinate.
	pub point: [f64; 3],
	/// First-containing-cell velocity trace.
	pub velocity: [f64; 3],
}

pub(crate) type CavityProbes = (Vec<VelocityProbe>, Vec<VelocityProbe>, Option<[f64; 3]>);

#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
pub(crate) fn cavity_probes(
	dimension: usize,
	sample: impl Fn(&[[f64; 3]]) -> Result<Vec<[f64; 3]>, CfdError>,
) -> Result<CavityProbes, CfdError> {
	let z = if dimension == 3 { 0.5 } else { 0. };
	let mut lines = Vec::new();
	for i in 0..=10 {
		let t = f64::from(i) / 10.;
		lines.extend([[0.5, t, z], [t, 0.5, z]]);
	}
	let line_values = sample(&lines)?;
	let profiles = lines
		.iter()
		.copied()
		.zip(line_values)
		.map(|(point, velocity)| VelocityProbe { point, velocity })
		.collect();
	let mut plane = Vec::new();
	for i in 0..=10 {
		for j in 0..=10 {
			plane.push([f64::from(i) / 10., f64::from(j) / 10., z]);
		}
	}
	let plane_values = sample(&plane)?;
	let mut candidate = None;
	let mut largest = 1e-14_f64;
	if dimension == 2 {
		for i in 1..10 {
			let mut psi = 0.;
			for j in 1..10 {
				psi = 0.05_f64.mul_add(
					plane_values[i * 11 + j - 1][0] + plane_values[i * 11 + j][0],
					psi,
				);
				if psi.abs() > largest {
					largest = psi.abs();
					candidate = Some(plane[i * 11 + j]);
				}
			}
		}
	}
	let samples = plane
		.into_iter()
		.zip(plane_values)
		.map(|(point, velocity)| VelocityProbe { point, velocity })
		.collect();
	Ok((profiles, samples, candidate))
}
