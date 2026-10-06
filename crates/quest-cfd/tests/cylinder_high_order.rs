//! Fixed complete P2 cylinder snapshot consumer, not a developed-flow benchmark.
#![allow(
	clippy::panic_in_result_fn,
	reason = "Independent bounded fixture assertions fail the test directly"
)]
use quest_cfd::cylinder_high_order::{CylinderPhysicalLimits, CylinderPhysicalSource};
#[test]
fn explicit_rectangle_p2_snapshot_retains_complete_source_and_resource_rejections()
-> Result<(), Box<dyn std::error::Error>> {
	let source = CylinderPhysicalSource::new(
		4,
		1,
		100,
		CylinderPhysicalLimits {
			max_work: 2_000_000_000,
			..Default::default()
		},
	)?;
	let geometry = serde_json::to_value(source.geometry_export())?;
	assert_eq!(
		geometry["source_policy"],
		"explicit-rectangle-corner-priority-v1"
	);
	assert_eq!(source.model().dimension(), 54);
	let mut prepared = source.prepare()?;
	let initial = prepared.initial_state().to_vec();
	let before = prepared.resources().cumulative_work;
	assert!(prepared.snapshot_at(&initial, f64::NAN).is_err());
	assert!(prepared.snapshot_at(&initial[..1], 0.).is_err());
	assert_eq!(prepared.resources().cumulative_work, before);
	let snapshot = prepared.snapshot_at(&initial, 0.)?;
	assert!(snapshot.pressure.continuity_residual < 1e-9);
	assert!(snapshot.pressure.momentum_residual < 1e-9);
	assert!(snapshot.pressure.normalization_residual.is_none());
	assert!(snapshot.mean_kinetic_energy.is_finite());
	assert!(snapshot.cylinder_force.iter().all(|x| x.is_finite()));
	assert!(snapshot.resources.cumulative_work <= 2_000_000_000);
	assert!(
		prepared.snapshot_at(&initial, 0.).is_err(),
		"repeated queries must consume the same workflow allowance"
	);
	let default = CylinderPhysicalSource::new(4, 1, 100, CylinderPhysicalLimits::default())?;
	assert!(
		default.prepare_with_receipt().outcome.is_err(),
		"1b workflow cap must not reset per stage"
	);
	Ok(())
}

#[test]
fn generation_rejects_live_external_payload_before_allocating_source() {
	let limits = CylinderPhysicalLimits {
		max_bytes: 128 * 1024,
		mesh: quest_cfd::physical_space::PhysicalMeshLimits {
			external_retained_bytes: 1,
			..Default::default()
		},
		..Default::default()
	};
	let attempt = CylinderPhysicalSource::new_with_receipt(4, 1, 100, limits);
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.resources.last_stage, "generator");
	assert_eq!(attempt.resources.cumulative_work, 0);
	assert_eq!(attempt.resources.peak_bytes, 128 * 1024 + 1);
}

#[test]
fn unsupported_controls_and_generation_work_reject_without_completion() {
	for (angular, layers, reynolds) in [(3, 1, 100), (4, 0, 100), (4, 1, 20), (64, 8, 100)] {
		let attempt = CylinderPhysicalSource::new_with_receipt(
			angular,
			layers,
			reynolds,
			CylinderPhysicalLimits::default(),
		);
		assert!(attempt.outcome.is_err());
		assert_eq!(attempt.resources.cumulative_work, 0);
	}
	let attempt = CylinderPhysicalSource::new_with_receipt(
		4,
		1,
		100,
		CylinderPhysicalLimits {
			max_work: 1,
			..Default::default()
		},
	);
	assert!(attempt.outcome.is_err());
	assert_eq!(attempt.resources.last_stage, "generator");
	assert!(
		attempt
			.resources
			.attempted_stage_work
			.is_some_and(|work| work > 1)
	);
}
