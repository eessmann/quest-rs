#![cfg(feature = "native")]
use googletest::prelude::*;
use num_complex::Complex64 as C;
use quest_qsvt_io::{
	IoPolicy,
	hdf5::{StoredBlockEncoding, read_state_vector, write_block_encoding, write_state_vector},
};
use std::{
	ops::{Div, Neg, Sub},
	path::Path,
};
static FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn command(args: &[&str]) -> googletest::Result<serde_json::Value> {
	let output = std::process::Command::new(env!("CARGO_BIN_EXE_quest-qsvt-cli"))
		.args(args)
		.output()?;
	expect_true!(
		output.status.success(),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let text = String::from_utf8(output.stdout)?;
	// Native backends may print an initialization banner before the report.
	let start = text
		.find('{')
		.ok_or_else(|| std::io::Error::other("missing JSON report"))?;
	Ok(serde_json::from_str(
		text.get(start..)
			.ok_or_else(|| std::io::Error::other("report"))?,
	)?)
}
fn path(value: &Path) -> googletest::Result<&str> {
	Ok(value
		.to_str()
		.ok_or_else(|| std::io::Error::other("path"))?)
}
fn phases(path: &Path) -> googletest::Result<()> {
	std::fs::write(
		path,
		format!(
			r#"{{"angles":[{0},{0}],"convention":"pyqsp-wx-symmetric"}}"#,
			std::f64::consts::FRAC_PI_4
		),
	)?;
	Ok(())
}
#[gtest]
fn native_embedded_and_overlap_preserve_complex_phase_and_mass() -> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let encoding_path = dir.path().join("encoding.h5");
	let qsp = dir.path().join("phases.json");
	let input = dir.path().join("input.h5");
	let reference = dir.path().join("reference.h5");
	let output = dir.path().join("output.h5");
	let trace = dir.path().join("trace.json");
	let z = C::new(0.3, 0.4);
	let u = faer::Mat::from_fn(2, 2, |r, c| {
		if r == c {
			if r == 0 { z } else { z.conj().neg() }
		} else {
			C::new(0.75_f64.sqrt(), 0.0)
		}
	});
	let basis = faer::Mat::from_fn(2, 1, |r, _| C::new(if r == 0 { 1.0 } else { 0.0 }, 0.0));
	let block = StoredBlockEncoding::builder(u, basis.clone(), basis)
		.metadata(1.0, [1, 1], [1, 1])
		.build(IoPolicy::default())?;
	write_block_encoding(&encoding_path, &block, IoPolicy::default())?;
	write_state_vector(&input, &[C::new(1.0, 0.0)], IoPolicy::default())?;
	write_state_vector(&reference, &[C::new(0.0, 1.0)], IoPolicy::default())?;
	phases(&qsp)?;
	let report = command(&[
		"embedded",
		"--encoding",
		path(&encoding_path)?,
		"--qsp",
		path(&qsp)?,
		"--route",
		"standard",
		"--input-state",
		path(&input)?,
		"--output-state",
		path(&output)?,
		"--trace",
		path(&trace)?,
	])?;
	// Dense imported projectors: 9 circuit +4 projection +3 probability calls.
	expect_that!(
		report
			.pointer("/mass/native_dispatches/total")
			.and_then(serde_json::Value::as_u64),
		some(eq(16))
	);
	expect_that!(
		report
			.pointer("/mass/native_dispatches/scope")
			.and_then(serde_json::Value::as_str),
		some(eq("successful_run_per_rank"))
	);
	let amplitudes = read_state_vector(&output, IoPolicy::default())?;
	expect_that!(
		amplitudes
			.first()
			.ok_or_else(|| std::io::Error::other("amplitudes"))?
			.re,
		near(z.re, 1e-12)
	);
	expect_that!(
		amplitudes
			.first()
			.ok_or_else(|| std::io::Error::other("amplitudes"))?
			.im,
		near(z.im, 1e-12)
	);
	expect_that!(
		report
			.pointer("/mass/retained")
			.unwrap_or(&serde_json::Value::Null)
			.as_f64()
			.unwrap_or(-1.0),
		near(0.25, 1e-12)
	);
	expect_true!(trace.exists());
	let report = command(&[
		"overlap",
		"--encoding",
		path(&encoding_path)?,
		"--qsp",
		path(&qsp)?,
		"--route",
		"standard",
		"--input-state",
		path(&input)?,
		"--reference-state",
		path(&reference)?,
	])?;
	// Controlled projections add4 projectors +6 clone/add calls; readout7.
	expect_that!(
		report
			.pointer("/mass/native_dispatches/total")
			.and_then(serde_json::Value::as_u64),
		some(eq(30))
	);
	expect_that!(
		report
			.pointer("/overlap/0")
			.unwrap_or(&serde_json::Value::Null)
			.as_f64()
			.unwrap_or(-1.0),
		near(0.4, 1e-12)
	);
	expect_that!(
		report
			.pointer("/overlap/1")
			.unwrap_or(&serde_json::Value::Null)
			.as_f64()
			.unwrap_or(-1.0),
		near(-0.3, 1e-12)
	);
	Ok(())
}
#[gtest]
fn complex_solve_recovers_physical_norm_and_separately_checks_residual() -> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let matrix = dir.path().join("matrix.h5");
	let rhs = dir.path().join("rhs.h5");
	let qsp = dir.path().join("phases.json");
	let output = dir.path().join("physical.h5");
	let normalized = dir.path().join("normalized.h5");
	let diagonal = [C::new(3.0, 4.0), C::new(-4.0, 3.0)];
	let b = [C::new(1.0, 2.0), C::new(3.0, -1.0)];
	{
		let file = hdf5_metno::File::create(&matrix)?;
		let group = file.create_group("matrix")?;
		group
			.new_attr::<u64>()
			.shape(2)
			.create("shape")?
			.write_raw(&[2, 2])?;
		group
			.new_dataset::<C>()
			.shape([2, 2])
			.create("dense")?
			.write_raw(&[diagonal[0], C::new(0.0, 0.0), C::new(0.0, 0.0), diagonal[1]])?;
	}
	write_state_vector(&rhs, &b, IoPolicy::default())?;
	phases(&qsp)?;
	let report = command(&[
		"solve",
		"--matrix",
		path(&matrix)?,
		"--rhs",
		path(&rhs)?,
		"--qsp",
		path(&qsp)?,
		"--route",
		"standard",
		"--reciprocal-scale",
		"1.0",
		"--output-state",
		path(&output)?,
		"--normalized-output-state",
		path(&normalized)?,
		"--residual-tolerance",
		"1e-10",
	])?;
	let physical = read_state_vector(output, IoPolicy::default())?;
	for ((actual, b), diagonal) in physical.iter().zip(b).zip(diagonal) {
		expect_that!(actual.sub(b.div(diagonal)).norm(), near(0.0, 1e-12));
	}
	expect_that!(
		report
			.pointer("/relative_physical_residual")
			.unwrap_or(&serde_json::Value::Null)
			.as_f64()
			.unwrap_or(-1.0),
		near(0.0, 1e-12)
	);
	let normalized = read_state_vector(normalized, IoPolicy::default())?;
	expect_that!(
		normalized.iter().map(C::norm_sqr).sum::<f64>(),
		near(1.0, 1e-12)
	);
	expect_that!(
		physical.iter().map(C::norm_sqr).sum::<f64>(),
		near(0.6, 1e-12)
	);
	Ok(())
}

