#![allow(
	clippy::panic_in_result_fn,
	reason = "Test assertions compare independent full-system references"
)]
#[test]
fn lifted_nonlinear_smoke_is_compared_with_identical_full_dg_trajectories()
-> Result<(), Box<dyn std::error::Error>> {
	let flow = quest_cfd::PeriodicBdm1::assemble(0.01)?;
	let grid = quest_cfd::configuration::ConfigurationGrid::uniform(5, -1.0, 1.0, 2, 1, 2000)?;
	let initial = grid.initial_bump(&[0.15, -0.1, 0.07, 0.11, -0.04], 1.2)?;
	let report = quest_cfd::validation::compare_full_dg_transport(
		&flow,
		&grid,
		&initial,
		0.001,
		4,
		quest_numerics::SparseLimits::default(),
	)?;
	assert_eq!(report.retained_coordinates, 5);
	assert!(report.probability_drift < 1e-11);
	assert!(report.coordinate_mean_error.is_finite());
	assert!(report.energy_error.is_finite());
	assert!(report.initial.mean_kinetic_energy > 0.0);
	Ok(())
}
