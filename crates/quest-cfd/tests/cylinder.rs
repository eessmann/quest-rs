#![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]
#[test]
fn dfg_channel_assembles_nonzero_lift_and_balanced_open_flux() {
	let reference = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1).unwrap();
	assert!(reference.model.dimension() > 5);
	assert!(reference.model.energy(&reference.initial_state).unwrap() > 0.);
	assert!(
		reference
			.model
			.boundary_residual(&reference.initial_state)
			.unwrap()
			< 1e-8
	);
	assert!(
		reference
			.model
			.divergence_residual(&reference.initial_state)
			.unwrap()
			< 1e-8
	);
}

#[test]
fn dfg_cylinder_executes_full_dg_reference_with_geometry_evidence() {
	{
		let (id, reynolds) = ("shedding2d", 100);
		let reference = quest_cfd::cylinder::reference(id, reynolds, 4, 1, 1).unwrap();
		let snapshot = reference.reference(1e-6, 2).unwrap();
		assert_eq!(snapshot.independent_dimension, reference.model.dimension());
		assert!(snapshot.maximum_geometry_deviation > 0.);
		assert!(snapshot.maximum_geometry_deviation < 0.5);
		assert!(snapshot.pressure.momentum_residual < 1e-7, "{snapshot:?}");
		assert!(snapshot.boundary_residual < 1e-8);
		assert!(snapshot.drag_coefficient.is_finite());
		assert!(snapshot.lift_coefficient.is_finite());
	}
}

#[test]
fn cylinder_angular_refinement_reduces_declared_geometry_error() {
	let coarse = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1).unwrap();
	let fine = quest_cfd::cylinder::reference("shedding2d", 100, 8, 1, 1).unwrap();
	assert!(fine.maximum_geometry_deviation < coarse.maximum_geometry_deviation);
	assert!(fine.model.dimension() > coarse.model.dimension());
	assert!(quest_cfd::cylinder::reference("shedding3d", 300, 8, 2, 2).is_err());
}

#[test]
fn approved_3d_cylinder_is_rejected_without_its_actual_boundary_dynamics() {
	let error = quest_cfd::cylinder::reference("shedding3d", 300, 4, 1, 1).unwrap_err();
	assert!(matches!(error, quest_cfd::CfdError::Unsupported(_)));
}

#[test]
fn topology_counts_match_actual_full_dfg_rank_and_scale_without_dense_assembly() {
	let case = quest_cfd::cylinder::reference("shedding2d", 100, 4, 1, 1).unwrap();
	let (local, rank) =
		quest_cfd::cylinder::cylinder_chart_dimensions("shedding2d", 4, 1, 1).unwrap();
	assert_eq!(local, case.model.diagnostics().local_velocity_dimension);
	assert_eq!(rank, case.model.diagnostics().constraint_rank);
	let (large_local, large_rank) =
		quest_cfd::cylinder::cylinder_chart_dimensions("shedding3d", 32, 4, 4).unwrap();
	assert!(large_local > 768);
	assert!(large_rank < large_local);
}