#[gtest]
fn solve_rejects_rank_deficiency_before_publishing_output() -> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let matrix = dir.path().join("singular.h5");
	let rhs = dir.path().join("rhs.h5");
	let qsp = dir.path().join("phases.json");
	let output = dir.path().join("physical.h5");
	{
		let file = hdf5_metno::File::create(&matrix)?;
		let group = file.create_group("matrix")?;
		group
			.new_attr::<u64>()
			.shape(2)
			.create("shape")?
			.write_raw(&[2, 2])?;
		group
			.new_dataset::<f64>()
			.shape([2, 2])
			.create("dense")?
			.write_raw(&[1.0, 0.0, 0.0, 0.0])?;
	}
	write_state_vector(
		&rhs,
		&[C::new(1.0, 0.0), C::new(1.0, 0.0)],
		IoPolicy::default(),
	)?;
	phases(&qsp)?;
	let result = std::process::Command::new(env!("CARGO_BIN_EXE_quest-qsvt-cli"))
		.args([
			"solve",
			"--matrix",
			path(&matrix)?,
			"--rhs",
			path(&rhs)?,
			"--qsp",
			path(&qsp)?,
			"--route",
			"standard",
			"--reciprocal-scale",
			"1.0",
			"--output-state",
			path(&output)?,
		])
		.output()?;
	expect_false!(result.status.success());
	expect_true!(String::from_utf8_lossy(&result.stderr).contains("numerical full rank"));
	expect_false!(output.exists());
	Ok(())
}

