#![cfg(feature = "qsvt")]
use googletest::prelude::*;
use quest::{
	Complex64, Control, ControlState, Environment, MatrixPolicy, NumericalOperator, OracleFragment,
	QuantumRegionBuilder, QubitCount,
};
use std::ops::{Div, Mul, Sub};

fn isolated(name: &str, body: impl FnOnce() -> googletest::Result<()>) -> googletest::Result<()> {
	if std::env::var("QUEST_UNITARY_TEST").as_deref() == Ok(name) {
		return body();
	}
	let status = std::process::Command::new(std::env::current_exe()?)
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_UNITARY_TEST", name)
		.status()?;
	expect_true!(status.success());
	Ok(())
}
fn operator(scale: f64, admitted: bool) -> quest::Result<NumericalOperator> {
	let source = faer::Mat::from_fn(2, 2, |r, c| match (r, c) {
		(0, 1) => Complex64::new(0., scale),
		(1, 0) => Complex64::new(scale, 0.),
		_ => Complex64::new(0., 0.),
	});
	let matrix = NumericalOperator::from_view(&source, MatrixPolicy::default())?;
	Ok(if admitted {
		matrix.admit_unitary(1e-3, MatrixPolicy::default(), 64)?
	} else {
		matrix
	})
}
fn input() -> Vec<Complex64> {
	let values: Vec<_> = (0..32)
		.map(|i| Complex64::new(f64::from(i % 7) - 3., f64::from(i % 11) - 5.))
		.collect();
	let norm = values.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	values.into_iter().map(|v| v.div(norm)).collect()
}
fn action(
	state: &[Complex64],
	target: usize,
	controls: &[(usize, bool)],
	scale: f64,
	adjoint: bool,
) -> Vec<Complex64> {
	state
		.iter()
		.enumerate()
		.map(|(row, value)| {
			if controls
				.iter()
				.all(|&(wire, positive)| ((row >> wire) & 1 != 0) == positive)
			{
				let phase = match (row >> target & 1, adjoint) {
					(0, false) => Complex64::new(0., scale),
					(1, true) => Complex64::new(0., -scale),
					_ => Complex64::new(scale, 0.),
				};
				state
					.get(row ^ (1 << target))
					.copied()
					.unwrap_or_default()
					.mul(phase)
			} else {
				*value
			}
		})
		.collect()
}
#[gtest]
fn explicit_dense_unitaries_and_general_operators_match_whole_register_density()
-> googletest::Result<()> {
	isolated(
		"explicit_dense_unitaries_and_general_operators_match_whole_register_density",
		|| {
			let environment = Environment::builder().build()?;
			for (scale, admitted, oracle) in
				[(1., true, false), (1., true, true), (1.3, false, false)]
			{
				let matrix = operator(scale, admitted)?;
				let body = if oracle {
					let mut b = QuantumRegionBuilder::new(1, 0)?;
					b.numerical(matrix.clone(), &[b.qubit(0)?], &[])?;
					Some(
						OracleFragment::builder(b.finish()?.bind(&[])?)
							.matrix_tolerance(1e-12)?
							.build()?,
					)
				} else {
					None
				};
				let mut builder = QuantumRegionBuilder::new(5, 0)?;
				let mut expected = input();
				builder.numerical(matrix.clone(), &[builder.qubit(1)?], &[])?;
				expected = action(&expected, 1, &[], scale, false);
				for (target, profile, adjoint) in [
					(2, [(0, false), (1, true), (3, true)], false),
					(4, [(3, false), (2, true), (0, true)], oracle),
					(2, [(0, false), (1, true), (3, true)], false),
				] {
					let controls = profile
						.iter()
						.map(|&(wire, positive)| {
							Ok(Control::new(
								builder.qubit(wire)?,
								if positive {
									ControlState::One
								} else {
									ControlState::Zero
								},
							))
						})
						.collect::<quest::Result<Vec<_>>>()?;
					if let Some(body) = &body {
						builder.oracle(
							&if adjoint {
								body.adjoint()
							} else {
								body.clone()
							},
							&[builder.qubit(target)?],
							&controls,
						)?;
					} else {
						builder.numerical(matrix.clone(), &[builder.qubit(target)?], &controls)?;
					}
					expected = action(&expected, target, &profile, scale, adjoint);
				}
				let mut prepared = environment.prepare(
					quest::Program::from_region(builder.finish()?, &[])?
						.verify()?
						.lower()?
						.plan()?,
				)?;
				let mut state = environment.state_vector(QubitCount::new(5)?)?;
				let mut density = environment.density_matrix(QubitCount::new(5)?)?;
				state.init_pure(&input())?;
				density.init_pure(&input())?;
				let bytes = environment.allocated_bytes();
				prepared.run(&mut state, &quest::RunInputs::default())?;
				prepared.run(&mut density, &quest::RunInputs::default())?;
				expect_eq!(environment.allocated_bytes(), bytes);
				let actual = state.snapshot()?;
				let rho = density.snapshot()?;
				for (row, &value) in expected.iter().enumerate() {
					expect_true!(actual[(row, 0)].sub(value).norm() < 1e-12);
					for (col, &other) in expected.iter().enumerate() {
						expect_true!(rho[(row, col)].sub(value.mul(other.conj())).norm() < 1e-12);
					}
				}
			}
			Ok(())
		},
	)
}
#[gtest]
fn loose_unitary_evidence_rejects_before_native_allocation_but_general_still_runs()
-> googletest::Result<()> {
	isolated(
		"loose_unitary_evidence_rejects_before_native_allocation_but_general_still_runs",
		|| {
			let env = Environment::builder().build()?;
			for admitted in [true, false] {
				let mut b = QuantumRegionBuilder::new(2, 0)?;
				b.numerical(
					operator(1.000_001, admitted)?,
					&[b.qubit(0)?],
					&[Control::new(b.qubit(1)?, ControlState::Zero)],
				)?;
				let before = env.allocated_bytes();
				let prepared = env.prepare(
					quest::Program::from_region(b.finish()?, &[])?
						.verify()?
						.lower()?
						.plan()?,
				);
				if admitted {
					expect_true!(prepared.is_err());
					expect_eq!(env.allocated_bytes(), before);
				} else {
					let mut prepared = prepared?;
					let mut state = env.state_vector(QubitCount::new(2)?)?;
					prepared.run(&mut state, &quest::RunInputs::default())?;
					expect_true!(
						state
							.amplitude(1)?
							.sub(Complex64::new(1.000_001, 0.))
							.norm()
							< 1e-12
					);
				}
			}
			Ok(())
		},
	)
}
