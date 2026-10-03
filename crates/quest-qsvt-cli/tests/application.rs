use clap::Parser;
use googletest::prelude::*;
use quest_qsvt_cli::Cli;

#[gtest]
fn failed_trace_export_retains_primary_dispatch_error() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let trace = directory.path().join("missing-parent").join("trace.json");
	let result = Cli::try_parse_from([
		std::ffi::OsStr::new("qsvt"),
		std::ffi::OsStr::new("--trace"),
		trace.as_os_str(),
		std::ffi::OsStr::new("catalog"),
		std::ffi::OsStr::new("check"),
		std::ffi::OsStr::new("--tolerance"),
		std::ffi::OsStr::new("0"),
	])?
	.run();
	let error = result.expect_err("invalid tolerance must fail");
	let quest_qsvt_cli::Error::DispatchAndTrace { primary, trace } = &error else {
		return fail!("primary computation error was replaced");
	};
	expect_true!(matches!(
		primary.as_ref(),
		quest_qsvt_cli::Error::Input("positive finite tolerance required")
	));
	expect_true!(matches!(trace.as_ref(), quest_qsvt_cli::Error::Io(_)));
	expect_true!(
		error
			.to_string()
			.contains("positive finite tolerance required")
	);
	expect_true!(error.to_string().contains("trace"));
	Ok(())
}

#[gtest]
fn trace_outer_scope_is_application_dispatch_after_preflight() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let path = directory.path().join("trace.json");
	let report = Cli::try_parse_from([
		std::ffi::OsStr::new("qsvt"),
		std::ffi::OsStr::new("--trace"),
		path.as_os_str(),
		std::ffi::OsStr::new("catalog"),
		std::ffi::OsStr::new("list"),
	])?
	.run()?;
	let trace: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
	let events = trace
		.get("traceEvents")
		.and_then(serde_json::Value::as_array)
		.ok_or_else(|| std::io::Error::other("trace events"))?;
	let names = events
		.iter()
		.filter_map(|event| event.get("name").and_then(serde_json::Value::as_str))
		.collect::<Vec<_>>();
	expect_that!(
		names
			.iter()
			.filter(|name| **name == "application_dispatch")
			.count(),
		eq(1)
	);
	expect_false!(names.contains(&"end_to_end"));
	let total = report
		.pointer("/timings_seconds/total")
		.and_then(serde_json::Value::as_f64)
		.ok_or_else(|| std::io::Error::other("application total"))?;
	let pool = report
		.pointer("/timings_seconds/worker_pool")
		.and_then(serde_json::Value::as_f64)
		.ok_or_else(|| std::io::Error::other("worker admission duration"))?;
	let teardown = report
		.pointer("/timings_seconds/worker_pool_teardown")
		.and_then(serde_json::Value::as_f64)
		.ok_or_else(|| std::io::Error::other("worker teardown duration"))?;
	expect_that!(total, ge(std::ops::Add::add(pool, teardown)));
	Ok(())
}

#[gtest]
fn failed_worker_preflight_produces_no_dispatch_trace() -> googletest::Result<()> {
	let directory = tempfile::tempdir()?;
	let path = directory.path().join("trace.json");
	let result = Cli::try_parse_from([
		std::ffi::OsStr::new("qsvt"),
		std::ffi::OsStr::new("--trace"),
		path.as_os_str(),
		std::ffi::OsStr::new("--workers"),
		std::ffi::OsStr::new("2"),
		std::ffi::OsStr::new("catalog"),
		std::ffi::OsStr::new("list"),
	])?
	.run();
	expect_true!(result.is_err());
	expect_false!(path.exists());
	Ok(())
}

#[gtest]
fn worker_selection_parses_auto_and_preserves_the_serial_default() -> googletest::Result<()> {
	let default = Cli::try_parse_from(["qsvt", "catalog", "list"])?;
	expect_that!(default.workers.resolve()?.get(), eq(1));
	let auto = Cli::try_parse_from(["qsvt", "--workers", "auto", "catalog", "check"])?;
	expect_that!(auto.workers, eq(quest_qsvt_cli::Workers::Auto));
	expect_that!(
		auto.workers.resolve()?,
		eq(std::thread::available_parallelism()?)
	);
	for value in ["0", "-1", "automatic", "1.5"] {
		expect_true!(
			Cli::try_parse_from(["qsvt", "--workers", value, "catalog", "check"]).is_err()
		);
	}
	Ok(())
}

