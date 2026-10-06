//! Higher physical order for the frozen Taylor–Green and cavity box definitions.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Fixed two/three-dimensional geometry and checked full state sizes"
)]
use crate::{
	CfdError,
	cases::{self, CaseManifest, ReferenceSnapshot},
	physical_space::{PhysicalPressureRecovery, PhysicalSpace},
	simplex::BoxBoundary,
};

/// Bounded complete BDM1/P0 or BDM2/P1 physical reference.
pub struct HigherOrderBoxReference {
	pub manifest: CaseManifest,
	pub reynolds: u32,
	pub model: PhysicalSpace,
	pub initial_state: Vec<f64>,
}
/// Observables use the existing conventions; pressure has all local P1 modes at BDM2.
#[derive(Debug, serde::Serialize)]
pub struct HigherOrderSnapshot {
	pub physical_order: usize,
	/// Conservative total physical RK4 work; earlier physical assembly is separate.
	pub integration_modeled_work: usize,
	/// Explicit global allowance used for the complete trajectory.
	pub integration_work_limit: usize,
	#[serde(flatten)]
	pub observables: ReferenceSnapshot<PhysicalPressureRecovery>,
}
/// Assemble every higher-order physical coordinate, without a lift or quantum execution.
/// # Errors
/// Rejects non-box cases, unfrozen Reynolds choices and the physical reference size budget.
pub fn box_reference(
	id: &str,
	reynolds: u32,
	subdivisions: u32,
	order: usize,
) -> Result<HigherOrderBoxReference, CfdError> {
	let manifest = cases::manifest(id)?;
	let nu = manifest.viscosity(reynolds)?;
	let (extent, boundary) = match id {
		"tgv2d" | "tgv3d" => (std::f64::consts::TAU, BoxBoundary::Periodic),
		"cavity2d" | "cavity3d" => (1., BoxBoundary::Cavity { lid_speed: 1. }),
		_ => {
			return Err(CfdError::Unsupported(
				"higher-order assembly supports box cases".into(),
			));
		}
	};
	let model = PhysicalSpace::box_mesh(
		manifest.dimension,
		subdivisions,
		extent,
		nu,
		boundary,
		order,
	)?;
	let initial_state = match id {
		"tgv2d" => model.project_velocity(cases::tgv2d_initial)?,
		"tgv3d" => model.project_velocity(cases::tgv3d_initial)?,
		_ => vec![0.; model.dimension()],
	};
	Ok(HigherOrderBoxReference {
		manifest,
		reynolds,
		model,
		initial_state,
	})
}
impl HigherOrderBoxReference {
	/// Evolve the same complete physical ODE and reconstruct velocity and pressure observables.
	/// # Errors
	/// Rejects invalid time parameters, numerical failures and physical work admission.
	pub fn reference(&self, dt: f64, steps: u32) -> Result<HigherOrderSnapshot, CfdError> {
		self.reference_with_work_limit(dt, steps, 1_000_000_000)
	}
	/// Complete higher-order reference with an explicit modeled integration-work cap.
	/// # Errors
	/// Rejects invalid times, work admission and the same physical failures as `reference`.
	#[allow(
		clippy::too_many_lines,
		reason = "One reference transaction retains integration admission and all physical diagnostics"
	)]
	pub fn reference_with_work_limit(
		&self,
		dt: f64,
		steps: u32,
		max_work: usize,
	) -> Result<HigherOrderSnapshot, CfdError> {
		let time = dt * f64::from(steps);
		if !dt.is_finite()
			|| dt <= 0.
			|| steps == 0
			|| !time.is_finite()
			|| time > self.manifest.time_window[1]
		{
			return Err(CfdError::InvalidInput(
				"invalid higher-order reference interval",
			));
		}
		let step_count =
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("step count overflow"))?;
		let integration_modeled_work = self.model.integration_work_bound(step_count)?;
		let state = self.model.integrate_rk4_with_work_limit(
			&self.initial_state,
			time,
			step_count,
			max_work,
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
		let (analytic_velocity_error_l2, analytic_pressure_error_l2) =
			if self.manifest.id == "tgv2d" {
				let decay = (-2. * self.manifest.viscosity(self.reynolds)? * time).exp();
				(
					Some(self.model.velocity_error_l2(&state, |p| {
						cases::tgv2d_initial(p).map(|v| v * decay)
					})?),
					Some(self.model.pressure_error_l2(&state, |[x, y, _]| {
						0.25 * ((2. * x).cos() + (2. * y).cos()) * decay * decay
					})?),
				)
			} else {
				(None, None)
			};
		let drift = self.model.drift(&state)?;
		let steady_residual_l2 = drift.iter().fold(0_f64, |norm, v| norm.hypot(*v));
		let (centerline_profiles, midplane_samples, primary_vortex_candidate) =
			if self.manifest.id.starts_with("cavity") {
				cases::cavity_probes(self.manifest.dimension, |points| {
					points
						.iter()
						.map(|&point| self.model.sample_velocity(&state, point))
						.collect()
				})?
			} else {
				(Vec::new(), Vec::new(), None)
			};
		let cavity_3d = if self.manifest.id == "cavity3d" {
			Some(crate::cavity_observations::cavity_3d_probes(|points| {
				points
					.iter()
					.map(|&point| self.model.sample_velocity(&state, point))
					.collect()
			})?)
		} else {
			None
		};
		Ok(HigherOrderSnapshot {
			physical_order: self.model.order(),
			integration_modeled_work,
			integration_work_limit: max_work,
			observables: ReferenceSnapshot {
				method: format!(
					"classical RK4 of the complete BDM{}/P{} DG ODE",
					self.model.order(),
					self.model.order() - 1
				),
				case: self.manifest.id.clone(),
				reynolds: self.reynolds,
				time,
				independent_dimension: self.model.dimension(),
				mean_kinetic_energy: self.model.energy(&state)? / volume,
				mean_enstrophy: self.model.enstrophy(&state)? / volume,
				pressure: self.model.reconstruct_pressure(&state)?,
				center_velocity: self.model.sample_velocity(&state, center)?,
				analytic_velocity_error_l2,
				analytic_pressure_error_l2,
				steady_residual_l2,
				mean_gradient_dissipation: self.model.gradient_dissipation(&state)? / volume,
				centerline_profiles,
				midplane_samples,
				primary_vortex_candidate,
				cavity_3d,
			},
		})
	}
}

