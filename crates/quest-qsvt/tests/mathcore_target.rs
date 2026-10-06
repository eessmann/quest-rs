//! Formal exact source and its explicit numerical target remain distinguishable.
use googletest::prelude::*;
use mathcore::{
	arithmetic::ExactConstant,
	dynamic::ExpressionLimits,
	exact::{Owner, Symbol},
	multivariate::PolynomialLimits,
	typed,
};
use quest_numerics::arithmetic::F64Backend;
use quest_polynomial::{ExactMonomialTarget, Function, Limits};
use quest_qsp::SynthesisBuilder;
use quest_qsvt::{HermitianArgument, RouteTarget};
use std::ops::Mul;

#[gtest]
fn exact_dynamic_target_reaches_qsvt_route_without_losing_source() -> Result<()> {
	let symbol = Symbol::new(Owner::new(73), 0);
	let x = typed::variable::<0>();
	let source = Function::new(x.mul(x).mul(typed::exact(ExactConstant::Rational(1, 3))))
		.dynamic(&[symbol], ExpressionLimits::default())?;
	let target = ExactMonomialTarget::from_expression(
		source,
		symbol,
		PolynomialLimits::default(),
		Limits::default(),
	)?;
	let route = RouteTarget::<HermitianArgument>::from_monomial(target.polynomial().clone())?;
	let qsp_target = route.unit_circle_target()?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&qsp_target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let response = route.bind(candidate)?;
	verify_eq!(
		response
			.meaning()
			.target()
			.ok_or_else(|| std::io::Error::other("missing route target"))?
			.coefficients()
			.len(),
		3
	)?;
	let derivative = target
		.exact()
		.differentiate(symbol)?
		.lower(&mut F64Backend)?;
	verify_eq!(derivative.evaluate(&mut F64Backend, &[0.5])?, 1.0 / 3.0)?;
	verify_that!(target.rounding_bound(), gt(0.0))?;
	Ok(())
}
