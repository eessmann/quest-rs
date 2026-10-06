#![cfg(all(feature = "mpi", quest_native_mpi))]
use googletest::prelude::*;
use num_complex::Complex64 as C;
use quest_qsvt_io::{
	IoPolicy,
	hdf5::{StoredBlockEncoding, write_block_encoding, write_state_vector},
};
use std::path::{Path, PathBuf};

static FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
	_directory: tempfile::TempDir,
	encoding: PathBuf,
	qsp: PathBuf,
	input: PathBuf,
	reference: PathBuf,
}
fn fixture() -> googletest::Result<Fixture> {
	let directory = tempfile::tempdir()?;
	let encoding = directory.path().join("encoding.h5");
	let qsp = directory.path().join("input.json");
	let input = directory.path().join("input.h5");
	let reference = directory.path().join("reference.h5");
	let z = C::new(0.3, 0.4);
	let u = faer::Mat::from_fn(2, 2, |r, c| {
		if r == c {
			if r == 0 { z } else { C::new(-0.3, 0.4) }
		} else {
			C::new(0.75_f64.sqrt(), 0.0)
		}
	});
	let basis = faer::Mat::from_fn(2, 1, |r, _| C::new(if r == 0 { 1.0 } else { 0.0 }, 0.0));
	let block = StoredBlockEncoding::builder(u, basis.clone(), basis)
		.metadata(1.0, [1, 1], [1, 1])
		.build(IoPolicy::default())?;
	write_block_encoding(&encoding, &block, IoPolicy::default())?;
	write_state_vector(&input, &[C::new(1.0, 0.0)], IoPolicy::default())?;
	write_state_vector(&reference, &[C::new(0.0, 1.0)], IoPolicy::default())?;
	std::fs::write(&qsp, r#"{"basis":"Chebyshev","coefficients":[0.0,0.25]}"#)?;
	Ok(Fixture {
		_directory: directory,
		encoding,
		qsp,
		input,
		reference,
	})
}
fn path(value: &Path) -> googletest::Result<&str> {
	Ok(value
		.to_str()
		.ok_or_else(|| std::io::Error::other("path"))?)
}
fn run(count: &str, args: &[&str]) -> googletest::Result<quest_test_support::mpi::RunOutput> {
	Ok(
		quest_test_support::mpi::MpiTest::new(count.parse()?, std::time::Duration::from_secs(60))?
			.executable(env!("CARGO_BIN_EXE_quest-qsvt-cli"))
			.args(args)
			.output()?,
	)
}
fn report(
	output: &quest_test_support::mpi::RunOutput,
	ranks: u64,
) -> googletest::Result<serde_json::Value> {
	expect_true!(
		output.status.success(),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let text = String::from_utf8_lossy(&output.stdout);
	let start = text
		.find('{')
		.ok_or_else(|| std::io::Error::other("JSON report"))?;
	let result: serde_json::Value = serde_json::from_str(
		text.get(start..)
			.ok_or_else(|| std::io::Error::other("JSON report"))?,
	)?;
	expect_eq!(
		result
			.pointer("/distributed/ranks")
			.and_then(serde_json::Value::as_u64),
		Some(ranks)
	);
	Ok(result)
}
#[gtest]
fn mpi_two_and_four_rank_root_synthesis_preserve_embedded_and_overlap_mass()
-> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let fixture = fixture()?;
	for count in ["2", "4"] {
		let workers = if cfg!(feature = "rayon") {
			if count == "2" { "auto" } else { "2" }
		} else {
			"1"
		};
		let output = run(
			count,
			&[
				"embedded",
				"--distributed",
				"--workers",
				workers,
				"--encoding",
				path(&fixture.encoding)?,
				"--qsp",
				path(&fixture.qsp)?,
				"--route",
				"standard",
				"--synthesize-input",
				"--input-state",
				path(&fixture.input)?,
			],
		)?;
		let result = report(&output, count.parse()?)?;
		let timing = |name| {
			result
				.pointer(name)
				.and_then(serde_json::Value::as_f64)
				.ok_or_else(|| std::io::Error::other("complete MPI lifecycle timing"))
		};
		let accounted = std::ops::Add::add(
			std::ops::Add::add(
				timing("/timings_seconds/mpi_environment")?,
				timing("/timings_seconds/worker_pool")?,
			),
			timing("/timings_seconds/mpi_and_worker_teardown")?,
		);
		expect_that!(timing("/timings_seconds/total")?, ge(accounted));

		expect_that!(
			result
				.pointer("/mass/native_dispatches/total")
				.and_then(serde_json::Value::as_u64),
			some(eq(16))
		);
		expect_that!(
			result
				.pointer("/mass/native_dispatches/scope")
				.and_then(serde_json::Value::as_str),
			some(eq("successful_run_per_rank"))
		);
		expect_that!(
			result
				.pointer("/mass/retained")
				.and_then(serde_json::Value::as_f64)
				.unwrap_or(-1.0),
			near(0.015_625, 2e-12)
		);
		expect_eq!(
			result
				.pointer("/input_synthesis/construction")
				.and_then(serde_json::Value::as_str),
			Some("binary64")
		);
		let output = run(
			count,
			&[
				"overlap",
				"--distributed",
				"--workers",
				workers,
				"--encoding",
				path(&fixture.encoding)?,
				"--qsp",
				path(&fixture.qsp)?,
				"--route",
				"standard",
				"--synthesize-input",
				"--input-state",
				path(&fixture.input)?,
				"--reference-state",
				path(&fixture.reference)?,
			],
		)?;
		let result = report(&output, count.parse()?)?;
		expect_that!(
			result
				.pointer("/mass/native_dispatches/total")
				.and_then(serde_json::Value::as_u64),
			some(eq(30))
		);
		expect_that!(
			result
				.pointer("/overlap/0")
				.and_then(serde_json::Value::as_f64)
				.unwrap_or(-1.0),
			near(0.1, 2e-12)
		);
		expect_that!(
			result
				.pointer("/overlap/1")
				.and_then(serde_json::Value::as_f64)
				.unwrap_or(-1.0),
			near(-0.075, 2e-12)
		);
	}
	Ok(())
}
#[gtest]
fn root_input_failure_is_collective_and_never_enters_native_execution() -> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let output = run(
		"2",
		&[
			"embedded",
			"--distributed",
			"--encoding",
			"/nonexistent/qsvt-encoding.h5",
			"--qsp",
			"/nonexistent/qsvt-input.json",
			"--route",
			"standard",
			"--input-state",
			"/nonexistent/qsvt-state.h5",
		],
	)?;
	expect_false!(output.status.success());
	expect_false!(output.status.timed_out);
	expect_true!(String::from_utf8_lossy(&output.stderr).contains("another rank rejected"));
	Ok(())
}

