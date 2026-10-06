#![allow(
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	reason = "Bounded JSON workflow assertions"
)]
use serde_json::Value;
fn run(args: &[&str]) -> Result<Value, Box<dyn std::error::Error>> {
	let output = std::process::Command::new(env!("CARGO_BIN_EXE_quest-cfd"))
		.args(args)
		.output()?;
	assert!(
		output.status.success(),
		"{} {}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	Ok(serde_json::from_slice(&output.stdout)?)
}
#[test]
fn burgers_reference_and_carleman_history_preserve_all_eight_coordinates()
-> Result<(), Box<dyn std::error::Error>> {
	let reference = run(&["reference", "--case", "burgers"])?;
	assert_eq!(reference["quantum_execution"], false);
	assert_eq!(reference["reference"]["physical_dimension"], 8);
	assert_eq!(reference["reference"]["horizon"], 0.1);
	let build = run(&[
		"build",
		"--case",
		"burgers",
		"--lift",
		"carleman",
		"--carleman-order",
		"2",
	])?;
	assert_eq!(build["independent_coordinates"], 8);
	assert_eq!(build["lift_dimension"], 44);
	assert_eq!(build["history_dimension"], 176);
	assert_eq!(build["quantum_execution"], false);
	assert_eq!(build["theorem_convergence_certified"], false);
	assert_eq!(build["horizon"], 0.1);
	Ok(())
}
#[test]
fn classical_carleman_reference_reports_full_dg_comparison()
-> Result<(), Box<dyn std::error::Error>> {
	let result = run(&[
		"reference",
		"--case",
		"burgers",
		"--lift",
		"carleman",
		"--carleman-order",
		"2",
		"--dt",
		"0.001",
		"--steps",
		"10",
	])?;
	assert_eq!(result["quantum_execution"], false);
	assert_eq!(
		result["carleman_reference"]["physical_coordinates"]
			.as_array()
			.map(Vec::len),
		Some(8)
	);
	assert!(
		result["carleman_reference"]["absolute_coordinate_error"]
			.as_f64()
			.is_some_and(|v| v < 1e-7)
	);
	Ok(())
}

#[test]
fn carleman_estimate_and_zero_trajectory_do_not_claim_quantum_execution()
-> Result<(), Box<dyn std::error::Error>> {
	let estimate = run(&[
		"estimate",
		"--case",
		"smoke",
		"--lift",
		"carleman",
		"--carleman-order",
		"4",
	])?;
	assert_eq!(estimate["estimate"]["configuration_dimension"], "125");
	assert_eq!(estimate["estimate"]["independent_dimension"], 5);
	let zero = run(&[
		"solve",
		"--case",
		"burgers",
		"--lift",
		"carleman",
		"--initial-amplitude",
		"0",
	])?;
	assert_eq!(zero["status"], "exact-zero-trajectory");
	assert_eq!(zero["quantum_execution"], false);
	assert_eq!(
		zero["physical_coordinates"],
		serde_json::json!([0., 0., 0., 0., 0., 0., 0., 0.])
	);
	Ok(())
}

#[test]
fn kdv_keeps_evolving_auxiliary_coordinates_in_build_and_estimate()
-> Result<(), Box<dyn std::error::Error>> {
	let estimate = run(&[
		"estimate",
		"--case",
		"kdv",
		"--lift",
		"carleman",
		"--carleman-order",
		"2",
	])?;
	assert_eq!(estimate["estimate"]["independent_dimension"], 24);
	assert_eq!(estimate["estimate"]["configuration_dimension"], "324");
	let build = run(&[
		"build",
		"--case",
		"kdv",
		"--lift",
		"carleman",
		"--carleman-order",
		"1",
	])?;
	assert_eq!(build["independent_coordinates"], 24);
	assert_eq!(build["physical_order"], 2);
	assert_eq!(build["physical_diagnostics"]["auxiliary_dimension"], 12);
	assert_eq!(build["horizon"], 0.1);
	assert_eq!(build["history_dimension"], 96);
	assert_eq!(build["quantum_execution"], false);
	Ok(())
}

#[test]
fn physical_reference_rejects_unsupported_order_instead_of_silently_using_bdm1()
-> Result<(), Box<dyn std::error::Error>> {
	for case in ["tgv2d", "cavity3d", "shedding2d"] {
		let output = std::process::Command::new(env!("CARGO_BIN_EXE_quest-cfd"))
			.args([
				"reference",
				"--case",
				case,
				"--physical-order",
				"3",
				"--steps",
				"1",
			])
			.output()?;
		assert!(
			!output.status.success(),
			"unsupported order silently accepted for {case}"
		);
		let result: Value = serde_json::from_slice(&output.stdout)?;
		assert_eq!(result["status"], "rejected-or-failed");
	}
	Ok(())
}

#[test]
fn bdm2_workflows_retain_the_complete_higher_order_space() -> Result<(), Box<dyn std::error::Error>>
{
	let reference = run(&["reference", "--case", "tgv2d", "--physical-order", "2"])?;
	assert_eq!(reference["reference"]["independent_dimension"], 10);
	assert_eq!(reference["reference"]["physical_order"], 2);
	assert_eq!(reference["quantum_execution"], false);
	let estimate = run(&[
		"estimate",
		"--case",
		"tgv3d",
		"--physical-order",
		"2",
		"--lift",
		"carleman",
		"--carleman-order",
		"1",
	])?;
	assert_eq!(estimate["estimate"]["independent_dimension"], 85);
	let build = run(&[
		"build",
		"--case",
		"tgv2d",
		"--physical-order",
		"2",
		"--lift",
		"carleman",
		"--carleman-order",
		"1",
	])?;
	assert_eq!(build["independent_coordinates"], 10);
	assert_eq!(build["lift_dimension"], 10);
	assert_eq!(build["quantum_execution"], false);
	Ok(())
}

#[test]
fn requested_carleman_certificate_rejects_mean_modes_and_preserves_exact_zero()
-> Result<(), Box<dyn std::error::Error>> {
	let output = std::process::Command::new(env!("CARGO_BIN_EXE_quest-cfd"))
		.args([
			"build",
			"--case",
			"smoke",
			"--lift",
			"carleman",
			"--certify-carleman",
		])
		.output()?;
	assert!(!output.status.success());
	let value: Value = serde_json::from_slice(&output.stdout)?;
	assert_eq!(value["status"], "rejected-or-failed");
	assert!(
		value["reason"]
			.as_str()
			.is_some_and(|reason| reason.contains("logarithmic contraction"))
	);
	let zero = run(&[
		"solve",
		"--case",
		"burgers",
		"--lift",
		"carleman",
		"--initial-amplitude",
		"0",
		"--certify-carleman",
	])?;
	assert_eq!(zero["status"], "exact-zero-trajectory");
	assert_eq!(zero["quantum_execution"], false);
	Ok(())
}

#[test]
fn certificate_request_cannot_be_ignored_by_non_carleman_or_estimate_workflows()
-> Result<(), Box<dyn std::error::Error>> {
	for args in [
		vec![
			"reference",
			"--case",
			"burgers",
			"--steps",
			"1",
			"--certify-carleman",
		],
		vec![
			"estimate",
			"--case",
			"burgers",
			"--lift",
			"carleman",
			"--certify-carleman",
		],
	] {
		let output = std::process::Command::new(env!("CARGO_BIN_EXE_quest-cfd"))
			.args(args)
			.output()?;
		assert!(!output.status.success());
		let result: Value = serde_json::from_slice(&output.stdout)?;
		assert_eq!(result["status"], "rejected-or-failed");
		assert!(
			result["reason"]
				.as_str()
				.is_some_and(|reason| reason.contains("requires --lift carleman"))
		);
	}
	Ok(())
}
