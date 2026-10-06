#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Small independent boundary trace and momentum references"
)]
use quest_cfd::simplex::{BoundaryFacet, SimplexBdm};
fn channel() -> SimplexBdm {
	let vertices = [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
	let cells = vec![vec![0, 1, 2], vec![0, 2, 3]];
	let boundaries = [vec![0, 1], vec![1, 2], vec![2, 3], vec![3, 0]]
		.into_iter()
		.enumerate()
		.map(|(i, vertices)| BoundaryFacet {
			vertices,
			velocity: if i == 1 {
				None
			} else {
				Some(vec![[1., 0., 0.]; 2])
			},
			label: format!("edge{i}"),
		})
		.collect::<Vec<_>>();
	SimplexBdm::from_mesh(2, &vertices, &cells, &boundaries, &[], 0.01).unwrap()
}
#[test]
fn changing_normal_boundary_trace_retains_lifting_acceleration_in_momentum() {
	let model = channel();
	let state = (0..model.dimension())
		.map(|i| f64::from(u32::try_from(i).unwrap() + 1) * 0.02)
		.collect::<Vec<_>>();
	let scale = 0.7;
	let rate = 0.3;
	let drift = model
		.drift_with_boundary_scale(&state, scale, rate)
		.unwrap();
	let recovery = model
		.reconstruct_pressure_with_boundary_scale(&state, scale, rate)
		.unwrap();
	assert!(recovery.momentum_residual < 1e-9, "{recovery:?}");
	assert!(recovery.continuity_residual < 1e-10);
	assert!(model.boundary_residual_with_scale(&state, scale).unwrap() < 1e-10);
	let epsilon = 1e-6;
	let advanced = state
		.iter()
		.zip(&drift)
		.map(|(a, b)| a + epsilon * b)
		.collect::<Vec<_>>();
	let before = model
		.coefficients_with_boundary_scale(&state, scale)
		.unwrap();
	let after = model
		.coefficients_with_boundary_scale(&advanced, scale + epsilon * rate)
		.unwrap();
	let acceleration = model
		.coefficients_with_boundary_scale(&drift, rate)
		.unwrap();
	for ((a, b), expected) in after.iter().zip(before).zip(acceleration) {
		assert!(((a - b) / epsilon - expected).abs() < 1e-9);
	}
	let stationary = model.drift(&state).unwrap();
	assert_eq!(
		stationary,
		model.drift_with_boundary_scale(&state, 1., 0.).unwrap()
	);
	assert!(
		model
			.drift_with_boundary_scale(&state, f64::NAN, rate)
			.is_err()
	);
}

#[test]
fn affine_boundary_time_is_external_to_the_full_physical_chart() {
	use mathcore::multivariate::PolynomialLimits;
	use quest_cfd::polynomial::PolynomialOde;
	let model = channel();
	let snapshot = PolynomialOde::from_simplex_bdm1_affine_boundary(
		&model,
		0.4,
		0.3,
		PolynomialLimits::default(),
	)
	.unwrap();
	assert_eq!(snapshot.dynamics.dimension(), model.dimension());
	assert_eq!(snapshot.evidence.physical_dimension, model.dimension());
	let state = vec![0.13; model.dimension()];
	for time in [0., 0.2, 0.8] {
		let direct = model
			.drift_with_boundary_scale(&state, 0.4 + 0.3 * time, 0.3)
			.unwrap();
		let prepared = snapshot.dynamics.drift(time, &state).unwrap();
		for (a, b) in direct.iter().zip(prepared) {
			assert!((a - b).abs() < 1e-10);
		}
	}
}
