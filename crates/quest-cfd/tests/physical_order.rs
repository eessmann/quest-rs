#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Bounded independent complete-space rank, conservation and physical refinement references use indexed arithmetic"
)]
use quest_cfd::{physical_space::PhysicalSpace, simplex::BoxBoundary};
#[test]
fn periodic_two_triangle_bdm2_retains_ten_independent_modes() {
	let space = PhysicalSpace::box_mesh(2, 1, 1., 0., BoxBoundary::Periodic, 2).unwrap();
	let evidence = space.diagnostics();
	assert_eq!(evidence.local_velocity_dimension, 24);
	assert_eq!(evidence.constraint_rank, 14);
	assert_eq!(space.dimension(), 10);
	assert!(evidence.constraint_residual < 1e-11);
	assert!(evidence.mass_orthogonality_residual < 1e-11);
}
#[test]
fn complete_bdm2_convection_is_nonzero_and_energy_conserving_in_2d_and_3d() {
	for dimension in [2, 3] {
		let space =
			PhysicalSpace::box_mesh(dimension, 1, 1., 0., BoxBoundary::Periodic, 2).unwrap();
		let state = (0..space.dimension())
			.map(|i| 0.03 * f64::from(u32::try_from(i + 1).unwrap()).sin())
			.collect::<Vec<_>>();
		let drift = space.drift(&state).unwrap();
		assert!(drift.iter().map(|x| x * x).sum::<f64>() > 1e-8);
		assert!(
			state
				.iter()
				.zip(drift)
				.map(|(a, b)| a * b)
				.sum::<f64>()
				.abs()
				< 1e-10
		);
	}
}

#[test]
fn pressure_has_weighted_gauge_and_recovers_complete_momentum() {
	for dimension in [2, 3] {
		let space =
			PhysicalSpace::box_mesh(dimension, 1, 1., 0.01, BoxBoundary::Periodic, 2).unwrap();
		let state = (0..space.dimension())
			.map(|i| 0.02 * f64::from(u32::try_from(i + 1).unwrap()).cos())
			.collect::<Vec<_>>();
		let report = space.reconstruct_pressure(&state).unwrap();
		assert!(
			report.momentum_residual < 1e-9,
			"{}",
			report.momentum_residual
		);
		assert!(report.gauge_residual < 1e-12);
		assert!(report.continuity_residual < 1e-10);
		assert_eq!(report.pressure_coefficients.len(), space.cell_count());
		assert_eq!(report.pressure_coefficients[0].len(), dimension + 1);
	}
}

#[test]
fn complete_p1_matches_independent_existing_assembly_and_pressure() {
	for dimension in [2, 3] {
		for boundary in [BoxBoundary::Periodic, BoxBoundary::Cavity { lid_speed: 1. }] {
			let old =
				quest_cfd::simplex::SimplexBdm::box_mesh(dimension, 1, 1., 0.03, boundary).unwrap();
			let new = PhysicalSpace::box_mesh(dimension, 1, 1., 0.03, boundary, 1).unwrap();
			let state = (0..old.dimension())
				.map(|i| 0.02 * f64::from(u32::try_from(i + 1).unwrap()).sin())
				.collect::<Vec<_>>();
			let new_state = new.coordinates(&old.coefficients(&state).unwrap()).unwrap();
			let old_drift = old.coefficients(&old.drift(&state).unwrap()).unwrap();
			let new_drift = new.coefficients(&new.drift(&new_state).unwrap()).unwrap();
			assert!(
				old_drift
					.iter()
					.zip(new_drift)
					.map(|(a, b)| (a - b).abs())
					.fold(0., f64::max)
					< 1e-8
			);
			let old_pressure = old.reconstruct_pressure(&state).unwrap();
			let pressure = new.reconstruct_pressure(&new_state).unwrap();
			assert!(
				old_pressure
					.cell_pressure
					.iter()
					.zip(pressure.pressure_coefficients)
					.map(|(a, b)| (a - b[0]).abs())
					.fold(0., f64::max)
					< 1e-8
			);
		}
	}
}
#[test]
fn quadratic_direct_advective_form_and_sip_dissipation_agree() {
	for dimension in [2, 3] {
		let inviscid =
			PhysicalSpace::box_mesh(dimension, 1, 1., 0., BoxBoundary::Periodic, 2).unwrap();
		let viscous =
			PhysicalSpace::box_mesh(dimension, 1, 1., 0.01, BoxBoundary::Periodic, 2).unwrap();
		if dimension == 3 {
			assert_eq!(inviscid.diagnostics().local_velocity_dimension, 180);
			assert_eq!(inviscid.diagnostics().constraint_rank, 95);
			assert_eq!(inviscid.dimension(), 85);
		}
		let state = (0..inviscid.dimension())
			.map(|i| 0.02 * f64::from(u32::try_from(i + 1).unwrap()).cos())
			.collect::<Vec<_>>();
		let conservative = inviscid.drift(&state).unwrap();
		let advective = inviscid.advective_drift_reference(&state).unwrap();
		assert!(
			conservative
				.iter()
				.zip(advective)
				.map(|(a, b)| (a - b).abs())
				.fold(0., f64::max)
				< 1e-10
		);
		let dissipative = viscous.drift(&state).unwrap();
		let dissipation = state
			.iter()
			.zip(dissipative.iter().zip(&conservative))
			.map(|(a, (v, c))| a * (v - c))
			.sum::<f64>();
		assert!(dissipation < -1e-4);
		assert!(inviscid.divergence_residual(&state).unwrap() < 1e-11);
		assert!(inviscid.normal_trace_residual(&state).unwrap() < 1e-11);
	}
}

