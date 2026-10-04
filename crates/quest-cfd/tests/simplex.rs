#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops
)]

#[test]
fn periodic_refinement_retains_every_bdm1_degree() {
	// N triangles: periodic BDM1 has 3N global H(div) coefficients and N-1 divergence constraints.
	assert_eq!(
		quest_cfd::simplex::box_chart_dimension(2, 1, true).unwrap(),
		5
	);
	assert_eq!(
		quest_cfd::simplex::box_chart_dimension(2, 2, true).unwrap(),
		17
	);
	// Six tetrahedra, 12 periodically identified triangular facets * 3 traces - (6-1).
	assert_eq!(
		quest_cfd::simplex::box_chart_dimension(3, 1, true).unwrap(),
		31
	);
}

#[test]
fn full_3d_convection_and_sip_obey_energy_and_continuity() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	for dimension in [2, 3] {
		let model = SimplexBdm::box_mesh(dimension, 1, 1., 0., BoxBoundary::Periodic).unwrap();
		let state: Vec<_> = (0..model.dimension())
			.map(|i| {
				if i % 3 == 0 {
					0.1
				} else if i % 3 == 1 {
					-0.03
				} else {
					0.07
				}
			})
			.collect();
		let drift = model.drift(&state).unwrap();
		assert!(drift.iter().map(|v| v * v).sum::<f64>() > 1e-6);
		assert!(
			state
				.iter()
				.zip(&drift)
				.map(|(a, b)| a * b)
				.sum::<f64>()
				.abs()
				< 1e-10
		);
		assert!(model.divergence_residual(&state).unwrap() < 1e-10);
		let viscous = SimplexBdm::box_mesh(dimension, 1, 1., 0.01, BoxBoundary::Periodic).unwrap();
		assert!(
			state
				.iter()
				.zip(viscous.drift(&state).unwrap())
				.map(|(a, b)| a * b)
				.sum::<f64>()
				< 0.
		);
		let constant = model.project_velocity(|_| [1., 0., 0.]).unwrap();
		assert!((model.energy(&constant).unwrap() - 0.5).abs() < 1e-12);
		assert!(
			viscous
				.drift(&constant)
				.unwrap()
				.iter()
				.all(|v| v.abs() < 1e-10)
		);
	}
}

#[test]
fn moving_lid_drives_complete_2d_and_3d_cavity_from_rest() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	for dimension in [2, 3] {
		let model = SimplexBdm::box_mesh(
			dimension,
			1,
			1.,
			0.01,
			BoxBoundary::Cavity { lid_speed: 1. },
		)
		.unwrap();
		let initial = vec![0.; model.dimension()];
		assert!(
			model
				.drift(&initial)
				.unwrap()
				.iter()
				.map(|v| v * v)
				.sum::<f64>()
				> 1e-5
		);
		let end = model.integrate_rk4(&initial, 0.001, 5).unwrap();
		assert!(model.energy(&end).unwrap() > 1e-10);
		assert!(model.divergence_residual(&end).unwrap() < 1e-10);
	}
}

#[test]
fn box_mesh_rejects_unbudgeted_full_spaces_instead_of_reducing_them() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	assert!(SimplexBdm::box_mesh(3, 3, 1., 0.01, BoxBoundary::Periodic).is_err());
	assert!(SimplexBdm::box_mesh(4, 1, 1., 0.01, BoxBoundary::Periodic).is_err());
	assert!(SimplexBdm::box_mesh(2, u32::MAX, 1., 0.01, BoxBoundary::Periodic).is_err());
}

#[test]
fn full_simplex_pressure_recovery_closes_momentum_and_gauge() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	for dimension in [2, 3] {
		for boundary in [BoxBoundary::Periodic, BoxBoundary::Cavity { lid_speed: 1. }] {
			let model = SimplexBdm::box_mesh(dimension, 1, 1., 0.01, boundary).unwrap();
			let state = vec![0.1; model.dimension()];
			let p = model.reconstruct_pressure(&state).unwrap();
			assert!(
				p.momentum_residual < 1e-9,
				"{dimension} {boundary:?}: {p:?}"
			);
			assert!(p.pressure_mean_residual < 1e-12);
			assert!(p.continuity_residual < 1e-10);
		}
	}
}

#[test]
fn taylor_green_projection_converges_under_independent_mesh_refinement() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	let field = |p: [f64; 3]| [p[0].sin() * p[1].cos(), -p[0].cos() * p[1].sin(), 0.];
	let mut errors = Vec::new();
	for n in [2, 4, 8] {
		let model =
			SimplexBdm::box_mesh(2, n, std::f64::consts::TAU, 0.01, BoxBoundary::Periodic).unwrap();
		let state = model.project_velocity(field).unwrap();
		let error = model.velocity_error_l2(&state, field).unwrap();
		errors.push(error);
		assert!(model.divergence_residual(&state).unwrap() < 1e-9);
	}
	assert!(errors[1] < 0.7 * errors[0], "{errors:?}");
	assert!(errors[2] < 0.4 * errors[1], "{errors:?}");
}

#[test]
fn physical_probes_and_enstrophy_recover_uniform_flow() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm};
	let model = SimplexBdm::box_mesh(3, 1, 2., 0., BoxBoundary::Periodic).unwrap();
	let state = model.project_velocity(|_| [1., -2., 3.]).unwrap();
	let u = model.sample_velocity(&state, [0.7, 0.9, 1.1]).unwrap();
	for (actual, expected) in u.iter().zip([1., -2., 3.]) {
		assert!((actual - expected).abs() < 1e-12);
	}
	assert!(model.enstrophy(&state).unwrap() < 1e-20);
	assert!(model.sample_velocity(&state, [3., 0., 0.]).is_err());
}

#[test]
fn topology_formula_matches_every_small_box_chart() {
	use quest_cfd::simplex::{BoxBoundary, SimplexBdm, box_chart_dimensions};
	for dimension in [2, 3] {
		for periodic in [false, true] {
			let model = SimplexBdm::box_mesh(
				dimension,
				1,
				1.,
				0.,
				if periodic {
					BoxBoundary::Periodic
				} else {
					BoxBoundary::Cavity { lid_speed: 0. }
				},
			)
			.unwrap();
			let (local, rank) = box_chart_dimensions(dimension, 1, periodic).unwrap();
			assert_eq!(local, model.diagnostics().local_velocity_dimension);
			assert_eq!(rank, model.diagnostics().constraint_rank);
		}
	}
	let (local, rank) = box_chart_dimensions(3, 100, true).unwrap();
	assert_eq!(local, 72_000_000);
	assert_eq!(local - rank, 30_000_001);
}
