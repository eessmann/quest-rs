//! Independent classical transport validation against the identical complete DG drift.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Bounded reference integration checks finite results and reports its numerical errors"
)]
use crate::{
	CfdError, PeriodicBdm1,
	configuration::{ConfigurationGrid, ConfigurationObservables},
	configuration_diagnostics::{ConcentrationDiagnostic, concentration},
};
use quest_numerics::{Complex64, SparseLimits, SparseMatrix};

/// Distinct validation result, never a QSVT or converged CFD solution claim.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TransportComparison {
	pub method: String,
	pub retained_coordinates: usize,
	pub configuration_dimension: usize,
	pub horizon: f64,
	pub initial: ConfigurationObservables,
	pub lifted: ConfigurationObservables,
	pub trajectory_mean_coordinates: Vec<f64>,
	pub trajectory_mean_kinetic_energy: f64,
	pub coordinate_mean_error: f64,
	pub energy_error: f64,
	pub probability_drift: f64,
	pub initial_concentration: ConcentrationDiagnostic,
	pub final_concentration: ConcentrationDiagnostic,
}
fn step(
	generator: &SparseMatrix,
	state: &[Complex64],
	dt: f64,
	limits: SparseLimits,
) -> Result<Vec<Complex64>, CfdError> {
	let k1 = generator.matvec(state, limits)?;
	let intermediate: Vec<_> = state
		.iter()
		.zip(&k1)
		.map(|(x, k)| x + 0.5 * dt * k)
		.collect();
	let k2 = generator.matvec(&intermediate, limits)?;
	let intermediate: Vec<_> = state
		.iter()
		.zip(&k2)
		.map(|(x, k)| x + 0.5 * dt * k)
		.collect();
	let k3 = generator.matvec(&intermediate, limits)?;
	let intermediate: Vec<_> = state.iter().zip(&k3).map(|(x, k)| x + dt * k).collect();
	let k4 = generator.matvec(&intermediate, limits)?;
	let result: Vec<_> = state
		.iter()
		.zip(&k1)
		.zip(&k2)
		.zip(&k3)
		.zip(&k4)
		.map(|((((x, a), b), c), d)| x + (a + 2.0 * b + 2.0 * c + d) * (dt / 6.0))
		.collect();
	if result
		.iter()
		.any(|z| !z.re.is_finite() || !z.im.is_finite())
	{
		return Err(CfdError::InvalidInput(
			"classical lifted reference overflow",
		));
	}
	Ok(result)
}
/// Compare transported observables with an ensemble of full five-coordinate DG trajectories.
///
/// Classical RK4 is used only for the two independent validation references. The
/// quantum workflow still assembles and solves one complete global history.
/// # Errors
/// Rejects malformed time/state dimensions, resource limits or integration failures.
pub fn compare_full_dg_transport(
	flow: &PeriodicBdm1,
	grid: &ConfigurationGrid,
	initial: &[Complex64],
	horizon: f64,
	steps: u32,
	limits: SparseLimits,
) -> Result<TransportComparison, CfdError> {
	if !horizon.is_finite() || horizon <= 0.0 || steps == 0 || grid.axes() != flow.dimension() {
		return Err(CfdError::InvalidInput(
			"invalid full-DG transport validation inputs",
		));
	}
	let initial_observables = grid.observables(initial, |a| flow.energy(a))?;
	let generator = grid.generator(flow, limits)?;
	let bytes = grid
		.dimension()
		.checked_mul(size_of::<Complex64>())
		.and_then(|n| n.checked_mul(10))
		.and_then(|n| {
			n.checked_add(
				generator
					.nnz()
					.checked_mul(size_of::<Complex64>() + size_of::<usize>())?,
			)
		})
		.ok_or(CfdError::InvalidInput("transport reference byte overflow"))?;
	if bytes > limits.max_bytes {
		return Err(CfdError::InvalidInput(
			"transport reference exceeds storage admission",
		));
	}
	let dt = horizon / f64::from(steps);
	let mut lifted = initial.to_vec();
	for _ in 0..steps {
		lifted = step(&generator, &lifted, dt, limits)?;
	}
	let lifted_observables = grid.observables(&lifted, |a| flow.energy(a))?;
	let mut means = vec![0.0; flow.dimension()];
	let mut energy = 0.0;
	for (index, amplitude) in initial.iter().enumerate() {
		let probability = amplitude.norm_sqr() / initial_observables.probability;
		if probability == 0.0 {
			continue;
		}
		let point = grid
			.point(index)
			.ok_or(CfdError::Assembly("trajectory tensor index"))?;
		let final_state = flow.integrate_rk4(
			&point,
			dt,
			usize::try_from(steps).map_err(|_| CfdError::InvalidInput("trajectory step count"))?,
		)?;
		for (mean, value) in means.iter_mut().zip(&final_state) {
			*mean += probability * value;
		}
		energy = probability.mul_add(flow.energy(&final_state)?, energy);
	}
	let coordinate_mean_error = means
		.iter()
		.zip(&lifted_observables.coordinate_means)
		.fold(0.0_f64, |norm, (a, b)| norm.hypot(a - b));
	let energy_error = (energy - lifted_observables.mean_kinetic_energy).abs();
	let probability_drift =
		(lifted_observables.probability - initial_observables.probability).abs();
	let initial_concentration = concentration(grid, initial, None)?;
	let final_concentration = concentration(grid, &lifted, None)?;
	Ok(TransportComparison {method:"classical full-DG trajectory ensemble versus classical full-coordinate KvN semidiscretization; no quantum execution".to_owned(),retained_coordinates:flow.dimension(),configuration_dimension:grid.dimension(),horizon,initial:initial_observables,lifted:lifted_observables,trajectory_mean_coordinates:means,trajectory_mean_kinetic_energy:energy,coordinate_mean_error,energy_error,probability_drift,initial_concentration,final_concentration})
}
