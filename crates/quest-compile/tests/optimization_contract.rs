use googletest::Result;
use googletest::prelude::*;
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::*;

#[gtest]
fn exact_merge_reports_retain_only_immediate_rewrite_inputs() -> Result<()> {
	let mut builder = QuantumRegionBuilder::new(1, 0)?;
	let q = builder.qubit(0)?;
	for _ in 0..128 {
		builder.gate(Gate::Rz(Angle::pi(1, 7)?), &[q], &[])?;
	}
	let (program, report) = builder.finish()?.optimize_exact()?;
	expect_eq!(program.schedule().len(), 1);
	expect_eq!(report.rewrites.len(), 127);
	expect_true!(
		report
			.rewrites
			.iter()
			.all(|rewrite| rewrite.inputs.len() <= 2)
	);
	Ok(())
}

#[gtest]
fn dependency_cancellation_crosses_proven_commuting_gates_only() -> Result<()> {
	let mut b = QuantumRegionBuilder::new(3, 1)?;
	let q = b.qubit(0)?;
	let r = b.qubit(1)?;
	let c = b.qubit(2)?;
	b.gate(Gate::T, &[q], &[])?;
	b.gate(
		Gate::Rz(Angle::pi(1, 7)?),
		&[q],
		&[Control::new(c, ControlState::Zero)],
	)?;
	b.gate(Gate::H, &[r], &[])?;
	b.gate(Gate::Tdg, &[q], &[])?;
	let (optimized, report) = b.finish()?.optimize_exact()?;
	expect_eq!(optimized.schedule().len(), 2);
	expect_eq!(report.removed.len(), 2);
	for effect in [false, true] {
		let mut b = QuantumRegionBuilder::new(2, 1)?;
		let q = b.qubit(0)?;
		let r = b.qubit(1)?;
		b.gate(Gate::H, &[q], &[])?;
		if effect {
			b.measure(r, b.bit(0)?)?;
		} else {
			b.gate(Gate::X, &[q], &[])?;
		}
		b.gate(Gate::H, &[q], &[])?;
		expect_eq!(b.finish()?.optimize_exact()?.0.schedule().len(), 3);
	}
	Ok(())
}

#[gtest]
fn exact_rotation_merging_is_phase_correct_and_idempotent() -> Result<()> {
	let mut b = QuantumRegionBuilder::new(1, 0)?;
	let q = b.qubit(0)?;
	b.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
	b.gate(Gate::Rz(Angle::pi(1, 1)?), &[q], &[])?;
	let (p, report) = b.finish()?.optimize_exact()?;
	expect_eq!(report.after_operations, 1);
	let (again, _) = p.clone().optimize_exact()?;
	expect_eq!(again.schedule().len(), 1);
	let plan = p.bind(&[])?.plan()?;
	if let Operation::Gate { gate, .. } = plan.instructions()[0].operation() {
		let matrix = gate.matrix(MatrixPolicy::default())?;
		expect_lt!((matrix.view()[(0, 0)].re + 1.0).abs(), 1e-12);
		expect_lt!((matrix.view()[(1, 1)].re + 1.0).abs(), 1e-12);
	} else {
		fail!("rotation must remain symbolic")?;
	}
	Ok(())
}

#[gtest]
fn opaque_float_rotations_are_not_merged() -> Result<()> {
	let mut b = QuantumRegionBuilder::new(1, 0)?;
	let q = b.qubit(0)?;
	b.gate(Gate::Rx(Angle::radians(0.1)?), &[q], &[])?;
	b.gate(Gate::Rx(Angle::radians(-0.1)?), &[q], &[])?;
	expect_eq!(b.finish()?.optimize_exact()?.0.schedule().len(), 2);
	Ok(())
}

#[gtest]
fn fusion_preserves_multiplication_order_and_provenance() -> Result<()> {
	let mut b = QuantumRegionBuilder::new(1, 0)?;
	let q = b.qubit(0)?;
	let a = b.gate(Gate::X, &[q], &[])?;
	let z = b.gate(Gate::Z, &[q], &[])?;
	let (bound, report) = b.finish()?.bind(&[])?.fuse(FusionOptions::default())?;
	expect_eq!(report.after_operations, 1);
	expect_eq!(
		bound.provenance().source_leaves(
			bound.instructions()[0].provenance(),
			ExpansionLimits::default()
		)?,
		vec![a, z]
	);
	if let Operation::Numerical { matrix, .. } = bound.instructions()[0].operation() {
		expect_eq!(matrix.view()[(0, 1)].re, 1.0);
		expect_eq!(matrix.view()[(1, 0)].re, -1.0);
	} else {
		fail!("expected a fused numerical payload")?;
	}
	Ok(())
}

