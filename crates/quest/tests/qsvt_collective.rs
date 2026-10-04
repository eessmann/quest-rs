#![cfg(all(feature = "qsvt", feature = "mpi", quest_native_mpi))]
use googletest::prelude::*;
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::{Complex64, MemoryBudget, QubitCount};
use quest_qsvt::{DenseEncodingBuilder, NumericalPolicy, TransformBuilder};
use std::ops::Mul;
fn ranks(
	name: &str,
	count: &str,
	body: impl FnOnce() -> googletest::Result<()>,
) -> googletest::Result<()> {
	if std::env::var("QUEST_COLLECTIVE_TEST").as_deref() == Ok(name) {
		return body();
	}
	let output = std::process::Command::new("timeout")
		.args(["60s", "mpiexec", "-n", count])
		.arg(std::env::current_exe()?)
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_COLLECTIVE_TEST", name)
		.output()?;
	verify_that!(output.status.success(), eq(true)).with_failure_message(|| {
		format!(
			"{}\n{}\n{}",
			output.status,
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		)
	})?;
	Ok(())
}
fn transform(value: Complex64) -> googletest::Result<quest_qsvt::ValidatedTransform> {
	let matrix = faer::Mat::from_fn(1, 1, |_, _| value);
	let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
		.normalization(1.0)?
		.build()?;
	Ok(TransformBuilder::new()
		.encoding(encoding)
		.multiplication_odd(
			quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(
				quest_qsp::ControlSequence::builder()
					.angles(&[std::f64::consts::FRAC_PI_2], &[0.0])?
					.build()?,
			),
		)
		.build()?)
}

