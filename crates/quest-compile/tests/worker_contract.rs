#![cfg(all(feature = "workers", target_os = "linux"))]
use googletest::{Result, prelude::*};
use quest_compile::optimizer::{Client, WorkerLimits};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{Gate, QuantumRegionBuilder};
use quest_math::Limits;

#[cfg(all(target_os = "linux", feature = "macros"))]
#[gtest]
fn numerical_oracle_calls_keep_local_certificates_without_a_global_bound() -> Result<()> {
	use quest_compile::{MatrixPolicy, NumericalOperator, OracleFragment, circuit};
	use std::os::unix::fs::PermissionsExt;

	// A worker may propose identity: the parent must independently certify it.
	struct Remove(std::path::PathBuf);
	impl Drop for Remove {
		fn drop(&mut self) {
			let _ = std::fs::remove_file(&self.0);
		}
	}
	let path = std::env::temp_dir().join(format!("quest-identity-worker-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[]},\"engine\":\"identity-fixture\",\"precision_bits\":256}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let matrix = faer::Mat::from_fn(2, 2, |row, col| {
		num_complex::Complex64::new(if row == col { 2.0 } else { 0.0 }, 0.0)
	});
	let mut body = QuantumRegionBuilder::new(1, 0)?;
	body.numerical(
		NumericalOperator::from_view(&matrix, MatrixPolicy::default())?,
		&[body.qubit(0)?],
		&[],
	)?;
	let fragment = OracleFragment::builder(body.finish()?.bind(&[])?)
		.matrix_tolerance(10.0)?
		.build()?;
	let direct = circuit! { oracle amplified[1] = ${fragment.clone()}; qubit[2] q; rz(0.2) q[0]; amplified q[0]; }?;
	let nested = circuit! { oracle amplified[1] = ${fragment.clone()}; gate outer a { amplified a; } qubit[2] q; rz(0.2) q[0]; outer q[0]; }?;
	let controlled = circuit! { oracle amplified[1] = ${fragment.clone()}; qubit[2] q; rz(0.2) q[0]; negctrl @ amplified q[1], q[0]; }?;
	let adjoint = circuit! { oracle amplified[1] = ${fragment.clone()}; qubit[2] q; rz(0.2) q[0]; adjoint @ amplified q[0]; }?;
	for program in [direct, nested, controlled, adjoint] {
		let program = program.verify()?;
		let input_snapshot = program.ssa().snapshot();
		let (output, report) = program.synthesize_rotations(&client, 0.15, 0, Limits::default())?;
		expect_eq!(report.input_snapshot, input_snapshot);
		expect_eq!(report.output_snapshot, output.ssa().snapshot());
		expect_ne!(report.input_snapshot, report.output_snapshot);
		expect_eq!(report.rotations.len(), 1);
		expect_true!(
			report.rotations[0]
				.certificate
				.sequence()
				.operations
				.is_empty()
		);
		expect_true!(report.operator_error_bound.is_none());
	}
	let unitary = circuit! { qubit q; rz(0.2) q; }?.verify()?;
	let (_, report) = unitary.synthesize_rotations(&client, 0.15, 0, Limits::default())?;
	expect_true!(report.operator_error_bound.is_some());
	// The admitted local error is within epsilon, but the numerical oracle amplifies it.
	let operator_error = 2.0 * (0.2_f64 / 4.0).sin();
	expect_lt!(2.0_f64.sqrt() * operator_error, 0.15);
	expect_gt!(2.0 * operator_error, 0.15);
	Ok(())
}
#[gtest]
fn explicit_synthesis_validates_epsilon_even_without_any_rotation() -> Result<()> {
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
	let program = builder.finish()?;
	let client = Client::new("/nonexistent-worker", WorkerLimits::default())?;
	for epsilon in [0.0, -1.0, 1.0, 2.0, f64::NAN, f64::INFINITY] {
		expect_true!(
			program
				.clone()
				.synthesize_rotations(&client, epsilon, 0, Limits::default())
				.is_err()
		);
	}
	let (unchanged, report) = program.synthesize_rotations(&client, 1e-12, 0, Limits::default())?;
	expect_eq!(unchanged.schedule().len(), 1);
	expect_true!(report.rotations.is_empty());
	Ok(())
}

