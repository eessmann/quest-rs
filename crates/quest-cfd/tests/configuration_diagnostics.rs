#![allow(
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	clippy::float_cmp,
	reason = "Independent small resolution and concentration checks with exact delta-state moments"
)]
use quest_cfd::{
	configuration::ConfigurationGrid,
	configuration_diagnostics::{concentration, regularization_resolution},
};
use quest_numerics::Complex64;

#[test]
fn nonzero_sample_does_not_establish_resolved_regularization()
-> Result<(), Box<dyn std::error::Error>> {
	let coarse = ConfigurationGrid::uniform(5, -1., 1., 2, 1, 2000)?;
	assert!(coarse.initial_bump(&[0.; 5], 0.1).is_ok());
	assert!(regularization_resolution(&coarse, &[0.; 5], 0.1, 2).is_err());
	assert!(coarse.initial_bump(&[0.25; 5], 0.1).is_err());
	let fine = ConfigurationGrid::uniform(5, -1., 1., 4, 2, 300_000)?;
	let report = regularization_resolution(&fine, &[0.; 5], 0.6, 3)?;
	assert_eq!(report.distinct_samples_in_support, vec![5; 5]);
	assert!(!report.support_intersects_boundary);
	assert!(!report.convergence_certified);
	assert!(regularization_resolution(&fine, &[0.; 5], 0.6, 0).is_err());
	Ok(())
}

#[test]
fn concentration_gate_is_explicit_and_boundary_mass_is_only_occupation()
-> Result<(), Box<dyn std::error::Error>> {
	let grid = ConfigurationGrid::uniform(1, -1., 1., 4, 1, 8)?;
	let state = grid.initial_bump(&[0.], 0.75)?;
	let spread = concentration(&grid, &state, Some(0.01))?;
	assert!(spread.standard_deviations_per_spacing[0] > 0.01);
	assert!(spread.effective_nodal_coefficients > 1.);
	let mut delta = vec![Complex64::new(0., 0.); 8];
	delta[0] = Complex64::new(1., 0.);
	let diagnostic = concentration(&grid, &delta, None)?;
	assert_eq!(diagnostic.boundary_occupation_fraction, 1.);
	assert_eq!(diagnostic.standard_deviations_per_spacing, vec![0.]);
	assert!(concentration(&grid, &delta, Some(0.01)).is_err());
	assert!(concentration(&grid, &delta, Some(f64::NAN)).is_err());
	delta[0] = Complex64::new(f64::INFINITY, 0.);
	assert!(concentration(&grid, &delta, None).is_err());
	Ok(())
}
