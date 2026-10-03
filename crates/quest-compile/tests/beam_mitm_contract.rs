#![cfg(all(feature = "workers", target_os = "linux"))]
use googletest::{Result, prelude::*};
use quest_compile::optimizer::{Client, MitmResult, WorkerLimits};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{Angle, Gate, Operation, QuantumRegionBuilder};
use quest_compile::{Control, ControlState, WorkerError};
use quest_math::Limits;
use quest_optimizer_protocol::MitmLimits;
use std::os::unix::fs::PermissionsExt;

struct Remove(std::path::PathBuf);
impl Drop for Remove {
	fn drop(&mut self) {
		let _ = std::fs::remove_file(&self.0);
	}
}

#[gtest]
fn exact_mitm_candidate_keeps_effect_fence_and_certified_longer_region() -> Result<()> {
	let path =
		std::env::temp_dir().join(format!("quest-exact-mitm-candidate-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ninput=$(cat); case \"$input\" in *ExactMitm*) printf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"longer-fixture\",\"precision_bits\":0}}}' ;; *) exit 7 ;; esac\n",
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
	let replaced_id = *original
		.schedule()
		.get(2)
		.ok_or_else(|| std::io::Error::other("source region"))?;
	let limits = MitmLimits::for_qubits(1)?;
	let (candidate, report) =
		original.exact_mitm_candidate_from(2, &client, 0, limits, Limits::default(), 5)?;
	expect_eq!(report.candidate_window, Some((2, 3)));
	expect_true!(report.local_work > 0);
	expect_eq!(candidate.schedule().len(), 5);
	expect_eq!(&candidate.schedule()[0..2], retained.as_slice());
	expect_ne!(candidate.schedule().get(2), Some(&replaced_id));
	expect_true!(matches!(report.outcome, Some(MitmResult::Candidate(_))));
	candidate.bind(&[])?.plan()?;
	Ok(())
}

#[gtest]
fn exact_mitm_preserves_typed_terminal_status_and_original_program() -> Result<()> {
	let outcomes = [
		("NoCandidate", r#"{"NoCandidate":{"explored":17}}"#),
		(
			"Incomplete",
			r#"{"Incomplete":{"reason":"states","explored":18}}"#,
		),
		("Exhausted", r#"{"Exhausted":{"explored":19}}"#),
		(
			"Unresolved",
			r#"{"Unresolved":{"precision_bits":256,"explored":20}}"#,
		),
	];
	for (name, outcome) in outcomes {
		let path =
			std::env::temp_dir().join(format!("quest-exact-mitm-{name}-{}", std::process::id()));
		let _remove = Remove(path.clone());
		std::fs::write(
			&path,
			format!(
				"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{{\"version\":3,\"seed\":0,\"outcome\":{outcome}}}'\n"
			),
		)?;
		std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
		let client = Client::new(path, WorkerLimits::default())?;
		let mut builder = QuantumRegionBuilder::new(1, 0)?;
		let q = builder.qubit(0)?;
		builder.gate(Gate::H, &[q], &[])?;
		let original = builder.finish()?;
		let original_ids = original.schedule().to_vec();
		let (unchanged, report) = original.exact_mitm_candidate_from(
			0,
			&client,
			0,
			MitmLimits::for_qubits(1)?,
			Limits::default(),
			1,
		)?;
		expect_eq!(unchanged.schedule(), original_ids.as_slice());
		expect_eq!(report.candidate_window, Some((0, 1)));
		expect_true!(matches!(
			(name, report.outcome),
			(
				"NoCandidate",
				Some(MitmResult::NoCandidate { explored: 17 })
			) | (
				"Incomplete",
				Some(MitmResult::Incomplete { explored: 18, .. })
			) | ("Exhausted", Some(MitmResult::Exhausted { explored: 19 }))
				| (
					"Unresolved",
					Some(MitmResult::Unresolved {
						precision_bits: 256,
						explored: 20,
					}),
				)
		));
	}
	Ok(())
}

#[gtest]
fn exact_mitm_preserves_signed_controls_and_preflights_work() -> Result<()> {
	let path = std::env::temp_dir().join(format!("quest-exact-mitm-signed-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":2,\"operations\":[{\"gate\":\"X\",\"targets\":[0],\"controls\":[{\"qubit\":1,\"positive\":false}]}]},\"engine\":\"signed-fixture\",\"precision_bits\":0}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(2, 0)?;
	let target = builder.qubit(1)?;
	let control = builder.qubit(0)?;
	builder.gate(
		Gate::X,
		&[target],
		&[Control::new(control, ControlState::Zero)],
	)?;
	let original = builder.finish()?;
	let mut limits = MitmLimits::for_qubits(2)?;
	limits.max_work = 1;
	expect_true!(matches!(
		original
			.clone()
			.exact_mitm_candidate_from(0, &client, 0, limits, Limits::default(), 1),
		Err(WorkerError::Budget(
			"exact MITM output work" | "exact MITM scan work"
		))
	));
	let (candidate, report) = original.exact_mitm_candidate_from(
		0,
		&client,
		0,
		MitmLimits::for_qubits(2)?,
		Limits::default(),
		1,
	)?;
	expect_true!(matches!(report.outcome, Some(MitmResult::Candidate(_))));
	let bound = candidate.bind(&[])?;
	expect_true!(matches!(bound.instructions(), [instruction]
        if matches!(instruction.operation(), Operation::Gate { controls, .. }
            if controls.len() == 1 && controls.first().is_some_and(|control| control.state() == ControlState::Zero))));
	bound.plan()?;
	Ok(())
}

#[gtest]
fn exact_mitm_retains_symbolic_binding_obligations_across_a_fenced_region() -> Result<()> {
	let path =
		std::env::temp_dir().join(format!("quest-exact-mitm-symbolic-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"symbolic-fixture\",\"precision_bits\":0}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let p = builder.parameter("p")?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::Phase(Angle::parameter(p)?), &[q], &[])?;
	builder.gate(Gate::H, &[q], &[])?;
	let original = builder.finish()?;
	let (candidate, report) = original.exact_mitm_candidate_from(
		1,
		&client,
		0,
		MitmLimits::for_qubits(1)?,
		Limits::default(),
		4,
	)?;
	expect_true!(matches!(report.outcome, Some(MitmResult::Candidate(_))));
	expect_true!(candidate.clone().bind(&[]).is_err());
	candidate.bind(&[(p, 0.25)])?.plan()?;
	Ok(())
}

#[gtest]
fn exact_mitm_rejects_a_worker_candidate_without_parent_certificate() -> Result<()> {
	let path = std::env::temp_dir().join(format!("quest-exact-mitm-forged-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[]},\"engine\":\"forged-fixture\",\"precision_bits\":0}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::H, &[q], &[])?;
	let original = builder.finish()?;
	expect_true!(matches!(
		original.exact_mitm_candidate_from(
			0,
			&client,
			0,
			MitmLimits::for_qubits(1)?,
			Limits::default(),
			1,
		),
		Err(WorkerError::Worker(
			quest_compile::optimizer::Error::Verification(_)
		))
	));
	Ok(())
}

#[gtest]
fn exact_mitm_large_output_ceiling_reserves_only_reachable_depth() -> Result<()> {
	let path = std::env::temp_dir().join(format!("quest-exact-mitm-cap-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"large-cap-fixture\",\"precision_bits\":0}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::H, &[q], &[])?;
	let original = builder.finish()?;
	let (candidate, report) = original.exact_mitm_candidate_from(
		0,
		&client,
		0,
		MitmLimits::for_qubits(1)?,
		Limits::default(),
		16_384,
	)?;
	expect_eq!(candidate.schedule().len(), 1);
	expect_true!(matches!(report.outcome, Some(MitmResult::Candidate(_))));
	Ok(())
}

#[gtest]
fn exact_mitm_rejects_a_certified_worker_result_beyond_requested_depth() -> Result<()> {
	let path = std::env::temp_dir().join(format!("quest-exact-mitm-depth-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"overdepth-fixture\",\"precision_bits\":0}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::H, &[q], &[])?;
	let original = builder.finish()?;
	let mut limits = MitmLimits::for_qubits(1)?;
	limits.max_depth = 1;
	expect_true!(matches!(
		original.exact_mitm_candidate_from(0, &client, 0, limits, Limits::default(), 16_384),
		Err(WorkerError::RejectedOutput("exact MITM candidate depth"))
	));
	Ok(())
}

#[gtest]
fn exact_mitm_reports_post_request_output_cap_separately_from_preflight() -> Result<()> {
	let path = std::env::temp_dir().join(format!("quest-exact-mitm-output-{}", std::process::id()));
	let _remove = Remove(path.clone());
	std::fs::write(
		&path,
		"#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"version\":3,\"seed\":0,\"outcome\":{\"Candidate\":{\"sequence\":{\"qubits\":1,\"operations\":[{\"gate\":\"H\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]},{\"gate\":\"X\",\"targets\":[0],\"controls\":[]}]},\"engine\":\"output-fixture\",\"precision_bits\":0}}}'\n",
	)?;
	std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
	let client = Client::new(path, WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::H, &[q], &[])?;
	expect_true!(matches!(
		builder.finish()?.exact_mitm_candidate_from(
			0,
			&client,
			0,
			MitmLimits::for_qubits(1)?,
			Limits::default(),
			1,
		),
		Err(WorkerError::RejectedOutput("exact MITM output operations"))
	));
	Ok(())
}

#[gtest]
fn exact_mitm_charges_wide_operand_scan_before_visiting_window() -> Result<()> {
	let client = Client::new("/bin/false", WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1024, 0)?;
	let target = builder.qubit(0)?;
	let controls = (1..1024)
		.map(|index| Ok(Control::new(builder.qubit(index)?, ControlState::One)))
		.collect::<quest_compile::Result<Vec<_>>>()?;
	builder.gate(Gate::X, &[target], &controls)?;
	let mut limits = MitmLimits::for_qubits(2)?;
	limits.max_work = 100_000;
	expect_true!(matches!(
		builder.finish()?.exact_mitm_candidate_from(
			0,
			&client,
			0,
			limits,
			Limits::default(),
			16_384,
		),
		Err(WorkerError::Budget("exact MITM scan work"))
	));
	Ok(())
}

#[gtest]
fn exact_mitm_charges_source_phase_replay_before_visiting_window() -> Result<()> {
	let client = Client::new("/bin/false", WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::Phase(Angle::pi(1, 4)?), &[q], &[])?;
	let mut limits = MitmLimits::for_qubits(1)?;
	limits.max_work = 1_000;
	expect_true!(matches!(
		builder.finish()?.exact_mitm_candidate_from(
			0,
			&client,
			0,
			limits,
			Limits::default(),
			16_384,
		),
		Err(WorkerError::Budget("exact MITM scan work"))
	));
	Ok(())
}

#[gtest]
fn exact_mitm_rejects_malformed_limits_even_without_an_eligible_window() -> Result<()> {
	let client = Client::new("/bin/false", WorkerLimits::default())?;
	let mut builder = QuantumRegionBuilder::new(1, 1)?;
	let q = builder.qubit(0)?;
	let bit = builder.bit(0)?;
	builder.measure(q, bit)?;
	let mut limits = MitmLimits::for_qubits(1)?;
	limits.max_states = 0;
	expect_true!(matches!(
		builder
			.finish()?
			.exact_mitm_candidate_from(0, &client, 0, limits, Limits::default(), 1,),
		Err(WorkerError::Worker(quest_compile::optimizer::Error::Limits))
	));
	Ok(())
}
