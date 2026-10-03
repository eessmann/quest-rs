#![cfg(feature = "qsvt")]
use googletest::prelude::*;
use quest::{Complex64, Environment, QubitCount};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{DenseEncodingBuilder, NumericalPolicy, TransformBuilder};
use std::ops::Mul;

fn isolated(name: &str, body: impl FnOnce() -> googletest::Result<()>) -> googletest::Result<()> {
	if std::env::var("QUEST_QSVT_TEST").as_deref() == Ok(name) {
		return body();
	}
	let status = std::process::Command::new(std::env::current_exe()?)
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_QSVT_TEST", name)
		.status()?;
	expect_true!(status.success());
	Ok(())
}
#[gtest]
fn prepared_transform_preserves_phase_mass_and_reuses_resources() -> googletest::Result<()> {
	isolated(
		"prepared_transform_preserves_phase_mass_and_reuses_resources",
		|| {
			let snapshot = {
				let environment = Environment::builder().build()?;
				let matrix = faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0.3, 0.4));
				let encoding =
					DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
						.normalization(1.0)?
						.build()?;
				let phases =
					PhaseSequence::<WxSymmetric>::builder(vec![std::f64::consts::FRAC_PI_4; 2])
						.build()?;
				let transform = TransformBuilder::new()
					.encoding(encoding)
					.standard(phases)
					.build()?;
				let reference = transform.materialize_block()?;
				let width = transform.operands().num_qubits();
				let baseline = environment.allocated_bytes();
				{
					let _admitted = environment.qsvt().transform(transform.clone()).admit()?;
					expect_gt!(environment.allocated_bytes(), baseline);
				}
				expect_eq!(environment.allocated_bytes(), baseline);
				let admitted = environment.qsvt().transform(transform).admit()?;
				let mut prepared = admitted.prepare()?;
				let mut register = environment.state_vector(QubitCount::new(width)?)?;
				let bytes = environment.allocated_bytes();
				for _ in 0..3 {
					register.init_zero()?;
					let result = prepared.run(&mut register)?;
					expect_that!(
						result.mass().retained(),
						near(reference[(0, 0)].norm_sqr(), 1e-12)
					);
					let actual = result.logical_snapshot()?;
					expect_that!(actual[(0, 0)].re, near(reference[(0, 0)].re, 1e-12));
					expect_that!(actual[(0, 0)].im, near(reference[(0, 0)].im, 1e-12));
					let _ = result.release();
					expect_eq!(environment.allocated_bytes(), bytes);
				}
				register.init_zero()?;
				let result = prepared.run(&mut register)?.condition()?;
				expect_that!(result.register().total_probability()?, near(1.0, 1e-12));
				result.logical_snapshot()?
			};
			expect_false!(quest_sys::is_quest_env_init());
			expect_that!(snapshot[(0, 0)].norm_sqr(), near(1.0, 1e-12));
			Ok(())
		},
	)
}

#[gtest]
fn rectangular_generalized_routes_match_full_subnormalized_references() -> googletest::Result<()> {
	isolated(
		"rectangular_generalized_routes_match_full_subnormalized_references",
		|| {
			use quest_qsp::ControlSequence;
			use quest_qsvt::OperandLayout;
			let environment = Environment::builder().build()?;
			let matrix = faer::Mat::from_fn(2, 3, |r, c| {
				Complex64::new(
					0.05 * f64::from(u32::try_from(r.saturating_add(c).saturating_add(1)).unwrap()),
					0.03 * f64::from(
						i32::try_from(r)
							.unwrap()
							.saturating_sub(i32::try_from(c).unwrap()),
					),
				)
			});
			let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
				.normalization(1.0)?
				.build()?;
			let controls = ControlSequence::builder()
				.angles(&[0.21, -0.34], &[0.37, -0.28])?
				.build()?;
			let build = || {
				TransformBuilder::new()
					.encoding(encoding.clone())
					.operands(OperandLayout::new(5, vec![4, 1, 3], 2, Some(0)).unwrap())
			};
			let transforms = [
				build()
					.hermitianized_full(
						quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
							controls.clone(),
						),
					)
					.build()?,
				build()
					.hermitianized_even(
						quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
							controls.clone(),
						)
						.even_component(),
					)
					.build()?,
				build()
					.hermitianized_odd(
						quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
							controls.clone(),
						)
						.odd_component(),
					)
					.build()?,
				build()
					.multiplication_even(
						quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(
							controls.clone(),
						),
					)
					.build()?,
				build()
					.multiplication_odd(
						quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls),
					)
					.build()?,
			];
			for transform in transforms {
				let expected = transform.materialize_block()?;
				let basis = transform
					.input()
					.materialize_isometry(5, NumericalPolicy::default())?;
				let amplitudes: Vec<_> = (0..basis.nrows()).map(|r| basis[(r, 0)]).collect();
				let mut prepared = environment.qsvt().transform(transform).prepare()?;
				let mut register = environment.state_vector(QubitCount::new(5)?)?;
				register.init_pure(&amplitudes)?;
				let before = environment.allocated_bytes();
				let result = prepared.run(&mut register)?;
				let actual = result.logical_snapshot()?;
				let mass: f64 = (0..expected.nrows())
					.map(|r| expected[(r, 0)].norm_sqr())
					.sum();
				expect_that!(result.mass().retained(), near(mass, 2e-12));
				for r in 0..expected.nrows() {
					expect_that!(actual[(r, 0)].re, near(expected[(r, 0)].re, 2e-12));
					expect_that!(actual[(r, 0)].im, near(expected[(r, 0)].im, 2e-12));
				}
				let _ = result.release();
				expect_eq!(environment.allocated_bytes(), before);
			}
			Ok(())
		},
	)
}