#[cfg(feature = "certification")]
#[gtest]
fn compiled_qsp_load_recertifies_and_retains_native_response_evidence() -> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let encoding = dir.path().join("encoding.h5");
	let input = dir.path().join("input.h5");
	let output = dir.path().join("output.h5");
	let source = dir.path().join("source.json");
	let compiled = dir.path().join("compiled.json");
	let z = C::new(0.3, 0.4);
	let u = faer::Mat::from_fn(2, 2, |r, c| {
		if r == c {
			if r == 0 { z } else { z.conj().neg() }
		} else {
			C::new(0.75f64.sqrt(), 0.0)
		}
	});
	let basis = faer::Mat::from_fn(2, 1, |r, _| C::new(if r == 0 { 1.0 } else { 0.0 }, 0.0));
	let block = StoredBlockEncoding::builder(u, basis.clone(), basis)
		.metadata(1.0, [1, 1], [1, 1])
		.build(IoPolicy::default())?;
	write_block_encoding(&encoding, &block, IoPolicy::default())?;
	write_state_vector(&input, &[C::new(1.0, 0.0)], IoPolicy::default())?;
	for (mode, route, basis, offset, coefficients) in [
		("real-parity-wx", "standard", "Chebyshev", 0, "[0.0,0.3]"),
		(
			"unit-circle-response",
			"hermitianized-odd",
			"Laurent",
			0,
			"[0.0,0.3]",
		),
		(
			"unit-circle-response",
			"hermitianized-odd",
			"Laurent",
			1,
			"[0.3]",
		),
	] {
		for algorithm in ["rhw", "inverse-nlft"] {
			std::fs::write(
				&source,
				format!(
					r#"{{"basis":"{basis}","minimum_order":{offset},"coefficients":{coefficients}}}"#
				),
			)?;
			command(&[
				"synthesize",
				"--input",
				path(&source)?,
				"--output",
				path(&compiled)?,
				"--mode",
				mode,
				"--algorithm",
				algorithm,
				"--certify",
			])?;
			let report = command(&[
				"embedded",
				"--encoding",
				path(&encoding)?,
				"--qsp",
				path(&compiled)?,
				"--route",
				route,
				"--input-state",
				path(&input)?,
				"--output-state",
				path(&output)?,
			])?;
			expect_eq!(report["input_synthesis"]["algorithm"], algorithm);
			if route == "standard" {
				expect_eq!(report["certified_projector_payload"], true);
			} else {
				expect_eq!(report["response_evidence"], "certified");
			}
			let values = read_state_vector(&output, IoPolicy::default())?;
			expect_that!(values[0].re, near(0.09, 1e-11));
			expect_that!(values[0].im, near(0.12, 1e-11));
		}
	}
	Ok(())
}