#[test]
fn physical_h_and_p_refinement_improve_periodic_manufactured_projection() {
	let exact = |point: [f64; 3]| [(std::f64::consts::TAU * point[1]).cos(), 0., 0.];
	for dimension in [2, 3] {
		let mut errors = Vec::new();
		for (cells, order) in [(1, 1), (2, 1), (1, 2), (2, 2), (4, 2)] {
			if dimension == 3 && cells > 1 && order == 2 {
				continue;
			}
			let space =
				PhysicalSpace::box_mesh(dimension, cells, 1., 0., BoxBoundary::Periodic, order)
					.unwrap();
			let state = space.project_velocity(exact).unwrap();
			let error = space.velocity_error_l2(&state, exact).unwrap();
			eprintln!("projection d={dimension} n={cells} p={order} error={error:.12e}");
			errors.push(error);
		}
		assert!(errors[1] < 0.9 * errors[0]);
		assert!(errors[2] < 0.9 * errors[0]);
		if dimension == 2 {
			assert!(errors[3] < errors[2]);
			assert!(errors[3] < errors[1]);
			assert!(errors[4] < 0.2 * errors[3]);
		}
	}
}
#[test]
fn reference_admission_and_rk4_refinement_are_explicit() {
	assert!(PhysicalSpace::box_mesh(3, 2, 1., 0., BoxBoundary::Periodic, 2).is_err());
	assert!(PhysicalSpace::box_mesh(2, u32::MAX, 1., 0., BoxBoundary::Periodic, 2).is_err());
	let space = PhysicalSpace::box_mesh(2, 1, 1., 0., BoxBoundary::Periodic, 2).unwrap();
	let state = (0..space.dimension())
		.map(|i| 0.1 * f64::from(u32::try_from(i + 1).unwrap()).sin())
		.collect::<Vec<_>>();
	let reference = space.integrate_rk4(&state, 0.2, 128).unwrap();
	let mut errors = Vec::new();
	for steps in [2, 4, 8] {
		let result = space.integrate_rk4(&state, 0.2, steps).unwrap();
		errors.push(
			result
				.iter()
				.zip(&reference)
				.map(|(a, b)| (a - b).powi(2))
				.sum::<f64>()
				.sqrt(),
		);
	}
	assert!(errors[1] < 0.1 * errors[0]);
	assert!(errors[2] < 0.1 * errors[1]);
	assert!(space.integrate_rk4(&state, 1., usize::MAX).is_err());
	assert!(space.coordinates(&[1.; 23]).is_err());
}

#[test]
fn quadratic_cavity_lid_forces_the_complete_space_and_recovers_pressure() {
	for dimension in [2, 3] {
		let space = PhysicalSpace::box_mesh(
			dimension,
			1,
			1.,
			0.01,
			BoxBoundary::Cavity { lid_speed: 1. },
			2,
		)
		.unwrap();
		assert_eq!(space.dimension(), if dimension == 2 { 4 } else { 49 });
		let rest = vec![0.; space.dimension()];
		let force = space.drift(&rest).unwrap();
		assert!(force.iter().map(|x| x * x).sum::<f64>() > 1e-4);
		let advanced = space.integrate_rk4(&rest, 0.001, 2).unwrap();
		assert!(space.energy(&advanced).unwrap() > 1e-9);
		assert!(space.gradient_dissipation(&advanced).unwrap() > 0.);
		assert!(
			space
				.reconstruct_pressure(&advanced)
				.unwrap()
				.momentum_residual
				< 1e-9
		);
	}
}
