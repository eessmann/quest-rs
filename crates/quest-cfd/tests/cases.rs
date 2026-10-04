#![allow(
	clippy::unwrap_used,
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops
)]
#[test]
fn every_requested_physical_case_family_is_available() {
	let names = quest_cfd::cases::family_names();
	for required in [
		"tgv2d",
		"tgv3d",
		"cavity2d",
		"cavity3d",
		"shedding2d",
		"shedding3d",
	] {
		assert!(names.contains(&required), "missing {required}");
	}
}

#[test]
fn every_supported_box_case_runs_a_real_full_dg_reference() {
	for (id, reynolds) in [
		("tgv2d", 100),
		("tgv3d", 100),
		("tgv3d", 1600),
		("cavity2d", 100),
		("cavity2d", 1000),
		("cavity3d", 100),
		("cavity3d", 1000),
	] {
		let case = quest_cfd::cases::box_reference(id, reynolds, 1).unwrap();
		let snapshot = case.reference(0.001, 2).unwrap();
		assert_eq!(snapshot.independent_dimension, case.model.dimension());
		assert!(snapshot.pressure.momentum_residual < 1e-8);
		assert!(snapshot.pressure.continuity_residual < 1e-9);
		if id.starts_with("cavity") {
			assert!(snapshot.mean_kinetic_energy > 0.);
		}
		assert_eq!(snapshot.analytic_velocity_error_l2.is_some(), id == "tgv2d");
	}
}

#[test]
fn unsupported_cylinder_execution_and_unfrozen_reynolds_are_explicit() {
	for (id, reynolds) in [("shedding2d", 100), ("shedding3d", 300)] {
		assert!(quest_cfd::cases::box_reference(id, reynolds, 1).is_err());
	}
	assert!(quest_cfd::cases::box_reference("tgv2d", 1600, 1).is_err());
	assert!(quest_cfd::cases::manifest("unknown").is_err());
}