#[cfg(feature = "rayon")]
#[gtest]
fn automatic_local_workers_report_the_resolved_pool_size() -> googletest::Result<()> {
	let expected = u64::try_from(std::thread::available_parallelism()?.get())?;
	let report = Cli::try_parse_from([
		"qsvt",
		"--workers",
		"auto",
		"catalog",
		"check",
		"--kappa",
		"5",
		"--epsilon",
		"0.1",
	])?
	.run()?;
	expect_that!(
		report
			.pointer("/parallelism/workers")
			.and_then(serde_json::Value::as_u64),
		some(eq(expected))
	);
	expect_that!(
		report
			.pointer("/timings_seconds/worker_pool")
			.and_then(serde_json::Value::as_f64),
		some(ge(0.0))
	);
	Ok(())
}

#[gtest]
fn catalog_lists_exact_twenty_one_families_and_rejects_nearby_selection() -> googletest::Result<()>
{
	let report = Cli::try_parse_from(["qsvt", "catalog", "list"])?.run()?;
	expect_eq!(
		report
			.pointer("/families/0/source/dataset")
			.and_then(serde_json::Value::as_str),
		Some("inverse")
	);
	expect_eq!(
		report
			.pointer("/families/0/source/sha256")
			.and_then(serde_json::Value::as_str),
		Some("dccb518a24395d73af9ab701a922431f4600f553e904cc57507b49863a6d30a3")
	);
	expect_true!(report.pointer("/families/0/source_revision").is_none());
	expect_eq!(
		report
			.pointer("/families")
			.unwrap_or(&serde_json::Value::Null)
			.as_array()
			.map(Vec::len),
		Some(21)
	);
	let missing = Cli::try_parse_from([
		"qsvt",
		"catalog",
		"list",
		"--kappa",
		"2",
		"--epsilon",
		"0.123",
	])?;
	expect_true!(missing.run().is_err());
	Ok(())
}

