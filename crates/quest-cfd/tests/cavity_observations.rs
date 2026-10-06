#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Assertions on fixed independent reflected velocity fields test component parity and sampled statistics"
)]
use quest_cfd::{CfdError, cavity_observations::cavity_3d_probes};

#[test]
fn reflection_uses_vector_parity_and_records_both_secondary_planes() -> Result<(), CfdError> {
	let result =
		cavity_3d_probes(|points| Ok(points.iter().map(|&[x, y, z]| [x, y, z - 0.5]).collect()))?;
	assert_eq!(result.sample_queries, 810);
	assert_eq!(result.reflection_pairs, 324);
	assert_eq!(result.x_midplane.len(), 81);
	assert_eq!(result.y_midplane.len(), 81);
	assert!(result.reflection_rms_defect < 1e-15);
	assert!(result.reflection_maximum_defect < 1e-15);
	assert!((result.sampled_spanwise_velocity_rms - (1_f64 / 15.).sqrt()).abs() < 1e-15);
	assert!(
		result
			.x_midplane
			.iter()
			.all(|p| p.point[0].to_bits() == 0.5_f64.to_bits())
	);
	assert!(
		result
			.y_midplane
			.iter()
			.all(|p| p.point[1].to_bits() == 0.5_f64.to_bits())
	);
	let broken = cavity_3d_probes(|points| Ok(vec![[0., 0., 2.]; points.len()]))?;
	assert!((broken.reflection_rms_defect - 4.).abs() < 1e-13);
	assert!((broken.reflection_maximum_defect - 4.).abs() < 1e-13);
	Ok(())
}

#[test]
fn malformed_and_nonfinite_samples_reject() {
	assert!(cavity_3d_probes(|_| Ok(Vec::new())).is_err());
	for value in [f64::NAN, f64::INFINITY, f64::MAX] {
		assert!(cavity_3d_probes(|points| Ok(vec![[value; 3]; points.len()])).is_err());
	}
}

#[test]
fn complete_cavity_references_expose_diagnostics_at_both_orders() -> Result<(), CfdError> {
	let base = quest_cfd::cases::box_reference("cavity3d", 100, 1)?;
	let snapshot = base.reference(0.0001, 2)?;
	let diagnostics = snapshot
		.cavity_3d
		.ok_or(CfdError::Assembly("missing cavity probes"))?;
	assert_eq!(diagnostics.sample_queries, 810);
	assert!(diagnostics.sampled_spanwise_velocity_rms.is_finite());
	let high = quest_cfd::cases_high_order::box_reference("cavity3d", 100, 1, 2)?;
	let snapshot = high.reference(0.0001, 2)?;
	assert_eq!(
		snapshot.observables.independent_dimension,
		high.model.dimension()
	);
	assert!(snapshot.observables.cavity_3d.is_some());
	let planar = quest_cfd::cases::box_reference("cavity2d", 100, 1)?.reference(0.0001, 2)?;
	assert!(planar.cavity_3d.is_none());
	Ok(())
}
