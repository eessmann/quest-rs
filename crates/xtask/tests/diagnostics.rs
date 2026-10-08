//! Public diagnostic output stays a single JSON document on discovery failures.
#![forbid(unsafe_code)]
use googletest::prelude::*;
use std::process::Command;

#[derive(serde::Deserialize)]
struct Receipt {
	schema_version: u32,
	stages: Vec<Stage>,
}

#[derive(serde::Deserialize)]
struct Stage {
	stage: String,
	status: String,
}

#[gtest]
fn missing_selected_cargo_is_reported_as_a_json_failure() -> googletest::Result<()> {
	let fixture = tempfile::tempdir()?;
	let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
		.args(["native-doctor", "--json"])
		.env("CARGO", fixture.path().join("missing cargo"))
		.output()?;
	expect_false!(output.status.success());
	let receipt: Receipt = serde_json::from_slice(&output.stdout)?;
	expect_eq!(receipt.schema_version, 1);
	expect_true!(receipt.stages.iter().any(|stage| stage.status == "failed"));
	Ok(())
}

#[gtest]
fn missing_selected_quest_is_reported_without_cargo_directives() -> googletest::Result<()> {
	let fixture = tempfile::tempdir()?;
	let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
		.args(["native-doctor", "--json"])
		.env("QUEST_ROOT", fixture.path().join("missing QuEST"))
		.output()?;
	expect_false!(output.status.success());
	let receipt: Receipt = serde_json::from_slice(&output.stdout)?;
	expect_true!(
		receipt
			.stages
			.iter()
			.any(|stage| stage.stage == "native_discovery" && stage.status == "failed")
	);
	Ok(())
}
