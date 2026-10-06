#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Small independently computed matrices"
)]
mod portfolio_support;
use quest_qsvt::{
	Complex64, NumericalPolicy, ReplayEncoding, ShiftRegister, TensorShiftEncoding,
	portfolio::{KroneckerSum, PortfolioLimits, TensorProduct, WeightedLcu},
};
fn shift(n: usize, k: usize) -> quest_qsvt::Result<TensorShiftEncoding> {
	TensorShiftEncoding::new(
		n,
		vec![ShiftRegister::new(0, n, k)?],
		NumericalPolicy::default(),
	)
}
fn matrix<E: ReplayEncoding>(e: &E) -> quest_qsvt::Result<faer::Mat<Complex64>> {
	quest_qsvt::materialize_oracle(
		&e.replay_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)
}
#[googletest::gtest]
fn paired_complex_coefficient_signs_change_lcu_operator_identity() -> googletest::Result<()> {
	let source = |sign| {
		WeightedLcu::new(
			vec![
				(Complex64::new(sign, 0.), shift(1, 0)?),
				(Complex64::new(0., sign), shift(1, 1)?),
			],
			PortfolioLimits::default(),
		)
	};
	let positive = source(1.)?;
	let negative = source(-1.)?;
	googletest::expect_ne!(
		positive.descriptor()?.source_identity,
		negative.descriptor()?.source_identity
	);
	googletest::expect_ne!(
		positive.descriptor()?.construction_identity,
		negative.descriptor()?.construction_identity
	);
	let policy = NumericalPolicy::default();
	let transform = quest_qsvt::replay_transform::ReplayTransform::new(
		positive,
		quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![0., 0.]).build()?,
		policy,
	)?;
	let schedule = transform.schedule(policy)?;
	googletest::expect_true!(
		quest_qsvt::replay_transform::ReplayTransform::from_schedule(negative, schedule, policy)
			.is_err()
	);
	Ok(())
}
#[googletest::gtest]
fn weighted_complex_lcu_extracts_sum_and_preserves_every_failure_branch() -> googletest::Result<()>
{
	let e = WeightedLcu::new(
		vec![
			(Complex64::new(0.0, 2.0), shift(1, 1)?),
			(Complex64::new(-0.25, 0.0), shift(1, 0)?),
			(Complex64::new(0.1, -0.2), shift(1, 1)?),
		],
		PortfolioLimits::default(),
	)?;
	portfolio_support::whole_register(&e)?;
	let d = e.descriptor()?;
	let u = matrix(&e)?;
	let left = d
		.left
		.logical_space::<quest_qsvt::Left>(d.layout.num_qubits, NumericalPolicy::default())?;
	for i in 0..2 {
		for j in 0..2 {
			let expected = if i == j {
				Complex64::new(-0.25, 0.0)
			} else {
				Complex64::new(0.1, 1.8)
			};
			googletest::expect_true!(
				(u[(
					left.coordinate_at(i).unwrap(),
					left.coordinate_at(j).unwrap()
				)] * d.normalization
					- expected)
					.norm()
					< 3e-13
			);
		}
	}
	let mut v = vec![];
	e.visit_replay(false, &mut |g| {
		v.push(g);
		Ok(())
	})?;
	let mut a = vec![];
	e.visit_replay(true, &mut |g| {
		a.push(g);
		Ok(())
	})?;
	googletest::expect_eq!(v.len(), a.len());
	for (f, r) in v.iter().rev().zip(a) {
		let expected = match f.kind {
			quest_qsvt::ReplayKind::Ry(x) => quest_qsvt::ReplayKind::Ry(-x),
			quest_qsvt::ReplayKind::Phase(x) => quest_qsvt::ReplayKind::Phase(-x),
			k => k,
		};
		googletest::expect_true!(
			r.kind == expected
				&& r.target == f.target
				&& r.control_mask == f.control_mask
				&& r.control_value == f.control_value
		);
	}
	for i in 0..u.nrows() {
		for j in 0..u.nrows() {
			let dot: Complex64 = (0..u.nrows()).map(|k| u[(k, i)].conj() * u[(k, j)]).sum();
			googletest::expect_true!((dot - Complex64::new(f64::from(i == j), 0.0)).norm() < 4e-13);
		}
	}
	googletest::expect_true!(e.resources().preparation_gates > 0);
	Ok(())
}
#[googletest::gtest]
fn tensor_and_kronecker_sum_use_independent_clean_workspaces() -> googletest::Result<()> {
	let a = WeightedLcu::new(
		vec![
			(Complex64::new(0.0, 2.0), shift(1, 1)?),
			(Complex64::new(1.0, 0.0), shift(1, 0)?),
		],
		PortfolioLimits::default(),
	)?;
	let b = WeightedLcu::new(
		vec![(Complex64::new(-0.5, 0.0), shift(1, 1)?)],
		PortfolioLimits::default(),
	)?;
	let product = TensorProduct::new(a.clone(), b.clone(), PortfolioLimits::default())?;
	let sum = KroneckerSum::new(a, b, PortfolioLimits::default())?;
	portfolio_support::whole_register(&product)?;
	portfolio_support::whole_register(&sum)?;
	let check = |d: quest_qsvt::EncodingDescriptor,
	             u: faer::Mat<Complex64>,
	             sum: bool|
	 -> quest_qsvt::Result<()> {
		let s = d
			.left
			.logical_space::<quest_qsvt::Left>(d.layout.num_qubits, NumericalPolicy::default())?;
		for i in 0..4 {
			for j in 0..4 {
				let av = if i % 2 == j % 2 {
					Complex64::new(1.0, 0.0)
				} else {
					Complex64::new(0.0, 2.0)
				};
				let bv = if i / 2 == j / 2 {
					Complex64::default()
				} else {
					Complex64::new(-0.5, 0.0)
				};
				let expected = if sum {
					av * f64::from(i / 2 == j / 2) + bv * f64::from(i % 2 == j % 2)
				} else {
					av * bv
				};
				assert!(
					(u[(s.coordinate_at(i).unwrap(), s.coordinate_at(j).unwrap())]
						* d.normalization
						- expected)
						.norm()
						< 3e-13
				);
			}
		}
		Ok(())
	};
	check(product.descriptor()?, matrix(&product)?, false)?;
	check(sum.descriptor()?, matrix(&sum)?, true)?;
	Ok(())
}
#[googletest::gtest]
fn composition_identifies_term_order_and_rejects_incompatible_or_unadmitted_sources()
-> googletest::Result<()> {
	let terms = vec![
		(Complex64::new(0.0, 2.0), shift(1, 1)?),
		(Complex64::new(-0.25, 0.0), shift(1, 0)?),
	];
	let first = WeightedLcu::new(terms.clone(), PortfolioLimits::default())?;
	let reordered = WeightedLcu::new(
		terms.iter().rev().cloned().collect(),
		PortfolioLimits::default(),
	)?;
	googletest::expect_eq!(
		first.descriptor()?.source_identity,
		reordered.descriptor()?.source_identity
	);
	googletest::expect_true!(
		first.descriptor()?.construction_identity != reordered.descriptor()?.construction_identity
	);
	for limits in [
		PortfolioLimits {
			max_bytes: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_gates: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_compile_work: 1,
			..Default::default()
		},
		PortfolioLimits {
			max_table_entries: 1,
			..Default::default()
		},
	] {
		googletest::expect_true!(WeightedLcu::new(terms.clone(), limits).is_err());
	}
	googletest::expect_true!(
		WeightedLcu::new(
			vec![(1.0.into(), shift(1, 1)?), (1.0.into(), shift(2, 1)?)],
			PortfolioLimits::default()
		)
		.is_err()
	);
	let mut oversized = Vec::with_capacity(131_072);
	oversized.push((1.0.into(), shift(1, 1)?));
	googletest::expect_true!(
		WeightedLcu::new(
			oversized,
			PortfolioLimits {
				max_bytes: 16_384,
				..Default::default()
			}
		)
		.is_err()
	);
	Ok(())
}
