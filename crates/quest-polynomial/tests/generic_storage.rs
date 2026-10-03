#![allow(
	clippy::arithmetic_side_effects,
	clippy::float_cmp,
	reason = "Analytic fixtures compare exact expected coefficients and explicitly rounded rational references"
)]
use googletest::prelude::*;
use quest_numerics::arithmetic::PointBackend;
use quest_numerics::arithmetic::{Backend, ExactConstant, F64Backend, Interval64Backend};
use quest_polynomial::{DynamicShape, Limits, Monomial, Polynomial, StaticShape};

#[gtest]
fn coefficient_shape_is_checked_before_publication() -> googletest::Result<()> {
	let mut backend = F64Backend;
	expect_true!(
		Polynomial::from_scalars(
			Monomial,
			vec![1.0],
			StaticShape::<2>,
			&mut backend,
			Limits::default()
		)
		.is_err()
	);
	let polynomial = Polynomial::from_scalars(
		Monomial,
		vec![1.0, 2.0],
		StaticShape::<2>,
		&mut backend,
		Limits::default(),
	)?;
	expect_eq!(polynomial.coefficients(), &[1.0, 2.0]);
	expect_eq!(polynomial.evaluate_with(&mut backend, 3.0)?, 7.0);
	expect_true!(polynomial.with_shape(DynamicShape(3)).is_err());
	Ok(())
}

#[gtest]
fn scalar_validation_precedes_zero_and_support_shortcuts() -> googletest::Result<()> {
	let mut backend = F64Backend;
	expect_true!(
		Polynomial::from_scalars(
			Monomial,
			vec![f64::NAN],
			DynamicShape(1),
			&mut backend,
			Limits::default()
		)
		.is_err()
	);
	let zero = Polynomial::from_scalars(
		Monomial,
		vec![],
		StaticShape::<0>,
		&mut backend,
		Limits::default(),
	)?;
	expect_true!(zero.is_zero());
	expect_true!(zero.evaluate_with(&mut backend, f64::NAN).is_err());
	Ok(())
}

#[gtest]
fn signed_laurent_support_rejects_poles_and_preserves_removable_zeros() -> googletest::Result<()> {
	let mut backend = F64Backend;
	let removable = Polynomial::from_scalars(
		quest_polynomial::Laurent::new(-2),
		vec![0.0, 0.0, 2.0, 3.0],
		StaticShape::<4>,
		&mut backend,
		Limits::default(),
	)?;
	expect_eq!(removable.effective_support(), Some((0, 1)));
	expect_eq!(removable.evaluate_with(&mut backend, 0.0)?, 2.0);
	let mut enclosure = Interval64Backend;
	let bound = removable
		.evaluate_enclosure(&mut enclosure, quest_polynomial::Interval::new(-0.1, 0.1)?)?;
	expect_true!(bound.contains(2.0));
	let pole = Polynomial::from_scalars(
		quest_polynomial::Laurent::new(-1),
		vec![1.0],
		StaticShape::<1>,
		&mut backend,
		Limits::default(),
	)?;
	expect_true!(pole.evaluate_with(&mut backend, 0.0).is_err());
	Ok(())
}

#[gtest]
fn basis_recurrence_uses_backend_arithmetic_for_rational_coefficients() -> googletest::Result<()> {
	use quest_polynomial::Basis;
	let mut backend = Interval64Backend;
	let (scale, _, _) = quest_polynomial::Laguerre::new(0.0)?.recurrence_with(3, &mut backend)?;
	let numerator = backend.constant(&ExactConstant::Integer(-1))?;
	let denominator = backend.constant(&ExactConstant::Integer(3))?;
	let expected = backend.div(numerator, denominator)?;
	expect_eq!(scale.lower().to_bits(), expected.lower().to_bits());
	expect_eq!(scale.upper().to_bits(), expected.upper().to_bits());
	expect_true!(scale.lower() < scale.upper());
	Ok(())
}

