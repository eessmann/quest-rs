#![allow(
	clippy::panic_in_result_fn,
	reason = "Explicit reference work-admission boundaries"
)]
use quest_cfd::{physical_space::PhysicalSpace, simplex::BoxBoundary};
#[test]
fn explicit_work_allowance_preserves_complete_trajectory_and_default()
-> Result<(), Box<dyn std::error::Error>> {
	let space = PhysicalSpace::box_mesh(2, 1, 1., 0.01, BoxBoundary::Cavity { lid_speed: 1. }, 2)?;
	let initial = vec![0.; space.dimension()];
	let work = space.integration_work_bound(2)?;
	assert!(work > 0);
	assert!(
		space
			.integrate_rk4_with_work_limit(
				&initial,
				0.01,
				2,
				work.checked_sub(1).ok_or("positive work")?
			)
			.is_err()
	);
	let custom = space.integrate_rk4_with_work_limit(&initial, 0.01, 2, work)?;
	let ordinary = space.integrate_rk4(&initial, 0.01, 2)?;
	assert_eq!(custom, ordinary);
	assert_eq!(custom.len(), space.dimension());
	assert!(space.integration_work_bound(usize::MAX).is_err());
	assert!(
		space
			.integrate_rk4_with_work_limit(&initial, 0.01, 0, usize::MAX)
			.is_err()
	);
	Ok(())
}
#[test]
fn higher_order_snapshot_records_actual_full_work() -> Result<(), Box<dyn std::error::Error>> {
	let case = quest_cfd::cases_high_order::box_reference("tgv2d", 100, 1, 2)?;
	let expected = case.model.integration_work_bound(2)?;
	let report = case.reference_with_work_limit(0.001, 2, expected)?;
	assert_eq!(report.integration_modeled_work, expected);
	assert_eq!(report.integration_work_limit, expected);
	assert_eq!(
		report.observables.independent_dimension,
		case.model.dimension()
	);
	assert!(
		case.reference_with_work_limit(0.001, 2, expected.checked_sub(1).ok_or("positive work")?)
			.is_err()
	);
	Ok(())
}

#[test]
fn cli_rejects_ignored_allowances_and_records_supported_limit()
-> Result<(), Box<dyn std::error::Error>> {
	let binary = env!("CARGO_BIN_EXE_quest-cfd");
	let unsupported = std::process::Command::new(binary)
		.args([
			"reference",
			"--case",
			"burgers",
			"--max-classical-work",
			"1",
		])
		.output()?;
	assert!(!unsupported.status.success());
	let base = [
		"reference",
		"--case",
		"tgv2d",
		"--physical-order",
		"2",
		"--dt",
		"0.001",
		"--steps",
		"2",
		"--max-classical-work",
	];
	let rejected = std::process::Command::new(binary)
		.args(base)
		.arg("1")
		.output()?;
	assert!(!rejected.status.success());
	let admitted = std::process::Command::new(binary)
		.args(base)
		.arg("1000000000")
		.output()?;
	assert!(admitted.status.success());
	let value: serde_json::Value = serde_json::from_slice(&admitted.stdout)?;
	assert_eq!(
		value
			.pointer("/reference/integration_work_limit")
			.and_then(serde_json::Value::as_u64),
		Some(1_000_000_000)
	);
	assert!(
		value
			.pointer("/reference/integration_modeled_work")
			.and_then(serde_json::Value::as_u64)
			.is_some_and(|n| n > 0)
	);
	Ok(())
}