#[cfg(feature = "certification")]
#[gtest]
fn nonroot_missing_files_are_not_read_or_synthesized() -> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let fixture = fixture()?;
	// Rank-dependent argv through one executable works with both mpiexec and srun.
	// All paths are positional arguments; none are interpolated into shell text.
	let script = r#"rank=${SLURM_PROCID:-${OMPI_COMM_WORLD_RANK:-${PMI_RANK:-${PMIX_RANK:-}}}}
case "$rank" in
0) exec "$1" --workers "$2" embedded --distributed --encoding "$3" --qsp "$4" --route standard --synthesize-input --certify-input --input-state "$5";;
1) exec "$1" embedded --distributed --encoding /missing/nonroot-block.h5 --qsp /missing/nonroot-qsp.json --route standard --input-state /missing/nonroot-state.h5;;
*) exit 71;;
esac"#;
	let output = quest_test_support::mpi::MpiTest::new(2, std::time::Duration::from_secs(60))?
		.executable("/bin/sh")
		.args([
			"-c",
			script,
			"quest-cli-rank-arguments",
			env!("CARGO_BIN_EXE_quest-qsvt-cli"),
			if cfg!(feature = "rayon") { "2" } else { "1" },
			path(&fixture.encoding)?,
			path(&fixture.qsp)?,
			path(&fixture.input)?,
		])
		.output()?;
	let result = report(&output, 2)?;
	expect_eq!(
		result
			.pointer("/input_synthesis/certified")
			.and_then(serde_json::Value::as_bool),
		Some(true)
	);
	expect_that!(
		result
			.pointer("/mass/retained")
			.and_then(serde_json::Value::as_f64)
			.unwrap_or(-1.0),
		near(0.015_625, 2e-12)
	);
	Ok(())
}
