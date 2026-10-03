#![cfg(all(target_os = "linux", any(feature = "synthesis", feature = "zx")))]
use googletest::prelude::*;
#[cfg(feature = "zx")]
use quest_compile::ZxPasses;
#[cfg(feature = "synthesis")]
use quest_compile::{Angle, Control, ControlState, ExactPasses, RotationSynthesisPasses};
use quest_compile::{Gate, QuantumRegionBuilder};
use quest_math::Limits;
use quest_optimizer_client::{Client, WorkerLimits};
fn worker() -> Result<Client> {
	Ok(Client::new(
		env!("CARGO_BIN_EXE_quest-optimizer-worker"),
		WorkerLimits::default(),
	)?)
}

#[cfg(feature = "synthesis")]
#[gtest]
fn explicit_synthesis_retains_target_identity_control_phase_and_local_certificates() -> Result<()> {
	let mut builder = QuantumRegionBuilder::new(3, 0)?;
	let q = builder.qubit(2)?;
	let c = builder.qubit(0)?;
	let source = builder.gate(
		Gate::Rz(Angle::pi(1, 7)?),
		&[q],
		&[Control::new(c, ControlState::Zero)],
	)?;
	let (candidate, report) =
		builder
			.finish()?
			.synthesize_rotations(&worker()?, 1e-12, 42, Limits::default())?;
	expect_eq!(report.rotations.len(), 1);
	expect_true!(report.operator_error_bound.is_some());
	expect_true!(candidate.schedule().len() > 1);
	let certificate_root = report.rotations[0].provenance;
	expect_ne!(certificate_root, report.rotations[0].input);
	expect_eq!(
		report
			.provenance
			.source_leaves(certificate_root, quest_compile::ExpansionLimits::default())?,
		vec![source]
	);
	let (candidate, _) = candidate.optimize_exact()?;
	let plan = candidate.bind(&[])?.plan()?;
	for instruction in plan.instructions() {
		expect_eq!(
			plan.provenance().source_leaves(
				instruction.provenance(),
				quest_compile::ExpansionLimits::default()
			)?,
			vec![source]
		);
		match instruction.operation() {
			quest_compile::Operation::Gate {
				targets, controls, ..
			} => {
				expect_eq!(targets.as_ref(), &[q]);
				expect_eq!(controls.as_ref(), &[Control::new(c, ControlState::Zero)]);
			}
			quest_compile::Operation::GlobalPhase { controls, .. } => {
				expect_eq!(controls.as_ref(), &[Control::new(c, ControlState::Zero)]);
			}
			_ => fail!("unexpected synthesized effect")?,
		}
	}
	let mut builder = QuantumRegionBuilder::new(1, 1)?;
	builder.gate(Gate::Rx(Angle::pi(1, 2)?), &[builder.qubit(0)?], &[])?;
	builder.measure(builder.qubit(0)?, builder.bit(0)?)?;
	let (_, report) =
		builder
			.finish()?
			.synthesize_rotations(&worker()?, 1e-12, 43, Limits::default())?;
	expect_true!(report.operator_error_bound.is_none());
	expect_eq!(report.rotations.len(), 1);
	Ok(())
}

#[cfg(feature = "zx")]
#[gtest]
fn zx_replacements_are_transactional_and_preserve_measurement_occurrences() -> Result<()> {
	let mut builder = QuantumRegionBuilder::new(1, 1)?;
	let q = builder.qubit(0)?;
	builder.gate(Gate::H, &[q], &[])?;
	builder.gate(Gate::H, &[q], &[])?;
	let measure = builder.measure(q, builder.bit(0)?)?;
	builder.gate(Gate::X, &[q], &[])?;
	builder.gate(Gate::X, &[q], &[])?;
	let original = builder.finish()?;
	let missing = Client::new("/quest-worker-does-not-exist", WorkerLimits::default())?;
	let (retained, skipped) = original
		.clone()
		.optimize_zx(&missing, 3, Limits::default())?;
	expect_eq!(retained.schedule(), original.schedule());
	expect_eq!(skipped.skipped.len(), 2);
	let (optimized, report) = original.optimize_zx(&worker()?, 3, Limits::default())?;
	expect_eq!(optimized.schedule(), &[measure]);
	expect_eq!(report.accepted.len(), 2);
	for accepted in report.accepted {
		expect_eq!(
			report
				.provenance
				.source_leaves(
					accepted.provenance,
					quest_compile::ExpansionLimits::default()
				)?
				.len(),
			2
		);
		expect_true!(matches!(
			report.provenance.node(accepted.provenance)?,
			quest_compile::ProvenanceNode::Rewrite(_)
		));
		quest_math::verify_exact(
			accepted.certificate.candidate(),
			accepted.certificate.target(),
			Limits::default(),
		)?;
	}
	Ok(())
}
