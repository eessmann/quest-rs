use googletest::prelude::*;
use quest_numerics::arithmetic::{ExactConstant, F64Backend, Interval64Backend};
use quest_polynomial::{
	Accuracy, Chebyshev, Complex64, ExactDomain, GenericFunction, Hermite, Interval, Jacobi,
	Laguerre, Limits, Monomial, Polynomial, RemezRequest, function,
};
const fn c(x: f64) -> Complex64 {
	Complex64::new(x, 0.0)
}
#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Expression operators build syntax; numerical evaluation is checked"
)]
fn one_expression_evaluates_values_and_encloses_derivatives() -> Result<()> {
	let f = function!(|x| (x * x + 1.0).ln());
	expect_that!(
		f.evaluate(&mut F64Backend, 0.5)?,
		near(1.25_f64.ln(), 1e-14)
	);
	let j = f.jet(&mut Interval64Backend, Interval::point(0.5)?)?;
	expect_true!(j.value.contains(1.25_f64.ln()));
	expect_true!(j.first.contains(0.8));
	expect_true!(j.second.contains(0.96));
	expect_true!(
		function!(|x| x.ln())
			.evaluate(&mut Interval64Backend, Interval::new(-1.0, 1.0)?)
			.is_err()
	);
	Ok(())
}
#[gtest]
fn derivatives_keep_basis_families_and_intervals_enclose_original_parameters() -> Result<()> {
	let l = Limits::default();
	let a = vec![c(0.0), c(0.0), c(1.0)];
	let p = Polynomial::new(Chebyshev, a.clone(), l)?;
	expect_that!(p.derivative()?.evaluate_real(0.3)?, near(1.2, 1e-14));
	let p = Polynomial::new(Hermite::physicists(), a.clone(), l)?;
	expect_that!(p.derivative()?.evaluate_real(0.3)?, near(2.4, 1e-14));
	let p = Polynomial::new(Laguerre::new(0.3)?, a.clone(), l)?;
	expect_that!(p.derivative()?.evaluate_real(0.3)?, near(-2.0, 1e-14));
	expect_true!(
		p.evaluate_interval(Interval::new(0.2, 0.4)?)?
			.contains(p.evaluate_real(0.3)?)
	);
	let p = Polynomial::new(Jacobi::new(0.3, 0.4)?, a, l)?;
	expect_true!(
		p.evaluate_interval(Interval::point(0.3)?)?
			.contains(p.evaluate_real(0.3)?)
	);
	let p = Polynomial::new(Jacobi::new(f64::MAX, f64::MAX)?, vec![c(2.0)], l)?;
	expect_that!(p.evaluate_real(0.3)?, eq(2.0));
	Ok(())
}
#[gtest]
fn conversions_return_enclosed_error_and_parity_is_admitted() -> Result<()> {
	let p = Polynomial::new(Chebyshev, vec![c(1.0), c(0.0), c(2.0)], Limits::default())?;
	let converted = p.to_monomial()?;
	expect_that!(
		converted.polynomial().evaluate_real(0.3)?,
		near(-0.64, 1e-14)
	);
	expect_true!(converted.coefficient_error_bound() >= 0.0);
	expect_true!(p.clone().admit_parity::<quest_polynomial::Even>().is_ok());
	expect_true!(p.admit_parity::<quest_polynomial::Odd>().is_err());
	let p = Polynomial::new(Monomial, vec![c(0.0), c(0.0), c(1.0)], Limits::default())?;
	expect_that!(p.derivative()?.evaluate_real(0.5)?, eq(1.0));
	Ok(())
}
#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Expression operators build syntax; numerical evaluation is checked"
)]
fn remez_linear_square_matches_known_minimax_and_certifies_domain() -> Result<()> {
	let f = function!(|x| x * x);
	let result = RemezRequest::binary64(f, ExactDomain::binary64(-1.0, 1.0), 1).run()?;
	expect_that!(
		result.polynomial().evaluate_with(&mut F64Backend, 0.3)?,
		near(0.5, 1e-8)
	);
	expect_true!(result.uniform_error().bound().upper() >= 0.5);
	expect_that!(result.uniform_error().bound().upper(), lt(0.50001));
	Ok(())
}
#[gtest]
fn basis_derivatives_preserve_parameters_and_match_exact_legendre_identity() -> Result<()> {
	let limits = Limits::default();
	let laguerre = Polynomial::new(Laguerre::new(0.3)?, vec![c(0.0), c(0.0), c(1.0)], limits)?;
	expect_that!(laguerre.derivative()?.basis().alpha(), eq(0.3));
	let p = Polynomial::new(
		Jacobi::new(0.0, 0.0)?,
		vec![c(0.0), c(0.0), c(0.0), c(1.0)],
		limits,
	)?;
	expect_that!(p.derivative()?.basis().parameters(), eq((0.0, 0.0)));
	expect_that!(p.derivative()?.evaluate_real(0.3)?, near(-0.825, 1e-14));
	Ok(())
}
#[gtest]
fn remez_exp_converges_with_domain_enclosure_and_error_lower_bound() -> Result<()> {
	let f = function!(|x| x.exp());
	let result = RemezRequest::binary64(f, ExactDomain::binary64(-1.0, 1.0), 3)
		.accuracy(Accuracy::MinimaxGap(ExactConstant::Binary64(1e-8)))
		.run()?;
	expect_true!(
		result
			.attempts()
			.iter()
			.any(|attempt| attempt.iterations > 1)
	);
	expect_that!(
		result.uniform_error().bound().upper(),
		near(0.005_528_37, 1e-6)
	);
	expect_that!(result.minimax_gap().gap().upper(), le(1e-8));
	for x in [-1.0, -0.9, -0.6, 0.0, 0.4, 0.9, 1.0] {
		expect_that!(
			std::ops::Sub::sub(
				result.target().evaluate(&mut F64Backend, x)?,
				result.polynomial().evaluate_with(&mut F64Backend, x)?
			)
			.abs(),
			le(result.uniform_error().bound().upper())
		);
	}
	Ok(())
}
#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Expression operators build a checked mathematical AST"
)]
fn root_isolation_keeps_tangent_candidates_nonunique_and_reports_budget_exhaustion() -> Result<()> {
	use quest_numerics::{
		ad::First,
		roots::{CoverLimits, cover},
	};
	let f = function!(|x| x.sin());
	let roots = cover(
		&mut Interval64Backend,
		Interval::new(0.0, 3.0)?,
		|b, x| {
			let j = f.jet(b, *x)?;
			Ok(First {
				value: j.first,
				first: j.second,
			})
		},
		CoverLimits::default(),
		&1e-9,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)?;
	expect_true!(
		roots
			.covered
			.iter()
			.any(|r| r.interval.contains(std::f64::consts::FRAC_PI_2) && r.evidence.unique())
	);
	let tangent = function!(|x| x * x * x);
	let tangent_roots = cover(
		&mut Interval64Backend,
		Interval::new(-1.0, 1.0)?,
		|b, x| {
			let j = tangent.jet(b, *x)?;
			Ok(First {
				value: j.first,
				first: j.second,
			})
		},
		CoverLimits::default(),
		&1e-9,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)?;
	expect_true!(
		tangent_roots.unresolved.iter().any(|r| r.contains(0.0))
			|| tangent_roots
				.covered
				.iter()
				.any(|r| r.interval.contains(0.0) && !r.evidence.unique())
	);
	let exhausted = cover(
		&mut Interval64Backend,
		Interval::new(0.0, 3.0)?,
		|b, x| {
			let j = f.jet(b, *x)?;
			Ok(First {
				value: j.first,
				first: j.second,
			})
		},
		CoverLimits {
			max_iterations: 1,
			..CoverLimits::default()
		},
		&1e-9,
		quest_numerics::roots::Premise::EnclosesContinuouslyDifferentiableFunction,
	)?;
	expect_false!(exhausted.complete());
	expect_true!(
		RemezRequest::binary64(function!(|x| x.ln()), ExactDomain::binary64(-1.0, 1.0), 3)
			.run()
			.is_err()
	);
	Ok(())
}
#[gtest]
fn explicit_conversion_preserves_complex_response_across_parameterized_bases() -> Result<()> {
	let limits = Limits::default();
	let p = Polynomial::new(
		Hermite::physicists(),
		vec![Complex64::new(0.2, 0.1), Complex64::new(-0.3, 0.2), c(0.4)],
		limits,
	)?;
	let cheb = p.to_basis(Chebyshev)?;
	let lag = p.to_basis(Laguerre::new(0.3)?)?;
	let jac = p.to_basis(Jacobi::new(0.3, 0.7)?)?;
	let x = Complex64::new(0.3, 0.2);
	let expected = p.evaluate(x)?;
	for actual in [
		cheb.polynomial().evaluate(x)?,
		lag.polynomial().evaluate(x)?,
		jac.polynomial().evaluate(x)?,
	] {
		expect_that!(std::ops::Sub::sub(actual, expected).norm(), lt(1e-13));
	}
	expect_true!(cheb.coefficient_error_bound() >= 0.0);
	expect_true!(lag.coefficient_error_bound() >= 0.0);
	expect_true!(jac.coefficient_error_bound() >= 0.0);
	Ok(())
}
#[gtest]
fn extensions_require_an_explicit_consistency_assumption() -> Result<()> {
	use quest_polynomial::{AssumedFunction, ConsistencyAssumption};
	struct Identity;
	impl GenericFunction for Identity {
		fn evaluate<B: quest_numerics::arithmetic::Backend>(
			&self,
			_: &mut B,
			x: B::Scalar,
		) -> std::result::Result<B::Scalar, B::Error> {
			Ok(x)
		}
	}
	let premise = ConsistencyAssumption::SameFunctionAndValidEnclosures;
	let f = AssumedFunction::new(Identity, premise);
	expect_that!(f.assumption(), eq(premise));
	expect_that!(f.evaluate(&mut F64Backend, 0.3)?, eq(0.3));
	expect_true!(
		f.jet(&mut Interval64Backend, Interval::new(0.2, 0.4)?)?
			.value
			.contains(0.3)
	);
	Ok(())
}
#[gtest]
fn exact_cosine_conversion_rejects_underflow_that_changes_the_polynomial() -> Result<()> {
	let p = Polynomial::new(
		Chebyshev,
		vec![c(0.0), c(f64::from_bits(1))],
		Limits::default(),
	)?;
	expect_true!(p.on_cosine_circle().is_err());
	Ok(())
}
#[gtest]
fn expression_metadata_preserves_the_original_function_structure() -> Result<()> {
	let function = function!(|x| x.exp());
	expect_that!(function.metadata().operations, eq(1));
	expect_that!(function.metadata().inputs, eq(1));
	expect_that!(function.evaluate(&mut F64Backend, 0.5)?, eq(0.5_f64.exp()));
	Ok(())
}

