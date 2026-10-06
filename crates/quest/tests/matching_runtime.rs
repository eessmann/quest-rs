#![cfg(feature = "qsvt")]
use googletest::prelude::*;
use quest::{Complex64, Environment, QubitCount};
use quest_compile::{Control, ControlState, QuantumRegionBuilder};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsvt::{MatchingEncoding, MatchingShard, NumericalPolicy, materialize_program};
use std::ops::{Div, Mul, Sub};

#[gtest]
fn prepared_matching_preserves_all_sectors_controls_spectators_and_adjoint()
-> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let matrix = SparseMatrix::from_triplets(
		2,
		3,
		SparseFormat::Csr,
		vec![
			(0, 0, Complex64::new(1.0, 1.0)),
			(0, 1, Complex64::new(-1.0, 0.0)),
			(0, 2, Complex64::new(0.0, 2.0)),
			(1, 0, Complex64::new(0.2, 0.0)),
		],
		SparseLimits::default(),
	)?;
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	let shard = MatchingShard::from_encoding(&encoding, 0, 1, policy)?;
	let targets = vec![2, 0, 4, 1, 5];
	let oracle = encoding.to_oracle(policy)?;
	let environment = Environment::builder().build()?;
	let mut prepared = environment.prepare_matching(shard, QubitCount::new(7)?, targets.clone())?;
	let mut register = environment.state_vector(QubitCount::new(7)?)?;
	let admission = register.admit_native_matrix(
		quest::native_admission::MatrixKind::CompMatr,
		2,
		false,
		1,
		quest::MemoryBudget::default(),
	)?;
	expect_true!(admission.replicated);
	expect_eq!(admission.local_matrix_elements, 16);
	expect_eq!(
		admission.local_register_elements,
		register.deployment().local_amplitudes()
	);
	expect_gt!(admission.peak_rank_bytes, environment.allocated_bytes());
	let state: Vec<_> = (0..128)
		.map(|i| Complex64::new(f64::from(i % 13) - 6.0, f64::from(i % 7) - 3.0))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|value| value.div(norm)).collect();
	for outer in [false, true] {
		let mut builder = QuantumRegionBuilder::new(7, 0)?;
		let mapped = targets
			.iter()
			.map(|&position| builder.qubit(position))
			.collect::<quest_compile::Result<Vec<_>>>()?;
		builder.oracle(
			&oracle,
			&mapped,
			&[Control::new(
				builder.qubit(6)?,
				if outer {
					ControlState::One
				} else {
					ControlState::Zero
				},
			)],
		)?;
		let program = builder.finish()?.bind(&[])?;
		let unitary = materialize_program(&program, policy)?;
		let expected: Vec<_> = (0..128)
			.map(|row| {
				(0..128)
					.map(|col| unitary[(row, col)].mul(state[col]))
					.sum::<Complex64>()
			})
			.collect();
		register.init_pure(&state)?;
		let bytes = environment.allocated_bytes();
		expect_true!(prepared.admit_apply(&register, false, 1 << 2, 0).is_err());
		expect_true!(
			prepared
				.admit_apply(&register, false, 1 << 6, 1 << 3)
				.is_err()
		);
		let cost = prepared.admit_apply(&register, false, 1 << 6, usize::from(outer) << 6)?;
		expect_gt!(cost.maximum_rank_work, 0);
		expect_eq!(register.amplitudes(0, 128)?, state);
		expect_eq!(environment.allocated_bytes(), bytes);
		prepared.apply(&mut register, false, 1 << 6, usize::from(outer) << 6)?;
		let actual = register.amplitudes(0, 128)?;
		for (actual, expected) in actual.iter().zip(&expected) {
			expect_true!(actual.sub(*expected).norm() < 1e-12);
		}
		prepared.apply(&mut register, true, 1 << 6, usize::from(outer) << 6)?;
		let actual = register.amplitudes(0, 128)?;
		for (actual, expected) in actual.iter().zip(&state) {
			expect_true!(actual.sub(*expected).norm() < 1e-12);
		}
		expect_eq!(environment.allocated_bytes(), bytes);
	}
	Ok(())
}
