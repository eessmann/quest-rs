#![allow(
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	reason = "Bounded CLI JSON resource evidence assertions"
)]
use serde_json::Value;
fn run(arguments: &[&str]) -> Result<Value, Box<dyn std::error::Error>> {
	let output = std::process::Command::new(env!("CARGO_BIN_EXE_quest-cfd"))
		.args(arguments)
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
fn real_smoke_resources_preserve_five_coordinates_and_budget_rejection()
-> Result<(), Box<dyn std::error::Error>> {
	let base = [
		"resource-build",
		"--case",
		"smoke",
		"--configuration-cells",
		"1",
		"--configuration-order",
		"1",
		"--time-cells",
		"1",
		"--horizon",
		"0.001",
	];
	let r = run(&base)?;
	assert_eq!(r["physical_coordinates"], 5);
	assert_eq!(r["all_independent_coordinates_retained"], true);
	assert_eq!(r["quantum_execution"], false);
	assert_eq!(r["history_build_peak_bytes"], Value::Null);
	assert_eq!(r["resources"]["status"], "constructed-resources-counted");
	assert!(
		r["resources"]["encoding"]["descriptor"]["auxiliary_qubits_including_signal"]
			.as_u64()
			.is_some_and(|n| n < 16)
	);
	let mut rejected = base.to_vec();
	rejected.extend(["--count-work", "0"]);
	let r = run(&rejected)?;
	assert_eq!(r["resources"]["status"], "constructed-resources-partial");
	assert!(r["resources"]["encoding"]["count"]["rejection"].is_string());
	assert!(r["resources"]["encoding"]["descriptor"]["normalization"].is_number());
	Ok(())
}
#[test]
fn resource_zero_and_representation_only_estimate_remain_distinct()
-> Result<(), Box<dyn std::error::Error>> {
	let r = run(&[
		"resource-build",
		"--case",
		"burgers",
		"--lift",
		"carleman",
		"--initial-amplitude",
		"0",
	])?;
	assert_eq!(r["status"], "zero-rhs-no-circuit");
	assert_eq!(r["quantum_execution"], false);
	let r = run(&["estimate", "--case", "smoke"])?;
	assert_eq!(r["request"]["auxiliary_qubits"], 16);
	assert!(
		r["estimate"]["status"]
			.as_str()
			.is_some_and(|s| s.contains("estimate-only"))
	);
	assert!(
		r["ancillas"]
			.as_str()
			.is_some_and(|s| s.contains("caller-supplied"))
	);
	Ok(())
}