#[gtest]
fn conversion_evidence_retains_both_immutable_polynomials() -> Result<()> {
	let p = Polynomial::new(Chebyshev, vec![c(0.0), c(0.0), c(1.0)], Limits::default())?;
	let conversion = p.to_monomial()?;
	expect_that!(conversion.source().coefficients(), eq(p.coefficients()));
	expect_that!(conversion.source().evaluate_real(0.0)?, eq(-1.0));
	expect_that!(
		conversion.polynomial().coefficients(),
		eq([c(-1.0), c(0.0), c(2.0)].as_slice())
	);
	expect_true!(conversion.coefficient_error_bound().is_finite());
	expect_that!(conversion.into_polynomial().evaluate_real(0.0)?, eq(-1.0));
	Ok(())
}

#[gtest]
fn basis_conversion_preflights_aggregate_storage_and_work() -> Result<()> {
	for limits in [
		Limits {
			shapes: (Limits::default()).shapes,
			resources: quest_numerics::ResourceLimits {
				max_peak_bytes: 6_400,
				..(Limits::default()).resources
			},
		},
		Limits {
			shapes: (Limits::default()).shapes,
			resources: quest_numerics::ResourceLimits {
				max_work_units: 1_800,
				..(Limits::default()).resources
			},
		},
	] {
		let polynomial = Polynomial::new(Monomial, vec![c(0.001); 20], limits)?;
		expect_true!(matches!(
			polynomial.to_basis(Chebyshev),
			Err(quest_polynomial::Error::Interval(
				quest_numerics::Error::Resource(_)
			))
		));
	}
	let polynomial = Polynomial::new(Monomial, vec![c(0.001); 20], Limits::default())?;
	expect_true!(polynomial.to_basis(Chebyshev).is_ok());
	Ok(())
}