#[gtest]
fn cycles_and_classical_read_write_hazards_are_checked() -> Result<()> {
	let mut b = QuantumRegionBuilder::new(2, 1)?;
	let q = b.qubit(0)?;
	let other = b.qubit(1)?;
	let c = b.bit(0)?;
	let write = b.measure(q, c)?;
	let read = b.gate_if(c, true, Gate::X, &[other], &[])?;
	let overwrite = b.measure(q, c)?;
	let p = b.finish()?;
	expect_true!(p.has_dependency(write, read)?);
	expect_true!(p.has_dependency(read, overwrite)?);
	let mut b = QuantumRegionBuilder::new(1, 0)?;
	let q = b.qubit(0)?;
	let a = b.gate(Gate::X, &[q], &[])?;
	let z = b.gate(Gate::Z, &[q], &[])?;
	b.depend(z, a)?;
	expect_true!(matches!(b.finish(), Err(Error::Cycle)));
	Ok(())
}

#[gtest]
fn symbolic_merging_preserves_all_finite_bindings() -> Result<()> {
	let mut b = QuantumRegionBuilder::new(1, 0)?;
	let q = b.qubit(0)?;
	let p = b.parameter("large")?;
	b.gate(Gate::Rx(Angle::parameter(p)?), &[q], &[])?;
	b.gate(Gate::Rx(Angle::parameter(p)?), &[q], &[])?;
	let original = b.finish()?;
	expect_true!(original.clone().bind(&[(p, 1e308)]).is_ok());
	let (optimized, _) = original.optimize_exact()?;
	expect_true!(optimized.bind(&[(p, 1e308)]).is_ok());
	Ok(())
}

#[gtest]
fn fusion_skips_blocks_when_retained_matrices_exhaust_program_budget() -> Result<()> {
	let limits = ProgramLimits {
		max_matrix_bytes: 384,
		..ProgramLimits::default()
	};
	let mut b = QuantumRegionBuilder::with_limits(4, 0, limits)?;
	for index in 0..4 {
		let q = b.qubit(index)?;
		b.gate(Gate::H, &[q], &[])?;
		b.gate(Gate::X, &[q], &[])?;
	}
	let (fused, report) = b.finish()?.bind(&[])?.fuse(FusionOptions::default())?;
	// One 128-byte result leaves insufficient space for another three-buffer
	// product, even though the second result alone would fit the final budget.
	expect_eq!(report.matrix_bytes, 128);
	expect_eq!(fused.instructions().len(), 7);
	Ok(())
}

#[gtest]
fn rational_merging_preserves_finite_lowering() -> Result<()> {
	let angle = Angle::rational_pi(quest_compile::RBig::from(
		dashu_int::IBig::from(10).pow(307),
	))?;
	let mut b = QuantumRegionBuilder::new(1, 0)?;
	let q = b.qubit(0)?;
	for _ in 0..10 {
		b.gate(Gate::Rz(angle.clone()), &[q], &[])?;
	}
	let original = b.finish()?;
	expect_true!(original.clone().bind(&[]).is_ok());
	expect_true!(original.optimize_exact()?.0.bind(&[]).is_ok());
	Ok(())
}

#[gtest]
fn reports_dependency_depth_before_and_after_exact_rewrites_and_fusion() -> Result<()> {
	let mut b = QuantumRegionBuilder::new(2, 0)?;
	let q = b.qubit(0)?;
	let r = b.qubit(1)?;
	b.gate(Gate::H, &[q], &[])?;
	b.gate(Gate::H, &[q], &[])?;
	b.gate(Gate::X, &[r], &[])?;
	let p = b.finish()?;
	expect_eq!(p.dependency_depth(), 2);
	let (_, report) = p.optimize_exact()?;
	expect_eq!(report.before_depth, 2);
	expect_eq!(report.after_depth, 1);

	let mut b = QuantumRegionBuilder::new(2, 0)?;
	let first = b.gate(Gate::H, &[b.qubit(0)?], &[])?;
	b.gate(Gate::X, &[b.qubit(0)?], &[])?;
	let last = b.gate(Gate::Z, &[b.qubit(1)?], &[])?;
	b.depend(first, last)?;
	let (p, report) = b.finish()?.bind(&[])?.fuse(FusionOptions::default())?;
	expect_eq!(report.before_depth, 2);
	expect_eq!(report.after_depth, 1);
	expect_eq!(p.dependency_depth(), 1);
	expect_eq!(report.peak_matrix_bytes, 1152);
	expect_eq!(report.matrix_bytes, 256);
	Ok(())
}