#[gtest]
fn matrix_embedding_auto_routes_and_physical_register_io_preserve_postselected_state()
-> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let matrix = dir.path().join("matrix.h5");
	let qsp = dir.path().join("phases.json");
	let logical = dir.path().join("logical.h5");
	let physical = dir.path().join("physical.h5");
	let input = dir.path().join("input.h5");
	let physical_input = dir.path().join("physical-input.h5");
	let a = faer::Mat::from_fn(1, 1, |_, _| C::new(0.3, 0.4));
	quest_qsvt_io::hdf5::write_matrix(&matrix, a.as_ref(), IoPolicy::default())?;
	write_state_vector(&input, &[C::new(1.0, 0.0)], IoPolicy::default())?;
	// canonical source=bit0; response=bit1, response initialized zero.
	write_state_vector(
		&physical_input,
		&[
			C::new(1.0, 0.0),
			C::new(0.0, 0.0),
			C::new(1.0, 0.0),
			C::new(0.0, 0.0),
		],
		IoPolicy::default(),
	)?;
	phases(&qsp)?;
	for physical_mode in [false, true] {
		let mut args = vec![
			"embedded",
			"--matrix",
			path(&matrix)?,
			"--alpha",
			"1",
			"--qsp",
			path(&qsp)?,
			"--input-state",
			path(if physical_mode {
				&physical_input
			} else {
				&input
			})?,
			"--output-state",
			path(&logical)?,
			"--physical-output-state",
			path(&physical)?,
		];
		if physical_mode {
			args.push("--physical-input");
		}
		let report = command(&args)?;
		expect_eq!(report["route"], "Standard");
		if physical_mode {
			expect_that!(
				report["mass"]["initial"].as_f64().unwrap_or(-1.0),
				near(2.0, 1e-12)
			);
			expect_that!(
				report["mass"]["input"].as_f64().unwrap_or(-1.0),
				near(1.0, 1e-12)
			);
		}
		let logical = read_state_vector(&logical, IoPolicy::default())?;
		expect_that!(logical[0].re, near(0.3, 1e-12));
		expect_that!(logical[0].im, near(0.4, 1e-12));
		let physical = read_state_vector(&physical, IoPolicy::default())?;
		expect_eq!(physical.len(), 4);
		expect_that!(physical[0].re, near(logical[0].re, 1e-12));
		expect_that!(physical[0].im, near(logical[0].im, 1e-12));
		for z in &physical[1..] {
			expect_that!(z.norm(), near(0.0, 1e-12));
		}
	}
	Ok(())
}

#[gtest]
fn seeded_presets_are_reproducible_and_catalogue_solve_checks_domain_and_residual()
-> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let first = dir.path().join("first.h5");
	let second = dir.path().join("second.h5");
	for (preset, rows, cols) in [
		("diagonal", "2", "2"),
		("hermitian", "3", "3"),
		("general", "2", "3"),
	] {
		for output in [&first, &second] {
			command(&[
				"matrix-preset",
				"--preset",
				preset,
				"--rows",
				rows,
				"--cols",
				cols,
				"--seed",
				"123",
				"--output",
				path(output)?,
			])?;
		}
		let a = quest_qsvt_io::hdf5::read_matrix(&first, IoPolicy::default())?
			.into_dense(IoPolicy::default())?;
		let b = quest_qsvt_io::hdf5::read_matrix(&second, IoPolicy::default())?
			.into_dense(IoPolicy::default())?;
		for r in 0..a.nrows() {
			for c in 0..a.ncols() {
				expect_eq!(a[(r, c)], b[(r, c)]);
			}
		}
	}
	let rhs = dir.path().join("rhs.h5");
	let solution = dir.path().join("solution.h5");
	let a = faer::Mat::from_fn(2, 2, |r, c| C::new(if r == c { 1.0 } else { 0.0 }, 0.0));
	quest_qsvt_io::hdf5::write_matrix(&first, a.as_ref(), IoPolicy::default())?;
	write_state_vector(
		&rhs,
		&[C::new(1.0, 0.0), C::new(0.0, 0.0)],
		IoPolicy::default(),
	)?;
	let report = command(&[
		"catalog",
		"solve",
		"--matrix",
		path(&first)?,
		"--rhs",
		path(&rhs)?,
		"--kappa",
		"5",
		"--epsilon",
		"0.001",
		"--output-state",
		path(&solution)?,
		"--residual-tolerance",
		"0.1",
	])?;
	expect_eq!(report["input_synthesis"]["algorithm"], "inverse-nlft");
	expect_eq!(report["failed"], 0);
	expect_that!(
		report["relative_physical_residual"].as_f64().unwrap_or(2.0),
		lt(0.1)
	);
	// Domain admission happens before synthesis/output for kappa(A)>kappa(family).
	let a = faer::Mat::from_fn(2, 2, |r, c| {
		C::new(
			if r == c {
				if r == 0 { 1.0 } else { 0.1 }
			} else {
				0.0
			},
			0.0,
		)
	});
	quest_qsvt_io::hdf5::write_matrix(&first, a.as_ref(), IoPolicy::default())?;
	std::fs::remove_file(&solution)?;
	let failure = std::process::Command::new(env!("CARGO_BIN_EXE_quest-qsvt-cli"))
		.args([
			"catalog",
			"solve",
			"--matrix",
			path(&first)?,
			"--rhs",
			path(&rhs)?,
			"--kappa",
			"5",
			"--epsilon",
			"0.001",
			"--output-state",
			path(&solution)?,
		])
		.output()?;
	expect_false!(failure.status.success());
	expect_false!(solution.exists());
	expect_true!(String::from_utf8_lossy(&failure.stderr).contains("catalogue domain"));
	Ok(())
}

