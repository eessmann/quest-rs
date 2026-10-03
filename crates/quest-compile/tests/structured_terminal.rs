use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::*;

#[gtest]
fn structured_pipeline_runs_exact_cleanup_before_terminal_fusion() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; h q;", "pipeline.qasm")?.verify()?;
	let snapshot = program.ssa().snapshot();
	let outcome = program.optimize_structured(
		&options(1, ApproximationMode::Disabled)?,
		&BudgetLedger::new(OptimizationLimits::default()),
	)?;
	expect_eq!(outcome.report().input_snapshot(), snapshot);
	expect_true!(outcome.report().flow_usage().is_some());
	expect_true!(outcome.report().exact_report().is_some());
	expect_eq!(
		outcome
			.report()
			.terminal_report()
			.map(StructuredTerminalReport::fused_windows),
		Some(0)
	);
	expect_true!(outcome.program().ssa().blocks().iter().all(|block| {
		block.instructions.iter().all(|item| {
			!matches!(
				item.kind,
				language::ssa::InstructionKind::Gate { .. }
					| language::ssa::InstructionKind::Call { .. }
			)
		})
	}));
	Ok(())
}

#[gtest]
fn structured_pipeline_exhaustion_retains_original_publication() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; h q;", "pipeline.qasm")?.verify()?;
	let snapshot = program.ssa().snapshot();
	let outcome = program.optimize_structured(
		&options(1, ApproximationMode::Disabled)?,
		&BudgetLedger::new(OptimizationLimits::new(1, 256 * 1024 * 1024)?),
	)?;
	expect_eq!(outcome.report().status(), TerminalStatus::WorkLimit);
	expect_eq!(outcome.program().ssa().snapshot(), snapshot);
	Ok(())
}

#[gtest]
fn structured_pipeline_releases_scratch_before_terminal_comparison() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; h q;", "pipeline.qasm")?.verify()?;
	let ledger = BudgetLedger::new(OptimizationLimits::new(10_000_000, 1024 * 1024)?);
	let outcome =
		program.optimize_structured(&options(1, ApproximationMode::Disabled)?, &ledger)?;
	expect_eq!(outcome.report().status(), TerminalStatus::Complete);
	expect_true!(outcome.report().exact_report().is_some());
	expect_lt!(ledger.usage().retained_bytes, 1024 * 1024);
	Ok(())
}

#[gtest]
fn structured_pipeline_compares_branch_blocks_without_frequency_assumptions() -> Result<()> {
	let program = Program::<Constructed>::parse(
		"input bool flag; qubit q; if (flag) { h q; h q; } else { x q; }",
		"branches.qasm",
	)?
	.verify()?;
	let outcome = program.optimize_structured(
		&options(1, ApproximationMode::Disabled)?,
		&BudgetLedger::new(OptimizationLimits::default()),
	)?;
	expect_true!(outcome.report().exact_report().is_some());
	expect_true!(
		outcome
			.program()
			.ssa()
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.all(|item| !matches!(
				item.kind,
				language::ssa::InstructionKind::Gate {
					gate: language::GateKind::H,
					..
				}
			))
	);
	outcome.into_program().lower()?.plan()?;
	Ok(())
}

#[gtest]
fn structured_pipeline_declines_unknown_mpi_communication() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; h q;", "mpi.qasm")?.verify()?;
	let snapshot = program.ssa().snapshot();
	let options = OptimizationOptions::new(
		OptimizationTarget::once(
			DeploymentSnapshot::new(DeploymentKind::StateVector, 1, false, true, false, 0, 2, 1)?,
			CostProfile::NativeV1,
		)?,
		OptimizationLimits::default(),
		ApproximationMode::Disabled,
	)?;
	let outcome =
		program.optimize_structured(&options, &BudgetLedger::new(OptimizationLimits::default()))?;
	expect_true!(
		outcome
			.report()
			.skipped_stages()
			.contains(&StructuredSkippedStage::UnknownCommunication)
	);
	expect_eq!(outcome.program().ssa().snapshot(), snapshot);
	Ok(())
}