#[gtest]
fn generic_jets_follow_the_same_point_and_enclosure_recurrence() -> googletest::Result<()> {
	let mut backend = F64Backend;
	let polynomial = Polynomial::from_scalars(
		Monomial,
		vec![1.0, 2.0, 3.0],
		StaticShape::<3>,
		&mut backend,
		Limits::default(),
	)?;
	let point = polynomial.jet_with(&mut backend, 2.0)?;
	expect_eq!(point.value, 17.0);
	expect_eq!(point.first, 14.0);
	expect_eq!(point.second, 6.0);
	let mut enclosure = Interval64Backend;
	let interval =
		polynomial.jet_enclosure(&mut enclosure, quest_polynomial::Interval::point(2.0)?)?;
	expect_true!(interval.value.contains(point.value));
	expect_true!(interval.first.contains(point.first));
	expect_true!(interval.second.contains(point.second));
	Ok(())
}

#[gtest]
fn multiprecision_recurrence_retains_rational_accuracy_and_noncopy_coefficients()
-> googletest::Result<()> {
	use quest_numerics::arithmetic::{MpBackend, Precision};
	use quest_polynomial::Basis;
	let mut backend = MpBackend::new(Precision {
		bits: 256,
		..Precision::default()
	})?;
	let (scale, _, _) = quest_polynomial::Laguerre::new(0.0)?.recurrence_with(3, &mut backend)?;
	let rounded = backend.point(-1.0 / 3.0)?;
	expect_eq!(backend.compare(&scale, &rounded)?, std::cmp::Ordering::Less);
	let coefficient = backend.constant(&ExactConstant::Rational(1, 10))?;
	let polynomial = Polynomial::from_scalars(
		Monomial,
		vec![coefficient.clone()],
		StaticShape::<1>,
		&mut backend,
		Limits::default(),
	)?;
	let zero = backend.point(0.0)?;
	let evaluated = polynomial.evaluate_with(&mut backend, zero)?;
	expect_eq!(
		backend.compare(&evaluated, &coefficient)?,
		std::cmp::Ordering::Equal
	);
	let mut enclosure = quest_numerics::arithmetic::MpIntervalBackend::new(Precision {
		bits: 256,
		..Precision::default()
	})?;
	let x = enclosure.point(0.0)?;
	let jet = polynomial.jet_enclosure(&mut enclosure, x)?;
	expect_true!(jet.value.lower() <= &coefficient);
	expect_true!(jet.value.upper() >= &coefficient);
	expect_true!(
		jet.first.lower().repr().significand().is_zero()
			&& jet.first.upper().repr().significand().is_zero()
	);
	expect_true!(
		jet.second.lower().repr().significand().is_zero()
			&& jet.second.upper().repr().significand().is_zero()
	);
	Ok(())
}

#[gtest]
fn admission_accounts_for_retained_high_precision_mantissas_and_vec_capacity()
-> googletest::Result<()> {
	use quest_numerics::arithmetic::{MpBackend, Precision};
	let mut source = MpBackend::new(Precision {
		bits: 4096,
		..Precision::default()
	})?;
	let coefficient = source.constant(&ExactConstant::Rational(1, 10))?;
	let mut backend = MpBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})?;
	let limits = Limits {
		max_bytes: 256,
		..Limits::default()
	};
	expect_true!(
		Polynomial::from_scalars(
			Monomial,
			vec![coefficient],
			StaticShape::<1>,
			&mut backend,
			limits
		)
		.is_err()
	);
	let mut large_capacity = Vec::with_capacity(100);
	large_capacity.push(1.0);
	let mut backend = F64Backend;
	expect_true!(
		Polynomial::from_scalars(
			Monomial,
			large_capacity,
			DynamicShape(1),
			&mut backend,
			limits
		)
		.is_err()
	);
	Ok(())
}

