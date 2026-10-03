#![cfg(feature = "qsvt")]
use googletest::prelude::*;
use quest::{Complex64 as C, Environment, QubitCount};
use quest_compile::{
	Angle, Control, ControlState, Gate, NumericalOperator, OracleFragment, QuantumRegionBuilder,
};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{
	EncodingBuilder, Left, LogicalSpace, NumericalPolicy, OperandLayout, Right, TransformBuilder,
};

// One native test in this binary: QuEST initialization cannot be repeated.
#[gtest]
fn successful_run_counts_native_decompositions_and_hadamard_readout() -> Result<()> {
	let policy = NumericalPolicy::default();
	let mut body = QuantumRegionBuilder::new(2, 0)?;
	let a = body.qubit(0)?;
	let b = body.qubit(1)?;
	body.gate(
		Gate::U {
			theta: Angle::radians(0.2)?,
			phi: Angle::radians(0.3)?,
			lambda: Angle::radians(0.4)?,
		},
		&[b],
		&[Control::new(a, ControlState::Zero)],
	)?;
	body.global_phase(Angle::radians(0.1)?, &[Control::new(b, ControlState::Zero)])?;
	body.gate(Gate::Sx, &[a], &[])?;
	let diagonal = faer::Mat::from_fn(2, 2, |r, c| match (r, c) {
		(0, 0) => C::new(0.0, 1.0),
		(1, 1) => C::new(1.0, 0.0),
		_ => C::new(0.0, 0.0),
	});
	body.numerical(
		NumericalOperator::from_view(diagonal.as_ref(), policy.matrix_policy())?,
		&[a],
		&[],
	)?;
	let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
		.matrix_tolerance(1e-12)?
		.build()?;
	let encoding = EncodingBuilder::new()
		.oracle(oracle)
		.left(LogicalSpace::<Left>::coordinates(4, &[0], policy)?)
		.right(LogicalSpace::<Right>::coordinates(4, &[0], policy)?)
		.normalization(1.0)?
		.build()?;
	let transform = TransformBuilder::new()
		.encoding(encoding)
		.operands(OperandLayout::new(3, vec![2, 0], 1, None)?)
		.standard(PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?)
		.build()?;
	let expected = transform.materialize_block()?;
	// Each branch has two projector-phase pairs: negative response 2*(3+7),
	// positive response 2*(1+5). Source body costs 26 or16 (U, phase, Sx, matrix).
	// H/Rz/H add3 =>32+42+3=77 circuit calls. Two cube projectors +3 probabilities.
	let env = Environment::builder().build()?;
	let mut prepared = env.qsvt().transform(transform.clone()).admit()?.prepare()?;
	let mut register = env.state_vector(QubitCount::new(3)?)?;
	for _ in 0..2 {
		register.init_zero()?;
		let result = prepared.run(&mut register)?;
		let report = result.mass().native_dispatches();
		expect_that!(report.circuit(), eq(77));
		expect_that!(report.projection(), eq(2));
		expect_that!(report.readout(), eq(3));
		expect_that!(report.state_management(), eq(0));
		expect_that!(report.total(), eq(82));
		let actual = result.logical_snapshot()?;
		expect_that!(actual[(0, 0)].re, near(expected[(0, 0)].re, 2e-12));
		expect_that!(actual[(0, 0)].im, near(expected[(0, 0)].im, 2e-12));
		let _ = result.release();
	}
	let mut overlap = env
		.qsvt()
		.transform(transform)
		.overlap()
		.input(vec![C::new(1.0, 0.0)])
		.reference(vec![C::new(1.0, 0.0)])
		.admit()?
		.prepare()?;
	let observed = overlap.run()?;
	let report = observed.native_dispatches();
	// The added outer control is positive, so circuit toggles are unchanged.
	// Each conditional projection adds2 projectors and2 clone/add operations;
	// restoration/readout add2 clones; readout has4 probabilities and3 gates.
	expect_that!(report.circuit(), eq(77));
	expect_that!(report.projection(), eq(6));
	expect_that!(report.readout(), eq(7));
	expect_that!(report.state_management(), eq(6));
	expect_that!(report.total(), eq(96));
	expect_that!(observed.overlap().re, near(expected[(0, 0)].re, 2e-12));
	expect_that!(observed.overlap().im, near(expected[(0, 0)].im, 2e-12));
	Ok(())
}