#[gtest]
fn structured_pipeline_rejects_wrong_deployment_before_resource_fallback() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; h q;", "width.qasm")?.verify()?;
	let result = program.optimize_structured(
		&options(2, ApproximationMode::Disabled)?,
		&BudgetLedger::new(OptimizationLimits::default()),
	);
	expect_true!(matches!(
		result,
		Err(StructuredTerminalError::Circuit(Error::Budget(
			"terminal target width"
		)))
	));
	Ok(())
}

fn options(
	width: usize,
	approximation: ApproximationMode,
) -> quest_compile::Result<OptimizationOptions> {
	OptimizationOptions::new(
		OptimizationTarget::once(
			DeploymentSnapshot::new(
				DeploymentKind::StateVector,
				width,
				false,
				false,
				false,
				0,
				1,
				1 << width,
			)?,
			CostProfile::NativeV1,
		)?,
		OptimizationLimits::default(),
		approximation,
	)
}

#[gtest]
fn structured_terminal_publishes_ssa_and_oracles_together() -> Result<()> {
	let source = "qubit q; h q; x q;";
	let program = Program::<Constructed>::parse(source, "terminal.qasm")?.verify()?;
	let snapshot = program.ssa().snapshot();
	let ledger = BudgetLedger::new(OptimizationLimits::default());
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&options(1, ApproximationMode::Disabled)?,
		&ledger,
	)?;
	expect_eq!(outcome.report().fused_windows(), 1);
	expect_ne!(outcome.program().ssa().snapshot(), snapshot);
	let plan = outcome.into_program().lower()?.plan()?;
	expect_eq!(plan.oracle_captures().len(), 1);
	expect_eq!(
		plan.syntax(),
		&Program::<Constructed>::parse(source, "terminal.qasm")?
			.verify()?
			.lower()?
			.plan()?
			.syntax()
			.clone()
	);
	Ok(())
}

#[gtest]
fn user_gate_bodies_stay_symbolic_for_inverse_and_negative_power() -> Result<()> {
	let program = Program::<Constructed>::parse(
		"gate pair q { h q; x q; } qubit q; inv @ pair q; pow(-2) @ pair q;",
		"gate.qasm",
	)?
	.verify()?;
	let ledger = BudgetLedger::new(OptimizationLimits::default());
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&options(1, ApproximationMode::Disabled)?,
		&ledger,
	)?;
	expect_eq!(outcome.report().fused_windows(), 0);
	expect_true!(
		outcome
			.into_program()
			.lower()?
			.plan()?
			.oracle_captures()
			.is_empty()
	);
	Ok(())
}

#[gtest]
fn global_approximation_skips_uncertified_fusion_and_zero_wire_windows() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; x q;", "global.qasm")?.verify()?;
	let snapshot = program.ssa().snapshot();
	let ledger = BudgetLedger::new(OptimizationLimits::default());
	let mode = ApproximationMode::global(RBig::from_parts_signed(1.into(), 100.into()))?;
	let outcome = program.fuse_terminal(TerminalOptions::default(), &options(1, mode)?, &ledger)?;
	expect_eq!(outcome.program().ssa().snapshot(), snapshot);
	expect_eq!(outcome.report().fused_windows(), 0);
	let program =
		Program::<Constructed>::parse("qubit q; gphase(0.25); gphase(0.5);", "scalar.qasm")?
			.verify()?;
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&options(1, ApproximationMode::Disabled)?,
		&ledger,
	)?;
	expect_eq!(outcome.report().fused_windows(), 0);
	Ok(())
}

