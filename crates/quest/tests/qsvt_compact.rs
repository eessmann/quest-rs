#![cfg(feature = "qsvt")]

use googletest::prelude::*;
use quest::{Complex64, Environment, MemoryBudget, QubitCount};
use quest_compile::{OracleFragment, QuantumRegionBuilder};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{
	EncodingBuilder, ExplicitUnitaryPremise, Left, LogicalSpace, NumericalPolicy, OperandLayout,
	Right, TransformBuilder,
};
use std::ops::{Mul, Sub};

fn isolated(name: &str, body: impl FnOnce() -> googletest::Result<()>) -> googletest::Result<()> {
	if std::env::var("QUEST_QSVT_COMPACT_TEST").as_deref() == Ok(name) {
		return body();
	}
	let status = std::process::Command::new(std::env::current_exe()?)
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_QSVT_COMPACT_TEST", name)
		.status()?;
	expect_true!(status.success());
	Ok(())
}

fn identity_encoding(
	width: usize,
	left: LogicalSpace<Left>,
	right: LogicalSpace<Right>,
) -> quest_qsvt::Result<quest_qsvt::ProjectedEncoding> {
	let oracle = OracleFragment::builder(QuantumRegionBuilder::new(width, 0)?.finish()?.bind(&[])?)
		.matrix_tolerance(1e-12)?
		.build()?;
	EncodingBuilder::new()
		.oracle(oracle)
		.left(left)
		.right(right)
		.normalization(1.0)?
		.unitarity_assumption(ExplicitUnitaryPremise::new("empty circuit is identity")?)
		.build()
}

#[gtest]
fn compact_native_ranges_project_decode_and_condition_ordered_rectangular_spaces()
-> googletest::Result<()> {
	isolated(
		"compact_native_ranges_project_decode_and_condition_ordered_rectangular_spaces",
		|| {
			let policy = NumericalPolicy::default();
			let encoding = identity_encoding(
				4,
				LogicalSpace::logical_range(16, 3..9, policy)?,
				LogicalSpace::logical_range(16, 2..7, policy)?,
			)?;
			let transform = TransformBuilder::new()
				.encoding(encoding)
				.operands(OperandLayout::new(5, vec![4, 1, 3, 0], 2, None)?)
				.standard(
					PhaseSequence::<WxSymmetric>::builder(vec![std::f64::consts::FRAC_PI_4; 2])
						.build()?,
				)
				.build()?;
			let reference = transform.materialize_block()?;
			let environment = Environment::builder().build()?;
			let mut prepared = environment.qsvt().transform(transform.clone()).prepare()?;
			let mut register = environment.state_vector(QubitCount::new(5)?)?;
			for condition in [false, true] {
				register.init_zero()?;
				for &target in transform.operands().source() {
					register.h(target)?;
				}
				let result = prepared.run(&mut register)?;
				expect_true!((result.mass().retained() - 0.25).abs() < 1e-12);
				let actual = if condition {
					result.condition()?.logical_snapshot()?
				} else {
					result.logical_snapshot()?
				};
				for row in 0..6 {
					let expected = (0..5)
						.map(|col| reference[(row, col)])
						.sum::<Complex64>()
						.mul(if condition { 0.5 } else { 0.25 });
					expect_true!(actual[(row, 0)].sub(expected).norm() < 1e-12);
				}
			}
			Ok(())
		},
	)
}

#[gtest]
fn compact_native_preparation_does_not_reserve_every_logical_coordinate() -> googletest::Result<()>
{
	isolated(
		"compact_native_preparation_does_not_reserve_every_logical_coordinate",
		|| {
			let policy = NumericalPolicy::default();
			let encoding = identity_encoding(
				18,
				LogicalSpace::bit_constraints(1 << 18, 7, 0, policy)?,
				LogicalSpace::bit_constraints(1 << 18, 7, 0, policy)?,
			)?;
			let transform = TransformBuilder::new()
				.encoding(encoding)
				.standard(PhaseSequence::<WxSymmetric>::builder(vec![0.21]).build()?)
				.build()?;
			let environment = Environment::builder()
				.memory_budget(MemoryBudget::new(128 * 1024))
				.build()?;
			let baseline = environment.allocated_bytes();
			let prepared = environment.qsvt().transform(transform).prepare()?;
			expect_true!(
				environment
					.allocated_bytes()
					.checked_sub(baseline)
					.ok_or(quest::Error::Overflow)?
					< 128 * 1024
			);
			drop(prepared);
			expect_eq!(environment.allocated_bytes(), baseline);
			Ok(())
		},
	)
}

#[gtest]
fn compact_overlap_prepares_by_scattering_without_dense_isometry() -> googletest::Result<()> {
	isolated(
		"compact_overlap_prepares_by_scattering_without_dense_isometry",
		|| {
			let policy = NumericalPolicy::default();
			let encoding = identity_encoding(
				6,
				LogicalSpace::bit_constraints(64, 32, 0, policy)?,
				LogicalSpace::bit_constraints(64, 32, 0, policy)?,
			)?;
			let transform = TransformBuilder::new()
				.encoding(encoding)
				.standard(
					PhaseSequence::<WxSymmetric>::builder(vec![std::f64::consts::FRAC_PI_4])
						.build()?,
				)
				.build()?;
			let environment = Environment::builder()
				.memory_budget(MemoryBudget::new(96 * 1024))
				.build()?;
			let values = vec![Complex64::new(1.0 / 32.0_f64.sqrt(), 0.0); 32];
			let mut prepared = environment
				.qsvt()
				.transform(transform)
				.overlap()
				.input(values.clone())
				.reference(values)
				.prepare()?;
			let result = prepared.run()?;
			expect_true!((result.overlap().re - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
			expect_true!(result.overlap().im.abs() < 1e-12);
			Ok(())
		},
	)
}
