#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing
)]
use quest_cfd::simplex::{BoundaryFacet, SimplexBdm};
#[test]
fn inhomogeneous_normal_lifting_preserves_exact_uniform_channel_flow() {
	let vertices = [[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
	let cells = vec![vec![0, 1, 2], vec![0, 2, 3]];
	let boundaries = vec![
		BoundaryFacet {
			vertices: vec![0, 1],
			velocity: Some(vec![[1., 0., 0.]; 2]),
			label: "bottom".into(),
		},
		BoundaryFacet {
			vertices: vec![1, 2],
			velocity: None,
			label: "outlet".into(),
		},
		BoundaryFacet {
			vertices: vec![2, 3],
			velocity: Some(vec![[1., 0., 0.]; 2]),
			label: "top".into(),
		},
		BoundaryFacet {
			vertices: vec![3, 0],
			velocity: Some(vec![[1., 0., 0.]; 2]),
			label: "inlet".into(),
		},
	];
	let model = SimplexBdm::from_mesh(2, &vertices, &cells, &boundaries, &[], 0.01).unwrap();
	let state = model.project_velocity(|_| [1., 0., 0.]).unwrap();
	assert!((model.energy(&state).unwrap() - 0.5).abs() < 1e-10);
	assert!(model.divergence_residual(&state).unwrap() < 1e-10);
	assert!(model.drift(&state).unwrap().iter().all(|v| v.abs() < 1e-9));
	assert!(
		model
			.reconstruct_pressure(&state)
			.unwrap()
			.momentum_residual
			< 1e-9
	);
}

#[test]
fn box_constructor_rejects_unspecified_mixed_boundaries() {
	assert!(SimplexBdm::box_mesh(2, 1, 1., 0.01, quest_cfd::simplex::BoxBoundary::Mixed).is_err());
}

#[test]
fn closed_unequal_volume_pressure_has_physical_zero_mean() {
	let vertices = [[0., 0., 0.], [2., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
	let cells = vec![vec![0, 1, 2], vec![0, 2, 3]];
	let boundaries: Vec<_> = [vec![0, 1], vec![1, 2], vec![2, 3], vec![3, 0]]
		.into_iter()
		.map(|vertices| BoundaryFacet {
			vertices,
			velocity: Some(vec![[0.; 3]; 2]),
			label: "wall".into(),
		})
		.collect();
	let model = SimplexBdm::from_mesh(2, &vertices, &cells, &boundaries, &[], 0.01).unwrap();
	let state = vec![1.; model.dimension()];
	let recovery = model.reconstruct_pressure(&state).unwrap();
	assert!(recovery.pressure_mean_residual < 1e-12, "{recovery:?}");
	assert!(
		0.5f64
			.mul_add(recovery.cell_pressure[1], recovery.cell_pressure[0])
			.abs()
			< 1e-12,
		"{recovery:?}"
	);
	assert!(recovery.momentum_residual < 1e-10, "{recovery:?}");
}
