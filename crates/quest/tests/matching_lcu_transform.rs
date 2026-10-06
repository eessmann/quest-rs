#![cfg(feature = "qsvt")]
use googletest::prelude::*;
use quest::qsvt::matching_lcu::MatchingLcuLimits;
use quest::qsvt::matching_lcu_transform::TransformExecutionLimits;
use quest::{Complex64, Environment, QubitCount};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::replay_transform::{ReplayTransform, TransformSchedule};
use quest_qsvt::{
	MatchingEncoding, MatchingShard, NumericalPolicy,
	portfolio::{PortfolioLimits, WeightedLcu},
};
use std::ops::{Div, Mul, Sub};

#[gtest]
fn prepared_lcu_transform_is_whole_unitary_and_admits_before_response() -> googletest::Result<()> {
	let environment = Environment::builder().build()?;
	compare_prepared_matching(&environment, false)?;
	compare_prepared_matching(&environment, true)?;
	independently_known_blocks_and_inverse(&environment)
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
	let sequence = PhaseSequence::<WxSymmetric>::builder(vec![0.12, -0.3, -0.3, 0.12]).build()?;
	let portable = ReplayTransform::new(portable, sequence.clone(), p)?;
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
	let source =
		environment.prepare_matching_lcu(terms, vec![8, 3], MatchingLcuLimits::default())?;
	let schedule = TransformSchedule::from_phase_sequence(
		source.plan().descriptor().clone(),
		sequence.clone(),
		p,
	)?;
	let mut prepared = environment.prepare_matching_lcu_transform(
		source,
		7,
		schedule,
		TransformExecutionLimits::default(),
	)?;
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
	let rejected_source = environment.prepare_matching_lcu(child_terms, vec![8, 3], low)?;
	let schedule = TransformSchedule::from_phase_sequence(
		rejected_source.plan().descriptor().clone(),
		sequence,
		p,
	)?;
	let mut rejected = environment.prepare_matching_lcu_transform(
		rejected_source,
		7,
		schedule,
		TransformExecutionLimits {
			max_queries: 0,
			..Default::default()
		},
	)?;
	register.init_pure(&state)?;
	let rejected_bytes = environment.allocated_bytes();
	expect_true!(rejected.apply(&mut register, false, 0, 0).is_err());
	expect_eq!(register.amplitudes(0, 512)?, state);
	expect_eq!(environment.allocated_bytes(), rejected_bytes);
	drop(rejected);
	for positive in [false, true] {
		for adjoint in [false, true] {
			register.init_pure(&state)?;
			let bytes = environment.allocated_bytes();
			expect_true!(prepared.admit_apply(&register, adjoint, 1 << 2, 0).is_err());
			let admitted =
				prepared.admit_apply(&register, adjoint, 1 << 6, usize::from(positive) << 6)?;
			expect_gt!(admitted.maximum_rank_work, 0);
			expect_eq!(register.amplitudes(0, 512)?, state);
			prepared.apply(&mut register, adjoint, 1 << 6, usize::from(positive) << 6)?;
			let mut expected = state.clone();
			portable.apply_mapped_reference(
				&mut expected,
				&[2, 0, 4, 1, 5, 8, 3, 7],
				1 << 6,
				usize::from(positive) << 6,
				adjoint,
				p,
			)?;
			for (value, expected) in register.amplitudes(0, 512)?.iter().zip(expected) {
				expect_true!(value.sub(expected).norm() < 2e-12);
			}

			expect_eq!(environment.allocated_bytes(), bytes);
		}
	}
	Ok(())
}