#[gtest]
fn rectangular_and_extreme_scale_solves_keep_physical_dimensions_and_finite_residuals()
-> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let matrix = dir.path().join("matrix.h5");
	let rhs = dir.path().join("rhs.h5");
	let qsp = dir.path().join("phases.json");
	let output = dir.path().join("out.h5");
	phases(&qsp)?;
	for (rows, cols, scale) in [(3, 2, 1.0), (2, 3, 1.0), (2, 2, 1e308), (2, 2, 1e-308)] {
		let a = faer::Mat::from_fn(rows, cols, |r, c| {
			C::new(if r == c { scale } else { 0.0 }, 0.0)
		});
		quest_qsvt_io::hdf5::write_matrix(&matrix, a.as_ref(), IoPolicy::default())?;
		let b: Vec<_> = (0..rows)
			.map(|r| C::new(if r == 0 { scale } else { 0.0 }, 0.0))
			.collect();
		write_state_vector(&rhs, &b, IoPolicy::default())?;
		let report = command(&[
			"solve",
			"--matrix",
			path(&matrix)?,
			"--rhs",
			path(&rhs)?,
			"--qsp",
			path(&qsp)?,
			"--reciprocal-scale",
			"1",
			"--output-state",
			path(&output)?,
			"--residual-tolerance",
			"1e-10",
		])?;
		expect_eq!(report["failed"], 0);
		expect_true!(
			report["relative_physical_residual"]
				.as_f64()
				.is_some_and(|x| x.is_finite() && x < 1e-10)
		);
		let x = read_state_vector(&output, IoPolicy::default())?;
		expect_eq!(x.len(), cols);
		expect_that!(x[0].re, near(1.0, 1e-12));
		for z in &x[1..] {
			expect_that!(z.norm(), near(0.0, 1e-12));
		}
	}
	Ok(())
}

#[gtest]
fn generalized_auto_uses_full_hermitianization_and_retains_both_logical_sides()
-> googletest::Result<()> {
	let _guard = FIXTURES
		.lock()
		.map_err(|_| std::io::Error::other("fixture lock"))?;
	let dir = tempfile::tempdir()?;
	let matrix = dir.path().join("matrix.h5");
	let input = dir.path().join("input.h5");
	let source = dir.path().join("source.json");
	let out = dir.path().join("out.h5");
	let a = faer::Mat::from_fn(1, 1, |_, _| C::new(0.3, 0.4));
	quest_qsvt_io::hdf5::write_matrix(&matrix, a.as_ref(), IoPolicy::default())?;
	std::fs::write(&source, r#"{"basis":"Laurent","coefficients":[0.0,0.3]}"#)?;
	// Full Hermitianization input/output order is [left,right].
	write_state_vector(
		&input,
		&[C::new(0.0, 0.0), C::new(1.0, 0.0)],
		IoPolicy::default(),
	)?;
	let report = command(&[
		"embedded",
		"--matrix",
		path(&matrix)?,
		"--alpha",
		"1",
		"--qsp",
		path(&source)?,
		"--synthesize-input",
		"--input-state",
		path(&input)?,
		"--output-state",
		path(&out)?,
	])?;
	expect_eq!(report["route"], "HermitianizedFull");
	let out = read_state_vector(&out, IoPolicy::default())?;
	expect_eq!(out.len(), 2);
	expect_that!(out[0].re, near(0.09, 1e-11));
	expect_that!(out[0].im, near(0.12, 1e-11));
	expect_that!(out[1].norm(), near(0.0, 1e-11));
	Ok(())
}