/// Complete box dimension from facet/pressure topology, without allocating a dense chart.
///
/// BDM2 has d(d+1)(d+2)/2 broken cell coefficients, d(d+1)/2 normal facet modes,
/// and d+1 divergence modes per cell. Closed or periodic connected boxes have one
/// pressure dependency. Construction separately checks numerical rank.
/// # Errors
/// Rejects unsupported orders, invalid topology and any machine-count overflow.
pub fn box_chart_dimensions(
	dimension: usize,
	subdivisions: u32,
	periodic: bool,
	order: usize,
) -> Result<(usize, usize), CfdError> {
	let (local_one, rank_one) =
		crate::simplex::box_chart_dimensions(dimension, subdivisions, periodic)?;
	if order == 1 {
		return Ok((local_one, rank_one));
	}
	if order != 2 {
		return Err(CfdError::Unsupported(
			"physical box order requires BDM1 or BDM2".into(),
		));
	}
	let cells = local_one / (dimension * (dimension + 1));
	let facets = (rank_one - (cells - 1)) / dimension;
	let local = cells
		.checked_mul(dimension * (dimension + 1) * (dimension + 2) / 2)
		.ok_or(CfdError::InvalidInput("BDM2 velocity dimension overflow"))?;
	let rank = facets
		.checked_mul(dimension * (dimension + 1) / 2)
		.and_then(|trace| {
			cells
				.checked_mul(dimension + 1)
				.and_then(|pressure| trace.checked_add(pressure - 1))
		})
		.ok_or(CfdError::InvalidInput("BDM2 constraint rank overflow"))?;
	Ok((local, rank))
}
