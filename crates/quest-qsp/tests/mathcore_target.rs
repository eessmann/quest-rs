//! Shared typed/dynamic algebra becomes an explicit frozen QSP coefficient target.
use googletest::prelude::*;
use mathcore::{
	arithmetic::ExactConstant,
	dynamic::ExpressionLimits,
	exact::{Owner, Symbol},
	multivariate::PolynomialLimits,
	typed,
};
use quest_numerics::arithmetic::F64Backend;
use quest_polynomial::{ExactMonomialTarget, Function, Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, SynthesisBuilder};
use std::ops::{Mul, Sub};

#[gtest]
fn exact_dynamic_source_lowers_once_then_synthesizes_qsp() -> Result<()> {
	let symbol = Symbol::new(Owner::new(72), 0);
	let source =
		Function::new(typed::variable::<0>().mul(typed::exact(ExactConstant::Rational(1, 3))))
			.dynamic(&[symbol], ExpressionLimits::default())?;
	let target = ExactMonomialTarget::from_expression(
		source,
		symbol,
		PolynomialLimits::default(),
		Limits::default(),
	)?;
	let kernel = target.source().lower(&mut F64Backend, &[symbol])?;
	for x in [-1.0, 0.0, 1.0] {
		verify_eq!(
			kernel.evaluate(&mut F64Backend, &[x])?,
			target.polynomial().evaluate_real(x)?
		)?;
	}
	let qsp = Polynomial::new(
		Laurent::new(0),
		target.polynomial().coefficients().to_vec(),
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&qsp)?
		.admit()?
		.complete()?
		.synthesize()?;
	for signal in [
		Complex64::new(1.0, 0.0),
		Complex64::new(0.0, 1.0),
		Complex64::new(-1.0, 0.0),
	] {
		let [[actual, _], [_, _]] = candidate.evaluate(signal)?;
		verify_that!(actual.sub(qsp.evaluate(signal)?).norm(), le(1e-9))?;
	}
	verify_that!(target.rounding_bound(), gt(0.0))?;
	Ok(())
}
