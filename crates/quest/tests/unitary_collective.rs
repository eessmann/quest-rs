#![cfg(all(feature = "qsvt", feature = "mpi", quest_native_mpi))]
use googletest::prelude::*;
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::{
	Complex64, Control, ControlState, MatrixPolicy, NumericalOperator, OracleFragment,
	QuantumRegionBuilder, QubitCount,
};
use std::ops::{Div, Mul, Sub};

fn matrix(admitted: bool, tolerance: f64) -> quest::Result<NumericalOperator> {
	let source = faer::Mat::from_fn(2, 2, |r, c| match (r, c) {
		(0, 1) => Complex64::new(0., 1.),
		(1, 0) => Complex64::new(1., 0.),
		_ => Complex64::new(0., 0.),
	});
	let matrix = NumericalOperator::from_view(&source, MatrixPolicy::default())?;
	Ok(if admitted {
		matrix.admit_unitary(tolerance, MatrixPolicy::default(), 64)?
	} else {
		matrix
	})
}
fn program(
	matrix: NumericalOperator,
	controls: bool,
	adjoint: bool,
) -> quest::Result<quest::Program<quest::Executable>> {
	let mut builder = QuantumRegionBuilder::new(4, 0)?;
	let controls = if controls {
		[
			(0, ControlState::Zero),
			(1, ControlState::One),
			(2, ControlState::One),
		]
		.into_iter()
		.map(|(q, s)| Ok(Control::new(builder.qubit(q)?, s)))
		.collect::<quest::Result<Vec<_>>>()?
	} else {
		Vec::new()
	};
	if adjoint {
		let mut b = QuantumRegionBuilder::new(1, 0)?;
		b.numerical(matrix, &[b.qubit(0)?], &[])?;
		let body = OracleFragment::builder(b.finish()?.bind(&[])?)
			.matrix_tolerance(1e-12)?
			.build()?;
		builder.oracle(&body.adjoint(), &[builder.qubit(3)?], &controls)?;
	} else {
		builder.numerical(matrix, &[builder.qubit(3)?], &controls)?;
	}
	Ok(quest::Program::from_region(builder.finish()?, &[])?
		.verify()?
		.lower()?
		.plan()?)
}
#[gtest]
fn native_unitary_controls_fit_mpi_target_bound_and_reject_evidence_disagreement()
-> googletest::Result<()> {
	if std::env::var("QUEST_UNITARY_RANKS").is_err() {
		for ranks in ["1", "2", "4"] {
			let status = quest_test_support::mpi::MpiTest::new(
				ranks.parse()?,
				std::time::Duration::from_secs(60),
			)?
			.args([
				"--exact",
				"native_unitary_controls_fit_mpi_target_bound_and_reject_evidence_disagreement",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_UNITARY_RANKS", ranks)
			.status()?;
			expect_true!(status.success());
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	{
		let env = CollectiveEnvironment::builder(&comm)?.build()?;
		let general = env.prepare(program(matrix(false, 1e-12)?, true, false)?);
		if comm.size()? > 1 {
			expect_true!(general.is_err());
		} else {
			expect_true!(general.is_ok());
		}
		drop(general);
		// Whole-fragment admission cannot grant individual matrix evidence.
		let general_oracle = env.prepare(program(matrix(false, 1e-12)?, true, true)?);
		if comm.size()? > 1 {
			expect_true!(general_oracle.is_err());
		} else {
			expect_true!(general_oracle.is_ok());
		}
		drop(general_oracle);
		let mut forward = env.prepare(program(matrix(true, 1e-12)?, true, false)?)?;
		let mut inverse = env.prepare(program(matrix(true, 1e-12)?, true, true)?)?;
		let mut state = env.state_vector_local(QubitCount::new(4)?)?;
		let local = state.deployment().local_amplitudes();
		let start = local
			.checked_mul(state.deployment().rank())
			.ok_or(quest::Error::Overflow)?;
		let values: Vec<_> = (0..16)
			.map(|i| Complex64::new(f64::from(i % 7) - 3., f64::from(i % 5) - 2.))
			.collect();
		let norm = values.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
		let values: Vec<_> = values.into_iter().map(|v| v.div(norm)).collect();
		let end = start.checked_add(local).ok_or(quest::Error::Overflow)?;
		let original = values.get(start..end).ok_or(quest::Error::Overflow)?;
		state.write_local_amplitudes(0, original)?;
		let bytes = env.view().allocated_bytes();
		forward.run(&mut state)?;
		for (offset, actual) in state.read_local_amplitudes(0, local)?.iter().enumerate() {
			let row = start.checked_add(offset).ok_or(quest::Error::Overflow)?;
			let expected = if row & 7 == 6 {
				values
					.get(row ^ 8)
					.copied()
					.ok_or(quest::Error::Overflow)?
					.mul(if row & 8 == 0 {
						Complex64::new(0., 1.)
					} else {
						Complex64::new(1., 0.)
					})
			} else {
				*values.get(row).ok_or(quest::Error::Overflow)?
			};
			expect_true!(actual.sub(expected).norm() < 1e-12);
		}
		inverse.run(&mut state)?;
		for (actual, expected) in state.read_local_amplitudes(0, local)?.iter().zip(original) {
			expect_true!(actual.sub(*expected).norm() < 1e-12);
		}
		expect_eq!(env.view().allocated_bytes(), bytes);
		if comm.size()? > 1 {
			for mode in [false, true] {
				let before = env.view().allocated_bytes();
				// Both schedules fit the partition. Identical entries differ only in attached
				// admission mode or evidence tolerance, and must fail canonical agreement.
				let admitted = mode || comm.rank()? != 0;
				let tolerance = if mode && comm.rank()? == 0 {
					1e-10
				} else {
					1e-12
				};
				expect_true!(
					env.prepare(program(matrix(admitted, tolerance)?, false, false)?)
						.is_err()
				);
				expect_eq!(env.view().allocated_bytes(), before);
			}
		}
	}
	Ok(())
}