#[gtest]
fn unchanged_worker_output_obeys_aggregate_provenance_budget() -> Result<()> {
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
	let program = builder.finish()?;
	let client = Client::new("/nonexistent-worker", WorkerLimits::default())?;
	let limits = Limits {
		bytes: 0,
		..Limits::default()
	};
	expect_true!(program.optimize_zx(&client, 0, limits).is_err());
	Ok(())
}

#[gtest]
fn zx_candidate_propagates_worker_error_after_an_effect_fence() -> Result<()> {
	let mut builder = QuantumRegionBuilder::new(1, 1)?;
	let q = builder.qubit(0)?;
	let bit = builder.bit(0)?;
	builder.measure(q, bit)?;
	builder.gate(Gate::H, &[q], &[])?;
	let program = builder.finish()?;
	let client = Client::new("/nonexistent-worker", WorkerLimits::default())?;
	expect_true!(
		program
			.clone()
			.zx_candidate(&client, 0, Limits::default(), 0, 2)
			.is_err()
	);
	expect_true!(
		program
			.zx_candidate(&client, 0, Limits::default(), 1, 2)
			.is_err()
	);
	Ok(())
}

#[cfg(target_os = "linux")]
#[gtest]
fn zx_candidate_accepts_a_longer_certified_region_with_fresh_identities() -> Result<()> {
	use std::os::unix::fs::PermissionsExt;
	struct Remove(std::path::PathBuf);
	impl Drop for Remove {
		fn drop(&mut self) {
			let _ = std::fs::remove_file(&self.0);
		}
	}
	let path =
		std::env::temp_dir().join(format!("quest-zx-candidate-worker-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"longer-fixture\",\"precision_bits\":256}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 1)?;
	let q = builder.qubit(0)?;
	let bit = builder.bit(0)?;
	builder.gate(Gate::H, &[q], &[])?;
	builder.measure(q, bit)?;
	builder.gate(Gate::H, &[q], &[])?;
	let original = builder.finish()?;
	let retained = original.schedule()[0..2].to_vec();
	expect_true!(
		original
			.clone()
			.zx_candidate_from(2, &client, 0, Limits::default(), 1, 4)
			.is_err()
	);
	let (candidate, report) = original.zx_candidate_from(2, &client, 0, Limits::default(), 1, 5)?;
	expect_eq!(report.accepted.len(), 1);
	expect_eq!(report.candidate_window, Some((2, 3)));
	expect_eq!(candidate.schedule().len(), 5);
	expect_eq!(&candidate.schedule()[0..2], retained.as_slice());
	candidate.bind(&[])?.plan()?;
	Ok(())
}

#[cfg(target_os = "linux")]
#[gtest]
fn zx_expanded_candidate_uses_its_distinct_request_and_keeps_growth_for_scoring() -> Result<()> {
	use std::os::unix::fs::PermissionsExt;
	struct Remove(std::path::PathBuf);
	impl Drop for Remove {
		fn drop(&mut self) {
			let _ = std::fs::remove_file(&self.0);
		}
	}
	let path =
		std::env::temp_dir().join(format!("quest-zx-expanded-worker-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ninput=$(cat); case \"$input\" in *ZxExpanded*) printf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"expanded-fixture\",\"precision_bits\":0}}}' ;; *) exit 7 ;; esac\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::H, &[q], &[])?;
	let original = builder.finish()?;
	let (candidate, report) =
		original.zx_expanded_candidate_from(0, &client, 0, Limits::default(), 1, 3)?;
	expect_eq!(candidate.schedule().len(), 3);
	expect_eq!(report.candidate_window, Some((0, 1)));
	candidate.bind(&[])?.plan()?;
	Ok(())
}