#[gtest]
fn canonical_synthesis_exports_reimportable_frozen_phases_with_stage_timings()
-> googletest::Result<()> {
	let dir = tempfile::tempdir()?;
	let input = dir.path().join("polynomial.json");
	let output = dir.path().join("phases.json");
	std::fs::write(&input, r#"{"basis":"Chebyshev","coefficients":[0.0,0.25]}"#)?;
	let cli = Cli::try_parse_from([
		"qsvt",
		"synthesize",
		"--export",
		"sequence",
		"--input",
		input
			.to_str()
			.ok_or_else(|| std::io::Error::other("path"))?,
		"--output",
		output
			.to_str()
			.ok_or_else(|| std::io::Error::other("path"))?,
	])?;
	let report = cli.run()?;
	expect_eq!(
		report
			.pointer("/construction")
			.unwrap_or(&serde_json::Value::Null)
			.as_str(),
		Some("binary64")
	);
	expect_eq!(
		report
			.pointer("/certified")
			.unwrap_or(&serde_json::Value::Null)
			.as_bool(),
		Some(false)
	);
	expect_true!(
		report
			.pointer("/timings_seconds/total")
			.unwrap_or(&serde_json::Value::Null)
			.as_f64()
			.is_some()
	);
	let frozen = quest_qsvt_io::read_qsp_json(
		&std::fs::read_to_string(output)?,
		quest_qsvt_io::IoPolicy::default(),
	)?;
	let quest_qsvt_io::QspInput::Symmetric(phases) = frozen else {
		return Err(std::io::Error::other("wrong export convention").into());
	};
	expect_eq!(phases.degree(), 1);
	Ok(())
}

#[gtest]
fn generalized_synthesis_preserves_matrix_export_and_rejects_phase_input() -> googletest::Result<()>
{
	let dir = tempfile::tempdir()?;
	let input = dir.path().join("polynomial.json");
	let output = dir.path().join("controls.json");
	std::fs::write(
		&input,
		r#"{"basis":"Laurent","coefficients":[[0.1,0.2],[0.05,-0.1]]}"#,
	)?;
	let args = [
		"qsvt",
		"synthesize",
		"--export",
		"sequence",
		"--mode",
		"unit-circle-response",
		"--input",
		input
			.to_str()
			.ok_or_else(|| std::io::Error::other("path"))?,
		"--output",
		output
			.to_str()
			.ok_or_else(|| std::io::Error::other("path"))?,
	];
	let report = Cli::try_parse_from(args)?.run()?;
	expect_eq!(
		report
			.pointer("/degree")
			.unwrap_or(&serde_json::Value::Null)
			.as_u64(),
		Some(1)
	);
	let frozen = quest_qsvt_io::read_qsp_json(
		&std::fs::read_to_string(&output)?,
		quest_qsvt_io::IoPolicy::default(),
	)?;
	expect_true!(matches!(
		frozen,
		quest_qsvt_io::QspInput::GeneralizedMatrices(_)
	));
	std::fs::write(
		&input,
		r#"{"angles":[0.1],"convention":"pyqsp-wx-symmetric"}"#,
	)?;
	expect_true!(Cli::try_parse_from(args)?.run().is_err());
	Ok(())
}

#[cfg(feature = "certification")]
#[gtest]
fn certification_is_explicit_and_reports_every_attempt() -> googletest::Result<()> {
	let dir = tempfile::tempdir()?;
	let input = dir.path().join("polynomial.json");
	let output = dir.path().join("phases.json");
	let trace = dir.path().join("trace.json");
	std::fs::write(&input, r#"{"coefficients":[0.1]}"#)?;
	let report = Cli::try_parse_from([
		std::ffi::OsStr::new("qsvt"),
		std::ffi::OsStr::new("synthesize"),
		std::ffi::OsStr::new("--input"),
		input.as_os_str(),
		std::ffi::OsStr::new("--output"),
		output.as_os_str(),
		std::ffi::OsStr::new("--certify"),
		std::ffi::OsStr::new("--trace"),
		trace.as_os_str(),
	])?
	.run()?;
	expect_eq!(
		report
			.pointer("/certified")
			.unwrap_or(&serde_json::Value::Null)
			.as_bool(),
		Some(true)
	);
	expect_true!(
		report
			.pointer("/certificate/attempts")
			.unwrap_or(&serde_json::Value::Null)
			.as_array()
			.is_some_and(|a| !a.is_empty())
	);
	expect_true!(
		report
			.pointer("/timings_seconds/certification")
			.unwrap_or(&serde_json::Value::Null)
			.as_f64()
			.is_some_and(|t| t > 0.0)
	);
	let trace: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(trace)?)?;
	expect_true!(
		trace
			.pointer("/traceEvents")
			.unwrap_or(&serde_json::Value::Null)
			.as_array()
			.is_some_and(|events| {
				events.iter().any(|e| {
					e.pointer("/name").unwrap_or(&serde_json::Value::Null) == "certification"
				})
			})
	);
	Ok(())
}
#[cfg(feature = "offline-synthesis")]
#[gtest]
fn offline_command_is_explicit_and_retains_computation_and_certification_timings()
-> googletest::Result<()> {
	let dir = tempfile::tempdir()?;
	let input = dir.path().join("polynomial.json");
	let output = dir.path().join("phases.json");
	std::fs::write(&input, r#"{"coefficients":[0.1]}"#)?;
	let report = Cli::try_parse_from([
		std::ffi::OsStr::new("qsvt"),
		std::ffi::OsStr::new("offline-synthesize"),
		std::ffi::OsStr::new("--input"),
		input.as_os_str(),
		std::ffi::OsStr::new("--output"),
		output.as_os_str(),
	])?
	.run()?;
	expect_eq!(
		report
			.pointer("/construction")
			.unwrap_or(&serde_json::Value::Null)
			.as_str(),
		Some("offline-arbitrary-precision")
	);
	expect_eq!(
		report
			.pointer("/certified")
			.unwrap_or(&serde_json::Value::Null)
			.as_bool(),
		Some(true)
	);
	expect_true!(
		report
			.pointer("/offline_attempts")
			.unwrap_or(&serde_json::Value::Null)
			.as_array()
			.is_some_and(|attempts| !attempts.is_empty()
				&& attempts.iter().all(|a| a
					.pointer("/certification_seconds")
					.unwrap_or(&serde_json::Value::Null)
					.as_f64()
					.is_some()
					&& a.pointer("/computation_seconds")
						.unwrap_or(&serde_json::Value::Null)
						.as_f64()
						.is_some()))
	);
	Ok(())
}

#[gtest]
fn failed_input_admission_still_writes_a_failed_trace_without_output() -> googletest::Result<()> {
	let dir = tempfile::tempdir()?;
	let input = dir.path().join("invalid.json");
	let output = dir.path().join("output.json");
	let trace = dir.path().join("trace.json");
	std::fs::write(&input, "{invalid")?;
	let result = Cli::try_parse_from([
		std::ffi::OsStr::new("qsvt"),
		std::ffi::OsStr::new("synthesize"),
		std::ffi::OsStr::new("--export"),
		std::ffi::OsStr::new("sequence"),
		std::ffi::OsStr::new("--input"),
		input.as_os_str(),
		std::ffi::OsStr::new("--output"),
		output.as_os_str(),
		std::ffi::OsStr::new("--trace"),
		trace.as_os_str(),
	])?
	.run();
	expect_true!(result.is_err());
	expect_false!(output.exists());
	let trace: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(trace)?)?;
	expect_true!(
		trace
			.get("traceEvents")
			.and_then(serde_json::Value::as_array)
			.is_some_and(|events| events.iter().any(|event| event
				.pointer("/args/outcome")
				.and_then(serde_json::Value::as_str)
				== Some("failed")))
	);
	Ok(())
}

#[gtest]
fn catalog_check_constructs_the_selected_target_instead_of_trusting_its_label()
-> googletest::Result<()> {
	let report = Cli::try_parse_from([
		"qsvt",
		"catalog",
		"check",
		"--kappa",
		"5",
		"--epsilon",
		"0.1",
	])?
	.run()?;
	expect_eq!(
		report.get("failed").and_then(serde_json::Value::as_u64),
		Some(0)
	);
	expect_eq!(
		report
			.pointer("/families/0/synthesis/degree")
			.and_then(serde_json::Value::as_u64),
		Some(9)
	);
	expect_eq!(
		report
			.pointer("/families/0/label_is_certificate")
			.and_then(serde_json::Value::as_bool),
		Some(false)
	);
	Ok(())
}

#[cfg(feature = "rayon")]
#[gtest]
fn explicit_workers_use_a_caller_owned_catalogue_pool_and_report_scope() -> googletest::Result<()> {
	let report = Cli::try_parse_from([
		"qsvt",
		"--workers",
		"2",
		"catalog",
		"check",
		"--kappa",
		"5",
		"--epsilon",
		"0.1",
	])?
	.run()?;
	expect_eq!(
		report
			.pointer("/parallelism/workers")
			.and_then(serde_json::Value::as_u64),
		Some(2)
	);
	expect_eq!(
		report
			.pointer("/parallelism/scope")
			.and_then(serde_json::Value::as_str),
		Some("independent catalogue families")
	);
	expect_eq!(
		report
			.pointer("/families/0/synthesis/degree")
			.and_then(serde_json::Value::as_u64),
		Some(9)
	);
	Ok(())
}

#[cfg(feature = "rayon")]
#[gtest]
fn per_synthesis_pool_exports_identical_frozen_words() -> googletest::Result<()> {
	let dir = tempfile::tempdir()?;
	let input = dir.path().join("target.json");
	let first = dir.path().join("serial.json");
	let second = dir.path().join("parallel.json");
	std::fs::write(
		&input,
		r#"{"basis":"Laurent","coefficients":[[0.1,0.2],[0.05,-0.1],[0.02,0.01]]}"#,
	)?;
	for (workers, path) in [("1", &first), ("2", &second)] {
		let report = Cli::try_parse_from([
			std::ffi::OsStr::new("qsvt"),
			std::ffi::OsStr::new("--workers"),
			std::ffi::OsStr::new(workers),
			std::ffi::OsStr::new("synthesize"),
			std::ffi::OsStr::new("--export"),
			std::ffi::OsStr::new("sequence"),
			std::ffi::OsStr::new("--mode"),
			std::ffi::OsStr::new("unit-circle-response"),
			std::ffi::OsStr::new("--input"),
			input.as_os_str(),
			std::ffi::OsStr::new("--output"),
			path.as_os_str(),
		])?
		.run()?;
		expect_eq!(
			report
				.get("construction")
				.and_then(serde_json::Value::as_str),
			Some("binary64")
		);
	}
	expect_eq!(std::fs::read(first)?, std::fs::read(second)?);
	Ok(())
}

#[gtest]
fn solver_defaults_are_inverse_nlft_and_catalogue_algorithm_is_selectable() -> googletest::Result<()>
{
	let cli = Cli::try_parse_from([
		"qsvt",
		"synthesize",
		"--input",
		"input.json",
		"--output",
		"out.json",
	])?;
	let quest_qsvt_cli::Command::Synthesize(args) = cli.command else {
		return fail!("command");
	};
	expect_true!(matches!(
		args.algorithm,
		quest_qsvt_cli::Algorithm::InverseNlft
	));
	for algorithm in ["inverse-nlft", "rhw"] {
		let report = Cli::try_parse_from([
			"qsvt",
			"catalog",
			"check",
			"--kappa",
			"5",
			"--epsilon",
			"0.1",
			"--algorithm",
			algorithm,
		])?
		.run()?;
		expect_eq!(report["failed"], 0);
		expect_eq!(report["families"][0]["algorithm"], algorithm);
	}
	Ok(())
}

#[gtest]
fn catalogue_export_constructs_exact_family_with_selected_algorithm() -> googletest::Result<()> {
	let dir = tempfile::tempdir()?;
	let output = dir.path().join("payload.json");
	for algorithm in ["inverse-nlft", "rhw"] {
		let report = Cli::try_parse_from([
			std::ffi::OsStr::new("qsvt"),
			std::ffi::OsStr::new("catalog"),
			std::ffi::OsStr::new("synthesize"),
			std::ffi::OsStr::new("--kappa"),
			std::ffi::OsStr::new("5"),
			std::ffi::OsStr::new("--epsilon"),
			std::ffi::OsStr::new("0.1"),
			std::ffi::OsStr::new("--algorithm"),
			std::ffi::OsStr::new(algorithm),
			std::ffi::OsStr::new("--export"),
			std::ffi::OsStr::new("sequence"),
			std::ffi::OsStr::new("--output"),
			output.as_os_str(),
		])?
		.run()?;
		expect_eq!(report["algorithm"], algorithm);
		expect_eq!(report["degree"], 9);
		let qsp = quest_qsvt_io::read_qsp_json(
			&std::fs::read_to_string(&output)?,
			quest_qsvt_io::IoPolicy::default(),
		)?;
		expect_true!(matches!(qsp, quest_qsvt_io::QspInput::Symmetric(_)));
	}
	Ok(())
}

#[cfg(feature = "native")]
#[gtest]
fn new_workflows_keep_matrix_and_physical_state_contracts_at_parse_time() {
	let common = [
		"qsvt",
		"embedded",
		"--matrix",
		"matrix.h5",
		"--alpha",
		"1",
		"--qsp",
		"qsp.json",
		"--input-state",
		"in.h5",
		"--physical-output-state",
		"out.h5",
	];
	expect_true!(Cli::try_parse_from(common).is_ok());
	let mut invalid = common.to_vec();
	invalid.extend(["--encoding", "block.h5"]);
	expect_true!(Cli::try_parse_from(invalid).is_err());
	let invalid = [
		"qsvt",
		"embedded",
		"--matrix",
		"matrix.h5",
		"--qsp",
		"qsp.json",
		"--input-state",
		"in.h5",
		"--output-state",
		"out.h5",
	];
	expect_true!(Cli::try_parse_from(invalid).is_err());
	let mut invalid = common.to_vec();
	invalid.push("--distributed");
	expect_true!(Cli::try_parse_from(invalid).is_err());
}
