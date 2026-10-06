#![cfg(feature = "qsvt")]
use googletest::prelude::*;
use quest::qsvt::matching_lcu::MatchingLcuLimits;
use quest::{Complex64, Environment, QubitCount};
use quest_compile::{Control, ControlState, QuantumRegionBuilder};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsvt::{
	MatchingEncoding, MatchingShard, NumericalPolicy, ReplayEncoding, materialize_program,
	portfolio::{PortfolioLimits, WeightedLcu},
};
use std::ops::{Div, Mul, Sub};

#[gtest]
fn prepared_weighted_matching_is_whole_unitary_and_admits_before_prepare() -> googletest::Result<()>
{
	let environment = Environment::builder().build()?;
	compare_prepared_matching(&environment, false)?;
	compare_prepared_matching(&environment, true)
}
#[allow(
	clippy::too_many_lines,
	reason = "one native environment runs independent whole-register and underflow/padding differential fixtures"
)]
fn compare_prepared_matching(environment: &Environment, edge: bool) -> googletest::Result<()> {
	let p = NumericalPolicy::default();
	let mut model_terms = Vec::new();
	for ordinal in 0..if edge { 5 } else { 3 } {
		let index = f64::from(ordinal);
		let scale = if edge && ordinal == 4 { 1e-100 } else { 1. };
		let sparse = SparseMatrix::from_triplets(
			2,
			3,
			SparseFormat::Csr,
			vec![
				(0, 0, Complex64::new(1. + index, 1.).mul(scale)),
				(0, 1, Complex64::new(-1., index).mul(scale)),
				(0, 2, Complex64::new(0.3, 2. + index).mul(scale)),
				(1, 0, Complex64::new(0.2, 0.).mul(scale)),
			],
			SparseLimits::default(),
		)?;
		model_terms.push(MatchingEncoding::from_sparse(&sparse, p)?);
	}
	let weights = if edge {
		vec![
			Complex64::new(0., 2.),
			Complex64::new(0., 0.),
			Complex64::new(-0.25, 0.),
			Complex64::new(0.1, -0.2),
			Complex64::new(0., 1e-320),
		]
	} else {
		vec![
			Complex64::new(0., 2.),
			Complex64::new(-0.25, 0.),
			Complex64::new(0.1, -0.2),
		]
	};
	if edge {
		expect_eq!(
			weights.get(4).ok_or(quest::Error::Overflow)?.norm().mul(
				model_terms
					.get(4)
					.ok_or(quest::Error::Overflow)?
					.normalization()
					.get()
			),
			0.
		);
	}

	let portable = WeightedLcu::new(
		weights
			.clone()
			.into_iter()
			.zip(model_terms.clone())
			.collect(),
		PortfolioLimits::default(),
	)?;
	let count = QubitCount::new(9)?;
	let targets = vec![2, 0, 4, 1, 5];
	let terms = weights
		.clone()
		.into_iter()
		.zip(&model_terms)
		.map(|(weight, source)| {
			Ok((
				weight,
				environment.prepare_matching(
					MatchingShard::from_encoding(source, 0, 1, p)?,
					count,
					targets.clone(),
				)?,
			))
		})
		.collect::<quest::qsvt::Result<Vec<_>>>()?;
	let mut prepared =
		environment.prepare_matching_lcu(terms, vec![8, 3], MatchingLcuLimits::default())?;
	if !edge {
		let baseline = environment.allocated_bytes();
		let source = environment.prepare_matching(
			MatchingShard::from_encoding(
				model_terms.first().ok_or(quest::Error::Overflow)?,
				0,
				1,
				p,
			)?,
			count,
			targets.clone(),
		)?;
		let limits = MatchingLcuLimits {
			max_constructor_work: 250_000,
			max_local_bytes: 0,
			..MatchingLcuLimits::default()
		};
		let error = environment
			.prepare_matching_lcu(
				vec![(Complex64::new(1., 0.), source)],
				vec![0; 262_144],
				limits,
			)
			.err()
			.ok_or(quest::Error::Value("oversized selectors accepted"))?;
		expect_true!(
			error
				.to_string()
				.contains("weighted matching selector width")
		);
		expect_eq!(environment.allocated_bytes(), baseline);
	}
	let mut register = environment.state_vector(count)?;
	let state: Vec<_> = (0..512)
		.map(|i| Complex64::new(f64::from(i % 13) - 6., f64::from(i % 7) - 3.))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|a| a.div(norm)).collect();
	// A completed constructor can still reject the full application before PREP.
	let child_terms = weights
		.into_iter()
		.zip(&model_terms)
		.map(|(weight, source)| {
			Ok((
				weight,
				environment.prepare_matching(
					MatchingShard::from_encoding(source, 0, 1, p)?,
					count,
					targets.clone(),
				)?,
			))
		})
		.collect::<quest::qsvt::Result<Vec<_>>>()?;
	let mut low = MatchingLcuLimits::default();
	low.plan.max_bytes = 1024 * 1024;
	low.max_rank_work = 0;
	let mut rejected = environment.prepare_matching_lcu(child_terms, vec![8, 3], low)?;
	register.init_pure(&state)?;
	let rejected_bytes = environment.allocated_bytes();
	expect_true!(rejected.apply(&mut register, false, 0, 0).is_err());
	expect_eq!(register.amplitudes(0, 512)?, state);
	expect_eq!(environment.allocated_bytes(), rejected_bytes);
	drop(rejected);
	for positive in [false, true] {
		let mut builder = QuantumRegionBuilder::new(9, 0)?;
		let mapped = [2, 0, 4, 1, 5, 8, 3]
			.into_iter()
			.map(|i| builder.qubit(i))
			.collect::<quest_compile::Result<Vec<_>>>()?;
		builder.oracle(
			&portable.replay_oracle(p)?,
			&mapped,
			&[Control::new(
				builder.qubit(6)?,
				if positive {
					ControlState::One
				} else {
					ControlState::Zero
				},
			)],
		)?;
		let unitary = materialize_program(&builder.finish()?.bind(&[])?, p)?;
		for adjoint in [false, true] {
			register.init_pure(&state)?;
			let bytes = environment.allocated_bytes();
			expect_true!(prepared.admit_apply(&register, adjoint, 1 << 2, 0).is_err());
			let admitted =
				prepared.admit_apply(&register, adjoint, 1 << 6, usize::from(positive) << 6)?;
			expect_gt!(admitted.maximum_rank_work, 0);
			expect_eq!(register.amplitudes(0, 512)?, state);
			prepared.apply(&mut register, adjoint, 1 << 6, usize::from(positive) << 6)?;
			let actual = register.amplitudes(0, 512)?;
			for (row, value) in actual.iter().enumerate() {
				let expected = state
					.iter()
					.enumerate()
					.map(|(column, &amplitude)| {
						if adjoint {
							unitary[(column, row)].conj().mul(amplitude)
						} else {
							unitary[(row, column)].mul(amplitude)
						}
					})
					.sum::<Complex64>();
				expect_true!(value.sub(expected).norm() < 2e-12);
			}
			expect_eq!(environment.allocated_bytes(), bytes);
		}
	}
	Ok(())
}
