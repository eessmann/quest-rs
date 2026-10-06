#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Bounded independent Airy, doubled-field energy and refinement references use indexed arithmetic; construction failures fail the test"
)]
use quest_cfd::kdv::KdvDg;
#[test]
fn frozen_kdv_keeps_the_evolving_auxiliary_field() {
	let model = KdvDg::new(4, 2).unwrap();
	assert_eq!(model.dimension(), 24);
	let initial = model.initial_state(0.05).unwrap();
	assert!(initial[12..].iter().all(|&v| v == 0.));
	let drift = model.drift(&initial).unwrap();
	assert!(drift[12..].iter().map(|v| v * v).sum::<f64>() > 1e-12);
}
#[test]
fn ultraweak_operator_is_skew_and_preserves_both_constant_fields() {
	for order in [2, 3] {
		let model = KdvDg::linear_airy(4, order).unwrap();
		let matrix = model
			.polynomial_ode()
			.jacobian(0., &vec![0.; model.dimension()])
			.unwrap();
		let mut error = 0_f64;
		for i in 0..model.dimension() {
			for j in 0..model.dimension() {
				error = error.max((matrix[i][j] + matrix[j][i]).abs());
			}
		}
		assert!(error < 1e-11, "skew error {error}");
		let constant = model.project_fields(|_| 0.17, |_| -0.23).unwrap();
		assert!(
			model
				.drift(&constant)
				.unwrap()
				.iter()
				.all(|v| v.abs() < 1e-11)
		);
	}
}
#[test]
fn nonlinear_combined_energy_and_means_are_conserved_and_direct_fluxes_match() {
	for order in [2, 3] {
		let model = KdvDg::new(4, order).unwrap();
		let state = (0..model.dimension())
			.map(|i| 0.03 * f64::from(u32::try_from(i + 1).unwrap()).sin())
			.collect::<Vec<_>>();
		let compiled = model.drift(&state).unwrap();
		let direct = model.direct_drift(&state).unwrap();
		let contraction = state.iter().zip(&compiled).map(|(a, b)| a * b).sum::<f64>();
		assert!(
			contraction.abs() < 1e-11,
			"energy contraction {contraction}"
		);
		for (a, b) in compiled.iter().zip(direct) {
			assert!((a - b).abs() < 1e-11, "{a} != {b}");
		}
		assert!(
			model
				.field_masses(&compiled)
				.unwrap()
				.iter()
				.all(|v| v.abs() < 1e-11)
		);
		let split = model.field_dimension();
		let physical = state[..split]
			.iter()
			.zip(&compiled[..split])
			.map(|(a, b)| a * b)
			.sum::<f64>();
		assert!(
			physical.abs() > 1e-6,
			"individual fields must be allowed to exchange energy"
		);
	}
}
#[test]
fn frozen_initial_state_keeps_all_twenty_four_carleman_coordinates() {
	use quest_cfd::carleman::{CarlemanLimits, SymmetricCarleman};
	let model = KdvDg::new(4, 2).unwrap();
	let initial = model.initial_state(0.05).unwrap();
	let ode = model.polynomial_ode();
	let evidence = ode.experimental_scaling(&initial, 0.1).unwrap();
	assert!(evidence.rc_upper.is_none());
	let lift =
		SymmetricCarleman::new(ode, 1, evidence.scale.unwrap(), CarlemanLimits::default()).unwrap();
	assert_eq!(lift.dimension(), 24);
	assert_eq!(lift.physical_dimension(), 24);
	for (a, b) in lift
		.recover(&lift.lift(&initial).unwrap())
		.unwrap()
		.iter()
		.zip(&initial)
	{
		assert!((a - b).abs() < 1e-17);
	}
}
#[test]
fn rejects_complete_state_and_assembly_budgets_before_growth() {
	use quest_cfd::kdv::{KdvLimits, KdvMode};
	assert!(KdvDg::new(1, 2).is_err());
	assert!(KdvDg::new(4, 1).is_err());
	assert!(KdvDg::new(u32::MAX, 3).is_err());
	assert!(
		KdvDg::with_limits(
			4,
			2,
			KdvMode::Nonlinear,
			KdvLimits {
				max_bytes: 1,
				..KdvLimits::default()
			}
		)
		.is_err()
	);
	assert!(
		KdvDg::with_limits(
			4,
			2,
			KdvMode::LinearAiry,
			KdvLimits {
				max_work: 0,
				..KdvLimits::default()
			}
		)
		.is_err()
	);
}
#[test]
fn linear_airy_phase_and_auxiliary_error_refine_in_space_and_order() {
	let mut errors = Vec::new();
	for (cells, order, steps) in [
		(4, 2, 256),
		(8, 2, 1024),
		(16, 2, 8192),
		(4, 3, 512),
		(8, 3, 4096),
		(16, 3, 8192),
	] {
		let model = KdvDg::linear_airy(cells, order).unwrap();
		let reference = model.classical_reference(0.05, 0.1, steps).unwrap();
		let error = reference.airy_l2_error.unwrap();
		eprintln!(
			"Airy cells={cells} p={order} L2={error} phi={} energy_change={}",
			reference.auxiliary_l2_norm,
			reference.final_energy - reference.initial_energy
		);
		errors.push(error);
		assert!(reference.auxiliary_l2_norm > 0.);
		assert!((reference.final_energy - reference.initial_energy).abs() < 1e-9);
	}
	// The coarse DG2 pair is pre-asymptotic with phi(0)=0; do not claim an ideal rate.
	assert!(errors[1] < errors[0]);
	assert!(errors[2] < errors[1] / 5.);
	assert!(errors[4] < errors[3] / 8.);
	assert!(errors[5] < errors[4] / 8.);
	assert!(errors[3] < errors[0] / 3.);
	assert!(errors[5] < errors[2] / 3.);
}
#[test]
fn nonlinear_rk4_time_refinement_is_separate_from_semidiscrete_conservation() {
	let model = KdvDg::new(4, 2).unwrap();
	let initial = model.initial_state(0.05).unwrap();
	let reference = model.integrate_rk4(&initial, 0.1 / 4096., 4096).unwrap();
	let mut previous = f64::INFINITY;
	for steps in [16u32, 32, 64] {
		let report = model.classical_reference(0.05, 0.1, steps).unwrap();
		assert!(report.airy_l2_error.is_none());
		assert!(report.auxiliary_l2_norm > 0.);
		let error = report
			.state
			.iter()
			.zip(&reference)
			.map(|(a, b)| (a - b).powi(2))
			.sum::<f64>()
			.sqrt();
		eprintln!(
			"KdV time steps={steps}, state_error={error}, energy_drift={}",
			report.final_energy - report.initial_energy
		);
		assert!(error < previous / 10.);
		previous = error;
		for (a, b) in report.initial_masses.iter().zip(report.final_masses) {
			assert!((a - b).abs() < 1e-14);
		}
	}
	// The lowered polynomial and independent physical flux residual define the same trajectory.
	let numerical = model.integrate_rk4(&initial, 0.0001, 100).unwrap();
	let compiled = model
		.polynomial_ode()
		.integrate_rk4(&initial, 0.0001, 100)
		.unwrap();
	for (a, b) in numerical.iter().zip(compiled) {
		assert!((a - b).abs() < 1e-13);
	}
}
#[test]
fn nonlinear_convection_has_the_physical_kdv_sign() {
	let model = KdvDg::new(8, 3).unwrap();
	let airy = KdvDg::linear_airy(8, 3).unwrap();
	let initial = model
		.project_fields(|x| 0.05_f64.mul_add(x.cos(), 0.3), |_| 0.)
		.unwrap();
	let full = model.drift(&initial).unwrap();
	let linear = airy.drift(&initial).unwrap();
	let expected = model
		.project_fields(
			|x| 6. * (0.05_f64.mul_add(x.cos(), 0.3)) * 0.05 * x.sin(),
			|_| 0.,
		)
		.unwrap();
	let error = full
		.iter()
		.zip(linear)
		.zip(&expected)
		.map(|((a, b), c)| (a - b - c).powi(2))
		.sum::<f64>()
		.sqrt();
	assert!(error < 0.001, "convection consistency {error}");
	assert!(
		full.iter()
			.zip(airy.drift(&initial).unwrap())
			.zip(expected)
			.map(|((a, b), c)| (a - b) * c)
			.sum::<f64>()
			> 0.001
	);
}
