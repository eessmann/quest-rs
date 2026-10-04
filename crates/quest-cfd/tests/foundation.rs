#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops
)]
#[test]
fn full_bdm_chart_has_exactly_five_independent_modes() {
	let model = quest_cfd::PeriodicBdm1::assemble(0.0).unwrap();
	let d = model.diagnostics();
	assert_eq!(d.local_velocity_dimension, 12);
	assert_eq!(d.constraint_rank, 7);
	assert_eq!(d.independent_dimension, 5);
	assert!(d.constraint_residual < 1e-12);
	assert!(d.mass_orthogonality_residual < 1e-12);
}

#[test]
fn inviscid_full_drift_is_nonlinear_nonzero_and_energy_conserving() {
	let model = quest_cfd::PeriodicBdm1::assemble(0.0).unwrap();
	let state = [0.7, -0.4, 0.9, 0.3, -0.2];
	let drift = model.drift(&state).unwrap();
	assert!(drift.iter().map(|v| v * v).sum::<f64>() > 1e-4);
	assert!(
		state
			.iter()
			.zip(&drift)
			.map(|(a, b)| a * b)
			.sum::<f64>()
			.abs()
			< 1e-11
	);
	let doubled = model.drift(&state.map(|x| 2.0 * x)).unwrap();
	for (a, b) in doubled.iter().zip(&drift) {
		assert!((a - 4.0 * b).abs() < 1e-10);
	}
}

#[test]
fn viscosity_dissipates_but_preserves_both_uniform_mean_flows() {
	let model = quest_cfd::PeriodicBdm1::assemble(0.01).unwrap();
	let state = [0.7, -0.4, 0.9, 0.3, -0.2];
	let drift = model.drift(&state).unwrap();
	assert!(state.iter().zip(&drift).map(|(a, b)| a * b).sum::<f64>() < -1e-3);
	for c in [
		[1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0.],
		[0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0.],
	] {
		let state = model.coordinates(&c).unwrap();
		assert!((model.energy(&state).unwrap() - 0.5).abs() < 1e-12);
		let reconstructed = model.coefficients(&state).unwrap();
		for (a, b) in c.iter().zip(reconstructed) {
			assert!((a - b).abs() < 1e-12);
		}
		assert!(model.drift(&state).unwrap().iter().all(|x| x.abs() < 1e-11));
	}
}

#[test]
fn pressure_and_normal_multipliers_close_all_twelve_momentum_equations() {
	let model = quest_cfd::PeriodicBdm1::assemble(0.03).unwrap();
	let state = [0.7, -0.4, 0.9, 0.3, -0.2];
	let p = model.reconstruct_pressure(&state).unwrap();
	assert!(p.momentum_residual < 1e-11, "{p:?}");
	assert!(p.continuity_residual < 1e-12);
	assert!((p.cell_pressure[0] + p.cell_pressure[1]).abs() < 1e-12);
}

#[test]
fn invalid_inputs_and_nonconforming_velocities_are_rejected() {
	assert!(quest_cfd::PeriodicBdm1::assemble(-1.0).is_err());
	assert!(quest_cfd::PeriodicBdm1::assemble(f64::NAN).is_err());
	let model = quest_cfd::PeriodicBdm1::assemble(0.01).unwrap();
	assert!(model.drift(&[0.0; 4]).is_err());
	assert!(model.drift(&[f64::NAN; 5]).is_err());
	assert!(model.drift(&[f64::MAX; 5]).is_err());
	assert!(
		model
			.coordinates(&[1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.])
			.is_err()
	);
	assert!(model.integrate_rk4(&[0.; 5], 0., 1).is_err());
}

#[test]
fn classical_reference_converges_under_time_refinement() {
	let model = quest_cfd::PeriodicBdm1::assemble(0.001).unwrap();
	let initial = [0.7, -0.4, 0.9, 0.3, -0.2];
	let coarse = model.integrate_rk4(&initial, 0.01, 10).unwrap();
	let medium = model.integrate_rk4(&initial, 0.005, 20).unwrap();
	let fine = model.integrate_rk4(&initial, 0.0025, 40).unwrap();
	let distance = |a: &[f64], b: &[f64]| {
		a.iter()
			.zip(b)
			.map(|(x, y)| (x - y).powi(2))
			.sum::<f64>()
			.sqrt()
	};
	let ratio = distance(&coarse, &medium) / distance(&medium, &fine);
	assert!(ratio > 12. && ratio < 20., "RK4 refinement ratio {ratio}");
}

#[test]
fn overflowing_finite_velocity_cannot_escape_coordinate_validation() {
	let model = quest_cfd::PeriodicBdm1::assemble(0.).unwrap();
	assert!(model.coordinates(&[f64::MAX; 12]).is_err());
}

#[test]
fn independent_nodal_and_monomial_assemblies_agree_on_complete_inviscid_dynamics() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	let monomial = quest_cfd::PeriodicBdm1::assemble(0.).unwrap();
	let initial = [0.7, -0.4, 0.9, 0.3, -0.2];
	let c = monomial.coefficients(&initial).unwrap();
	let nodal = SimplexBdm::box_mesh(2, 1, 1., 0., BoxBoundary::Periodic).unwrap();
	let z = nodal
		.project_velocity(|[x, y, _]| {
			let offset = if x >= y { 0 } else { 6 };
			[
				c[offset] + c[offset + 1] * x + c[offset + 2] * y,
				c[offset + 3] + c[offset + 4] * x + c[offset + 5] * y,
				0.,
			]
		})
		.unwrap();
	assert!((nodal.energy(&z).unwrap() - monomial.energy(&initial).unwrap()).abs() < 1e-11);
	let derivative = monomial
		.coefficients(&monomial.drift(&initial).unwrap())
		.unwrap();
	let nodal_drift = nodal.drift(&z).unwrap();
	for [x, y, _] in [[0.8, 0.3, 0.], [0.2, 0.7, 0.]] {
		let offset = if x >= y { 0 } else { 6 };
		let expected = [
			derivative[offset] + derivative[offset + 1] * x + derivative[offset + 2] * y,
			derivative[offset + 3] + derivative[offset + 4] * x + derivative[offset + 5] * y,
			0.,
		];
		let actual = nodal.sample_velocity(&nodal_drift, [x, y, 0.]).unwrap();
		for (a, b) in actual.iter().zip(expected) {
			assert!((a - b).abs() < 1e-10, "{actual:?} versus {expected:?}");
		}
	}
}