#[gtest]
fn mixed_parameter_bodies_fuse_and_dynamic_or_trapping_windows_stay_separate() -> Result<()> {
	let program = Program::<Constructed>::parse(
		"def pair(qubit a) { h a; x a; } qubit q; pair(q);",
		"mixed.qasm",
	)?
	.verify()?;
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&options(1, ApproximationMode::Disabled)?,
		&BudgetLedger::new(OptimizationLimits::default()),
	)?;
	expect_eq!(outcome.report().fused_windows(), 1);
	expect_eq!(
		outcome
			.report()
			.rewrites()
			.first()
			.ok_or(Error::InvalidId)?
			.inputs
			.len(),
		2
	);
	outcome.into_program().lower()?.plan()?;
	let program =
		Program::<Constructed>::parse("qubit[2] q; input int i; h q[i]; x q[i];", "dynamic.qasm")?
			.verify()?;
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&options(2, ApproximationMode::Disabled)?,
		&BudgetLedger::new(OptimizationLimits::default()),
	)?;
	expect_eq!(outcome.report().fused_windows(), 0);
	Ok(())
}

#[gtest]
fn exhausted_structured_transaction_keeps_original_snapshot_and_bank() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; x q;", "budget.qasm")?.verify()?;
	let snapshot = program.ssa().snapshot();
	let ledger = BudgetLedger::new(OptimizationLimits::new(1, 256 * 1024 * 1024)?);
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&options(1, ApproximationMode::Disabled)?,
		&ledger,
	)?;
	expect_eq!(outcome.report().status(), TerminalStatus::WorkLimit);
	expect_eq!(outcome.program().ssa().snapshot(), snapshot);
	expect_eq!(outcome.report().fused_windows(), 0);
	expect_true!(
		outcome
			.into_program()
			.lower()?
			.plan()?
			.oracle_captures()
			.is_empty()
	);
	Ok(())
}

#[gtest]
fn synthetic_capture_is_fresh_beside_existing_oracle_and_scalar_captures() -> Result<()> {
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	builder.gate(Gate::H, &[builder.qubit(0)?], &[])?;
	let fragment =
		OracleFragment::from_program(builder.finish()?.bind(&[])?, 0.0, MatrixPolicy::default())?;
	let program = circuit! { oracle existing[1] = ${fragment}; qubit q; existing q; rx(${0.3_f64}) q; h q; x q; }?.verify()?;
	let old = program.clone().lower()?.plan()?;
	let old_capture = *old
		.oracle_captures()
		.keys()
		.next()
		.ok_or(Error::InvalidId)?;
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&options(1, ApproximationMode::Disabled)?,
		&BudgetLedger::new(OptimizationLimits::default()),
	)?;
	expect_eq!(outcome.report().fused_windows(), 1);
	let capture = outcome
		.report()
		.rewrites()
		.first()
		.ok_or(Error::InvalidId)?
		.capture;
	expect_ne!(capture, old_capture);
	expect_ge!(capture, old.captures().len());
	expect_eq!(
		outcome
			.into_program()
			.lower()?
			.plan()?
			.oracle_captures()
			.len(),
		2
	);
	Ok(())
}

#[gtest]
fn semantic_storage_exhaustion_keeps_the_original_publication() -> Result<()> {
	let program = Program::<Constructed>::parse("qubit q; h q; x q;", "storage.qasm")?.verify()?;
	let snapshot = program.ssa().snapshot();
	let default = options(1, ApproximationMode::Disabled)?;
	let small = OptimizationOptions::new(
		default.target().clone(),
		OptimizationLimits::new(10_000_000, 1)?,
		ApproximationMode::Disabled,
	)?;
	let outcome = program.fuse_terminal(
		TerminalOptions::default(),
		&small,
		&BudgetLedger::new(OptimizationLimits::default()),
	)?;
	expect_eq!(outcome.report().status(), TerminalStatus::StorageLimit);
	expect_eq!(outcome.program().ssa().snapshot(), snapshot);
	expect_eq!(outcome.report().fused_windows(), 0);
	Ok(())
}