#[gtest]
fn zero_success_and_preparation_failure_release_borrows_and_storage() -> googletest::Result<()> {
	isolated(
		"zero_success_and_preparation_failure_release_borrows_and_storage",
		|| {
			let environment = Environment::builder().build()?;
			let matrix = faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0.0, 0.0));
			let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
				.normalization(1.0)?
				.build()?;
			let phases = PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?;
			let transform = TransformBuilder::new()
				.encoding(encoding)
				.standard(phases)
				.build()?;
			let mut prepared = environment.qsvt().transform(transform).prepare()?;
			let mut register = environment.state_vector(QubitCount::new(2)?)?;
			register.x(0)?; // Exact input projection annihilation, not a rounded zero polynomial.
			let result = prepared.run(&mut register)?;
			expect_eq!(result.mass().input(), 0.0);
			expect_true!(matches!(
				result.condition(),
				Err(quest::qsvt::Error::ZeroSuccess)
			));
			register.init_zero()?;
			Ok(())
		},
	)
}

#[gtest]
fn hadamard_overlap_retains_complex_phase_and_probability_ledger() -> googletest::Result<()> {
	isolated(
		"hadamard_overlap_retains_complex_phase_and_probability_ledger",
		|| {
			let environment = Environment::builder().build()?;
			let value = Complex64::new(0.3, 0.4);
			let matrix = faer::Mat::from_fn(1, 1, |_, _| value);
			let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
				.normalization(1.0)?
				.build()?;
			let transform = TransformBuilder::new()
				.encoding(encoding)
				.multiplication_odd(
					quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(
						quest_qsp::ControlSequence::builder()
							.angles(&[std::f64::consts::FRAC_PI_2], &[0.0])?
							.build()?,
					),
				)
				.build()?;
			let expected = transform.materialize_block()?[(0, 0)];
			let mut prepared = environment
				.qsvt()
				.transform(transform)
				.overlap()
				.input(vec![Complex64::new(1.0, 0.0)])
				.reference(vec![Complex64::new(0.0, 1.0)])
				.prepare()?;
			let before = environment.allocated_bytes();
			for _ in 0..3 {
				let observed = prepared.run()?;
				let overlap = Complex64::new(0.0, -1.0).mul(expected);
				expect_that!(observed.overlap().re, near(overlap.re, 2e-12));
				expect_that!(observed.overlap().im, near(overlap.im, 2e-12));
				expect_that!(
					observed.retained_mass(),
					near(1.0_f64.midpoint(expected.norm_sqr()), 2e-12)
				);
				expect_that!(observed.transformed_norm(), near(expected.norm(), 2e-12));
				expect_that!(
					observed.active_mass(),
					near(expected.norm_sqr() / 2.0, 2e-12)
				);
				expect_eq!(environment.allocated_bytes(), before);
			}
			Ok(())
		},
	)
}

#[gtest]
fn complex_isometry_projection_matches_canonical_projector_and_adjoint_decode()
-> googletest::Result<()> {
	isolated(
		"complex_isometry_projection_matches_canonical_projector_and_adjoint_decode",
		|| {
			use quest_qsvt::{EncodingBuilder, Left, LogicalSpace, Right};
			let environment = Environment::builder().build()?;
			let policy = NumericalPolicy::default();
			let matrix = faer::Mat::from_fn(1, 1, |_, _| Complex64::new(0.3, 0.4));
			let source = DenseEncodingBuilder::new(matrix.as_ref(), policy)?
				.normalization(1.0)?
				.build()?;
			let basis = faer::Mat::from_fn(2, 1, |r, _| {
				if r == 0 {
					Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0)
				} else {
					Complex64::new(0.0, std::f64::consts::FRAC_1_SQRT_2)
				}
			});
			let projector =
				faer::Mat::from_fn(2, 2, |r, c| basis[(r, 0)].mul(basis[(c, 0)].conj()));
			let encoding = EncodingBuilder::new()
				.oracle(source.oracle().clone())
				.left(LogicalSpace::<Left>::from_dense_projector(
					projector.as_ref(),
					basis.as_ref(),
					policy,
				)?)
				.right(LogicalSpace::<Right>::from_isometry(
					basis.as_ref(),
					policy,
				)?)
				.normalization(1.0)?
				.build()?;
			let transform = TransformBuilder::new()
				.encoding(encoding)
				.standard(PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?)
				.build()?;
			let expected = transform.materialize_block()?;
			let embedding = transform.input().materialize_isometry(2, policy)?;
			let amplitudes: Vec<_> = (0..embedding.nrows()).map(|r| embedding[(r, 0)]).collect();
			let mut prepared = environment.qsvt().transform(transform).prepare()?;
			let mut register = environment.state_vector(QubitCount::new(2)?)?;
			register.init_pure(&amplitudes)?;
			let result = prepared.run(&mut register)?;
			let actual = result.logical_snapshot()?;
			expect_that!(actual[(0, 0)].re, near(expected[(0, 0)].re, 2e-12));
			expect_that!(actual[(0, 0)].im, near(expected[(0, 0)].im, 2e-12));
			expect_that!(
				result.mass().retained(),
				near(expected[(0, 0)].norm_sqr(), 2e-12)
			);
			Ok(())
		},
	)
}