#[allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::too_many_lines,
	reason = "Independent 2x2 Chebyshev recurrence and inverse oracle use fixed 32-amplitude native fixtures"
)]
fn independently_known_blocks_and_inverse(env: &Environment) -> googletest::Result<()> {
	use quest_qsvt::reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence};
	let p = NumericalPolicy::default();
	let count = QubitCount::new(5)?;
	let make_source = || -> quest::qsvt::Result<_> {
		let mut terms = Vec::new();
		for (weight, flip) in [
			(Complex64::new(0.5, 0.), false),
			(Complex64::new(0., -0.125), true),
			(Complex64::new(0.125, 0.), false),
		] {
			let matrix = SparseMatrix::from_triplets(
				2,
				2,
				SparseFormat::Csr,
				(0..2)
					.map(|i| (i, if flip { 1 - i } else { i }, Complex64::new(1., 0.)))
					.collect(),
				SparseLimits::default(),
			)
			.map_err(quest_qsvt::Error::from)?;
			let model = MatchingEncoding::from_sparse(&matrix, p)?;
			terms.push((
				weight,
				env.prepare_matching(
					MatchingShard::from_encoding(&model, 0, 1, p)?,
					count,
					vec![0, 1],
				)?,
			));
		}
		env.prepare_matching_lcu(terms, vec![2, 3], MatchingLcuLimits::default())
	};
	let mut state = env.state_vector(count)?;
	let sigma = 0.625_f64.hypot(0.125);
	for degree in [1, 3, 5] {
		let source = make_source()?;
		let alpha = source.plan().descriptor().normalization;
		let descriptor = source.plan().descriptor().clone();
		expect_true!(descriptor.errors.preparation.is_none());
		expect_true!(descriptor.errors.encoding.is_none());
		let mut angles = vec![0.; degree + 1];
		angles[0] = std::f64::consts::FRAC_PI_4;
		angles[degree] = std::f64::consts::FRAC_PI_4;
		let phases = PhaseSequence::<WxSymmetric>::builder(angles).build()?;
		let schedule = TransformSchedule::from_phase_sequence(descriptor, phases, p)?;
		expect_true!(schedule.projector_response_bound().is_none());
		let mut prepared = env.prepare_matching_lcu_transform(
			source,
			4,
			schedule,
			TransformExecutionLimits::default(),
		)?;
		// Independently T_n(x)=2xT_(n-1)(x)-T_(n-2)(x), without replay/QSP expected gates.
		let x = sigma / alpha;
		let (mut previous, mut current) = (1., x);
		for _ in 2..=degree {
			let next = (2. * x).mul_add(current, -previous);
			previous = current;
			current = next;
		}
		for column in 0..2 {
			let mut input = vec![Complex64::new(0., 0.); 32];
			input[column << 1] = Complex64::new(1., 0.);
			state.init_pure(&input)?;
			let receipt = prepared.apply(&mut state, false, 0, 0)?;
			expect_eq!(receipt.source_queries, 2 * degree);
			let output = state.amplitudes(0, 32)?;
			let factor = current / sigma;
			for row in 0..2 {
				let expected = if row == column {
					Complex64::new(0.625 * factor, 0.)
				} else {
					Complex64::new(0., -0.125 * factor)
				};
				expect_true!((output[row << 1] - expected).norm() < 3e-12);
			}
			expect_true!(
				current
					.mul_add(-current, output[0].norm_sqr() + output[2].norm_sqr())
					.abs()
					< 5e-12
			);
			expect_true!((output.iter().map(Complex64::norm_sqr).sum::<f64>() - 1.).abs() < 5e-12);
		}
	}
	let source = make_source()?;
	let alpha = source.plan().descriptor().normalization;
	let spectrum = SpectralBounds::new(
		sigma,
		sigma,
		SpectralEvidence::Analytic {
			description: "A=.625I+.125iX; A†A=(.625²+.125²)I, both coordinates in range".into(),
		},
	)?;
	let polynomial = ReciprocalPolynomial::geometric(&spectrum, alpha, 1e-3, 255, p)?;
	let frozen = polynomial.synthesize(quest_qsp::Policy::default())?;
	let schedule = TransformSchedule::from_phase_sequence(
		source.plan().descriptor().clone(),
		frozen.phase_sequence(),
		p,
	)?;
	// Neither a synthesis diagnostic nor measured state success establishes uniform execution error.
	expect_true!(schedule.projector_response_bound().is_none());
	expect_true!(schedule.descriptor().errors.encoding.is_none());
	let mut prepared = env.prepare_matching_lcu_transform(
		source,
		4,
		schedule,
		TransformExecutionLimits::default(),
	)?;
	let mut input = vec![Complex64::new(0., 0.); 32];
	input[0] = Complex64::new(1., 0.);
	state.init_pure(&input)?;
	prepared.apply(&mut state, false, 0, 0)?;
	let output = state.amplitudes(0, 32)?;
	let scale = polynomial.physical_rescaling(1.)?;
	let denominator = sigma.powi(2);
	let expected = [
		Complex64::new(0.625 / denominator, 0.),
		Complex64::new(0., -0.125 / denominator),
	];
	for (row, expected) in expected.into_iter().enumerate() {
		expect_true!((output[row << 1] * scale - expected).norm() < 0.01);
	}
	let success = output[0].norm_sqr() + output[2].norm_sqr();
	expect_true!(success > 0.);
	expect_true!(success < 1.);
	expect_true!((output.iter().map(Complex64::norm_sqr).sum::<f64>() - 1.).abs() < 5e-12);
	// This is empirical inverse evidence. A conditional residual bound remains unavailable
	// because source/preparation/native/RHS uniform error premises have not been certified.
	Ok(())
}