#[gtest]
fn constant_and_zero_padding_avoid_irrelevant_basis_domain_arithmetic() -> googletest::Result<()> {
	let mut backend = F64Backend;
	let basis = quest_polynomial::Jacobi::new(f64::MAX, f64::MAX)?;
	let polynomial = Polynomial::from_scalars(
		basis,
		vec![2.0, 0.0],
		StaticShape::<2>,
		&mut backend,
		Limits::default(),
	)?;
	expect_eq!(polynomial.evaluate_with(&mut backend, 0.3)?, 2.0);
	let mut enclosure = Interval64Backend;
	let jet = polynomial.jet_enclosure(&mut enclosure, quest_polynomial::Interval::point(0.3)?)?;
	expect_true!(jet.value.contains(2.0));
	expect_eq!(jet.first.lower(), 0.0);
	expect_eq!(jet.second.upper(), 0.0);
	Ok(())
}

#[gtest]
fn generic_admission_and_signed_shift_obey_work_and_support_limits() -> googletest::Result<()> {
	let mut backend = F64Backend;
	let limits = Limits {
		max_work: 3,
		..Limits::default()
	};
	let polynomial = Polynomial::from_scalars(
		quest_polynomial::Laurent::new(1024),
		vec![1.0],
		StaticShape::<1>,
		&mut backend,
		limits,
	)?;
	expect_true!(matches!(
		polynomial.evaluate_with(&mut backend, 1.0),
		Err(quest_polynomial::Error::Budget(_))
	));
	expect_true!(
		Polynomial::from_scalars(
			Monomial,
			vec![1.0, 2.0],
			StaticShape::<2>,
			&mut backend,
			limits,
		)
		.is_err()
	);
	expect_true!(
		Polynomial::from_scalars(
			quest_polynomial::Laurent::new(i32::MAX),
			vec![0.0, 0.0],
			StaticShape::<2>,
			&mut backend,
			Limits::default(),
		)
		.is_err()
	);
	Ok(())
}

#[gtest]
fn complex_static_shape_keeps_conversion_and_norm_source_evidence() -> googletest::Result<()> {
	use quest_polynomial::{Chebyshev, Complex64, NormDomain, NormOptions, NormOutcome};
	let polynomial = Polynomial::new(Chebyshev, vec![Complex64::new(2.0, 0.0)], Limits::default())?
		.with_shape(StaticShape::<1>)?;
	let conversion = polynomial.to_monomial()?;
	let _: &Polynomial<Chebyshev, Complex64, StaticShape<1>> = conversion.source();
	expect_eq!(conversion.polynomial().evaluate_real(0.3)?, 2.0);
	let norm = polynomial.certify_norm(
		NormDomain::RealInterval(quest_polynomial::Interval::new(-1.0, 1.0)?),
		NormOptions::default(),
	)?;
	let NormOutcome::Bounded(evidence) = norm else {
		return fail!("constant norm was not established");
	};
	let _: &Polynomial<Chebyshev, Complex64, StaticShape<1>> = evidence.source();
	Ok(())
}

#[gtest]
fn finite_linear_jacobi_does_not_evaluate_unused_higher_recurrences() -> googletest::Result<()> {
	let mut backend = F64Backend;
	let polynomial = Polynomial::from_scalars(
		quest_polynomial::Jacobi::new(1e200, 1e200)?,
		vec![0.0, 1.0],
		StaticShape::<2>,
		&mut backend,
		Limits::default(),
	)?;
	expect_eq!(polynomial.evaluate_with(&mut backend, 0.0)?, 0.0);
	let jet = polynomial.jet_with(&mut backend, 0.0)?;
	expect_eq!(jet.first, 1e200);
	expect_eq!(jet.second, 0.0);
	Ok(())
}

#[gtest]
fn retained_storage_counts_capacity_and_shared_allocation_headers() -> googletest::Result<()> {
	let mut capacity = Vec::with_capacity(100);
	capacity.push(1.0);
	let stored = Polynomial::from_scalars(
		Monomial,
		capacity,
		DynamicShape(1),
		&mut F64Backend,
		Limits::default(),
	)?;
	expect_eq!(
		stored.retained_heap_bytes(&F64Backend)?,
		100 * size_of::<f64>() + size_of::<Vec<f64>>() + 2 * size_of::<usize>()
	);
	Ok(())
}