#[gtest]
fn collective_compact_range_projection_and_descriptor_disagreement() -> googletest::Result<()> {
	ranks(
		"collective_compact_range_projection_and_descriptor_disagreement",
		"2",
		|| {
			use quest_compile::{OracleFragment, QuantumRegionBuilder};
			use quest_qsvt::{
				EncodingBuilder, ExplicitUnitaryPremise, Left, LogicalSpace, OperandLayout, Right,
			};
			let runtime = MpiRuntime::initialize()?;
			let comm = runtime.world()?;
			let environment = CollectiveEnvironment::builder(&comm)?.build()?;
			let build = |start| -> quest_qsvt::Result<_> {
				let policy = NumericalPolicy::default();
				let oracle =
					OracleFragment::builder(QuantumRegionBuilder::new(3, 0)?.finish()?.bind(&[])?)
						.matrix_tolerance(1e-12)?
						.build()?;
				let encoding = EncodingBuilder::new()
					.oracle(oracle)
					.left(LogicalSpace::<Left>::logical_range(8, start..7, policy)?)
					.right(LogicalSpace::<Right>::logical_range(8, start..7, policy)?)
					.normalization(1.0)?
					.unitarity_assumption(ExplicitUnitaryPremise::new("identity circuit")?)
					.build()?;
				TransformBuilder::new()
					.encoding(encoding)
					.operands(OperandLayout::canonical(3, false)?.with_idle_high_qubits(1)?)
					.standard(
						quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![
							std::f64::consts::FRAC_PI_4,
						])
						.build()?,
					)
					.build()
			};
			let mismatched = build(if comm.rank()? == 0 { 1 } else { 2 })?;
			expect_true!(environment.qsvt().transform(mismatched).prepare().is_err());
			let transform = build(1)?;
			let mut prepared = environment.qsvt().transform(transform).prepare()?;
			let mut register = environment.state_vector(QubitCount::new(5)?)?;
			register.init_plus()?;
			let result = prepared.run(&mut register)?;
			expect_that!(result.mass().retained(), near(3.0 / 32.0, 1e-12));
			let conditioned = result.condition()?;
			expect_that!(
				conditioned.register().total_probability()?,
				near(1.0, 1e-12)
			);
			Ok(())
		},
	)
}
#[gtest]
fn collective_qsvt_mass_conditioning_and_complex_hadamard() -> googletest::Result<()> {
	ranks(
		"collective_qsvt_mass_conditioning_and_complex_hadamard",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				let transform = transform(Complex64::new(0.3, 0.4))?;
				let expected = transform.materialize_block()?[(0, 0)];
				let mut register =
					env.state_vector(QubitCount::new(transform.operands().num_qubits())?)?;
				let mut prepared = env.qsvt().transform(transform.clone()).prepare()?;
				let mut overlap = env
					.qsvt()
					.transform(transform)
					.overlap()
					.input(vec![Complex64::new(1., 0.)])
					.reference(vec![Complex64::new(0., 1.)])
					.prepare()?;
				let bytes = env.view().allocated_bytes();
				for _ in 0..3 {
					register.init_zero()?;
					let result = prepared.run(&mut register)?;
					expect_that!(result.mass().retained(), near(expected.norm_sqr(), 2e-12));
					let conditioned = result.condition()?;
					expect_that!(conditioned.register().total_probability()?, near(1., 2e-12));
					let _ = conditioned.release();
					let observed = overlap.run()?;
					let reference = Complex64::new(0., -1.).mul(expected);
					expect_that!(observed.overlap().re, near(reference.re, 2e-12));
					expect_that!(observed.overlap().im, near(reference.im, 2e-12));
					expect_that!(
						observed.active_mass(),
						near(expected.norm_sqr() / 2., 2e-12)
					);
					expect_eq!(env.view().allocated_bytes(), bytes);
				}
			}
			expect_true!(comm.all_agree(true)?);
			expect_true!(CollectiveEnvironment::builder(&comm)?.build().is_err());
			expect_true!(runtime.is_active()?);
			Ok(())
		},
	)
}
#[gtest]
fn collective_qsvt_rejects_transform_vector_and_budget_disagreement() -> googletest::Result<()> {
	ranks(
		"collective_qsvt_rejects_transform_vector_and_budget_disagreement",
		"2",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			{
				let env = CollectiveEnvironment::builder(&comm)?
					.memory_budget(MemoryBudget::new(if comm.rank()? == 0 {
						16_000
					} else {
						1_000_000
					}))
					.build()?;
				let rank = env.rank()?;
				expect_true!(
					env.qsvt()
						.transform(transform(Complex64::new(
							0.3,
							if rank == 0 { 0.4 } else { -0.4 }
						))?)
						.prepare()
						.is_err()
				);
				expect_eq!(env.view().allocated_bytes(), 0);
				let t = transform(Complex64::new(0.3, 0.4))?;
				expect_true!(
					env.qsvt()
						.transform(t.clone())
						.overlap()
						.input(vec![Complex64::new(1., 0.)])
						.reference(vec![Complex64::new(if rank == 0 { 1. } else { -1. }, 0.)])
						.prepare()
						.is_err()
				);
				expect_eq!(env.view().allocated_bytes(), 0);
				// Grow only idle operand width: the wire stays small while
				// aggregate register and basis storage exceeds rank zero's budget.
				let t = TransformBuilder::new()
					.encoding(t.encoding().clone())
					.operands(t.operands().clone().with_idle_high_qubits(5)?)
					.multiplication_odd(
						quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(
							quest_qsp::ControlSequence::builder()
								.angles(&[std::f64::consts::FRAC_PI_2], &[0.0])?
								.build()?,
						),
					)
					.build()?;
				expect_true!(
					env.qsvt()
						.transform(t)
						.overlap()
						.input(vec![Complex64::new(1., 0.)])
						.reference(vec![Complex64::new(1., 0.)])
						.prepare()
						.is_err()
				);
				expect_eq!(env.view().allocated_bytes(), 0);
			}
			expect_true!(comm.all_agree(true)?);
			Ok(())
		},
	)
}
#[gtest]
fn collective_qsvt_subgroups_run_independent_transforms() -> googletest::Result<()> {
	ranks(
		"collective_qsvt_subgroups_run_independent_transforms",
		"4",
		|| {
			let runtime = MpiRuntime::initialize()?;
			let mut world = runtime.world()?;
			let color = world.rank()? / 2;
			let mut subgroup = world.split(Some(color), world.rank()?)?.unwrap();
			{
				let env = CollectiveEnvironment::builder(&subgroup)?.build()?;
				let t = transform(Complex64::new(if color == 0 { 0.3 } else { 0.6 }, 0.2))?;
				let expected = t.materialize_block()?[(0, 0)];
				let mut p = env
					.qsvt()
					.transform(t)
					.overlap()
					.input(vec![Complex64::new(1., 0.)])
					.reference(vec![Complex64::new(1., 0.)])
					.prepare()?;
				for _ in 0..=color {
					let value = p.run()?.overlap();
					expect_that!(value.re, near(expected.re, 2e-12));
					expect_that!(value.im, near(expected.im, 2e-12));
				}
			}
			expect_true!(subgroup.all_agree(true)?);
			Ok(())
		},
	)
}
#[gtest]
fn collective_qsvt_checks_projector_representation_and_dense_complex_projection()
-> googletest::Result<()> {
	ranks(
		"collective_qsvt_checks_projector_representation_and_dense_complex_projection",
		"2",
		|| {
			use quest_qsvt::{EncodingBuilder, Left, LogicalSpace, Right};
			let runtime = MpiRuntime::initialize()?;
			let mut comm = runtime.world()?;
			{
				let env = CollectiveEnvironment::builder(&comm)?.build()?;
				let policy = NumericalPolicy::default();
				let seed = transform(Complex64::new(0.3, 0.4))?;
				let coordinate_basis =
					faer::Mat::from_fn(2, 1, |r, _| Complex64::new(f64::from(r == 0), 0.));
				let left = if env.rank()? == 0 {
					LogicalSpace::<Left>::coordinates(2, &[0], policy)?
				} else {
					LogicalSpace::<Left>::from_isometry(coordinate_basis.as_ref(), policy)?
				};
				let encoding = EncodingBuilder::new()
					.oracle(seed.encoding().oracle().clone())
					.left(left)
					.right(LogicalSpace::<Right>::coordinates(2, &[0], policy)?)
					.normalization(1.)?
					.build()?;
				let t = TransformBuilder::new()
					.encoding(encoding)
					.standard(
						quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![0.2, 0.2])
							.build()?,
					)
					.build()?;
				expect_true!(env.qsvt().transform(t).prepare().is_err());
				expect_eq!(env.view().allocated_bytes(), 0);
				let basis = faer::Mat::from_fn(2, 1, |r, _| {
					if r == 0 {
						Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.)
					} else {
						Complex64::new(0., std::f64::consts::FRAC_1_SQRT_2)
					}
				});
				let projector =
					faer::Mat::from_fn(2, 2, |r, c| basis[(r, 0)].mul(basis[(c, 0)].conj()));
				let encoding = EncodingBuilder::new()
					.oracle(seed.encoding().oracle().clone())
					.left(LogicalSpace::<Left>::from_dense_projector(
						projector.as_ref(),
						basis.as_ref(),
						policy,
					)?)
					.right(LogicalSpace::<Right>::from_isometry(
						basis.as_ref(),
						policy,
					)?)
					.normalization(1.)?
					.build()?;
				let t = TransformBuilder::new()
					.encoding(encoding)
					.standard(
						quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![0.2, 0.2])
							.build()?,
					)
					.build()?;
				let expected = t.materialize_block()?[(0, 0)];
				expect_true!(env.qsvt().transform(t.clone()).prepare().is_err());
				expect_eq!(env.view().allocated_bytes(), 0);
				let mut overlap = env
					.qsvt()
					.transform(t)
					.overlap()
					.input(vec![Complex64::new(1., 0.)])
					.reference(vec![Complex64::new(0., 1.)])
					.prepare()?;
				let observed = overlap.run()?;
				let target = Complex64::new(0., -1.).mul(expected);
				expect_that!(observed.overlap().re, near(target.re, 2e-12));
				expect_that!(observed.overlap().im, near(target.im, 2e-12));
			}
			expect_true!(comm.all_agree(true)?);
			Ok(())
		},
	)
}
