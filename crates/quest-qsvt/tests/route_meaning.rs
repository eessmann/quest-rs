use googletest::prelude::*;
use quest_polynomial::{Chebyshev, Complex64, Limits, Monomial, Polynomial};
use quest_qsp::{SynthesisAlgorithm, SynthesisBuilder};
use quest_qsvt::{GramArgument, HermitianArgument, RouteTarget};

#[gtest]
fn basis_conversion_and_gram_reduction_keep_distinct_mathematical_targets() -> Result<()> {
	let square = Polynomial::new(
		Monomial,
		vec![
			Complex64::new(0.0, 0.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(0.4, 0.0),
		],
		Limits::default(),
	)?;
	let hermitian = RouteTarget::<HermitianArgument>::from_monomial(square.clone())?;
	let gram =
		RouteTarget::<GramArgument>::reduce_even(square.admit_parity::<quest_polynomial::Even>()?)?;
	let t2 = RouteTarget::<HermitianArgument>::from_chebyshev(Polynomial::new(
		Chebyshev,
		vec![
			Complex64::new(0.0, 0.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(0.4, 0.0),
		],
		Limits::default(),
	)?);
	expect_eq!(
		hermitian.polynomial().coefficients(),
		&[
			Complex64::new(0.2, 0.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(0.2, 0.0)
		]
	);
	expect_eq!(
		gram.polynomial().coefficients(),
		&[Complex64::new(0.0, 0.0), Complex64::new(0.4, 0.0)]
	);
	expect_ne!(
		hermitian.unit_circle_target()?.coefficients(),
		t2.unit_circle_target()?.coefficients()
	);
	expect_eq!(hermitian.conversion().or_fail()?.source().stored_order(), 2);
	expect_eq!(gram.unreduced_source().or_fail()?.stored_order(), 2);
	Ok(())
}

#[gtest]
fn binding_rejects_a_different_frozen_target_for_both_solvers() -> Result<()> {
	let one = RouteTarget::<HermitianArgument>::from_chebyshev(Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.1, 0.1), Complex64::new(0.2, -0.05)],
		Limits::default(),
	)?);
	let other = RouteTarget::<HermitianArgument>::from_chebyshev(Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.1, 0.1), Complex64::new(0.2, 0.05)],
		Limits::default(),
	)?);
	for algorithm in [
		SynthesisAlgorithm::RhwHalfCholesky,
		SynthesisAlgorithm::InverseNlftDivideConquer,
	] {
		let candidate = SynthesisBuilder::new()
			.policy(quest_qsp::Policy {
				algorithm,
				..quest_qsp::Policy::default()
			})
			.unit_circle_response(&one.unit_circle_target()?)?
			.admit()?
			.complete()?
			.synthesize()?;
		expect_true!(other.clone().bind(candidate.clone()).is_err());
		let response = one.clone().bind(candidate)?;
		expect_eq!(
			response.meaning().target().or_fail()?.coefficients(),
			one.polynomial().coefficients()
		);
	}
	Ok(())
}

#[gtest]
fn route_binding_preserves_empty_source_span_instead_of_confusing_solver_padding() -> Result<()> {
	let empty = RouteTarget::<HermitianArgument>::from_chebyshev(Polynomial::new(
		Chebyshev,
		vec![],
		Limits::default(),
	)?);
	let stored_zero = RouteTarget::<HermitianArgument>::from_chebyshev(Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.0, 0.0)],
		Limits::default(),
	)?);
	for algorithm in [
		SynthesisAlgorithm::RhwHalfCholesky,
		SynthesisAlgorithm::InverseNlftDivideConquer,
	] {
		let candidate = SynthesisBuilder::new()
			.policy(quest_qsp::Policy {
				algorithm,
				..quest_qsp::Policy::default()
			})
			.unit_circle_response(&empty.unit_circle_target()?)?
			.admit()?
			.complete()?
			.synthesize()?;
		expect_true!(stored_zero.clone().bind(candidate.clone()).is_err());
		let bound = empty.clone().bind(candidate)?;
		expect_eq!(bound.meaning().target().or_fail()?.stored_support(), None);
	}
	Ok(())
}

#[gtest]
fn explicit_unit_circle_transfer_preserves_offset_and_checks_padding_budget() -> Result<()> {
	let source = Polynomial::new(
		quest_polynomial::Laurent::new(2),
		vec![Complex64::new(0.2, 0.1)],
		Limits::default(),
	)?;
	let route = RouteTarget::<HermitianArgument>::from_unit_circle_coefficients(source.clone())?;
	expect_eq!(
		route.polynomial().coefficients(),
		&[
			Complex64::new(0.0, 0.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(0.2, 0.1)
		]
	);
	expect_eq!(route.unit_circle_target()?.stored_support(), Some((2, 2)));
	for algorithm in [
		SynthesisAlgorithm::RhwHalfCholesky,
		SynthesisAlgorithm::InverseNlftDivideConquer,
	] {
		let candidate = SynthesisBuilder::new()
			.policy(quest_qsp::Policy {
				algorithm,
				..quest_qsp::Policy::default()
			})
			.unit_circle_response(&source)?
			.admit()?
			.complete()?
			.synthesize()?;
		let padded = RouteTarget::<HermitianArgument>::from_chebyshev(route.polynomial().clone());
		expect_true!(padded.bind(candidate.clone()).is_err());
		let bound = route.clone().bind(candidate)?;
		expect_eq!(
			bound
				.meaning()
				.unit_circle_source()
				.or_fail()?
				.stored_support(),
			Some((2, 2))
		);
	}
	let source = Polynomial::new(
		quest_polynomial::Laurent::new(1_024),
		vec![Complex64::new(0.2, 0.1)],
		Limits {
			max_coefficients: 4,
			..Limits::default()
		},
	)?;
	expect_true!(RouteTarget::<GramArgument>::from_unit_circle_coefficients(source).is_err());
	Ok(())
}
