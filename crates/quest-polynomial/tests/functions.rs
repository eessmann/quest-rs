#![feature(const_trait_impl, const_ops, generic_const_exprs)]
#![allow(incomplete_features)]
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Expression construction and independent analytic derivative references"
)]
use googletest::prelude::*;
use quest_numerics::arithmetic::{Backend, Budget, BudgetedBackend, F64Backend, Interval64Backend};
use quest_polynomial::{
	AssumedFunction, ConsistencyAssumption, ExactDomain, Expression, Function, GenericFunction,
	Interval, Limits, RemezOptions, RemezRequest, Shape, StaticShape, function,
};

#[gtest]
fn const_expression_and_independent_derivatives() -> Result<()> {
	const F: Function<quest_polynomial::typed::Typed<quest_polynomial::typed::VariableNode<0>>> =
		function!(|x| x);
	expect_that!(F.evaluate(&mut F64Backend, 0.25)?, eq(0.25));
	let f = function!(|x| (1.0 + x * x).ln());
	let j = f.jet(&mut F64Backend, 0.5)?;
	expect_that!(j.value, near(1.25_f64.ln(), 1e-15));
	expect_that!(j.first, near(0.8, 1e-15));
	expect_that!(j.second, near(0.96, 1e-15));
	let bounds = f.jet(&mut Interval64Backend, Interval::new(0.49, 0.51)?)?;
	expect_true!(bounds.first.contains(0.8));
	Ok(())
}

#[gtest]
fn static_degree_controls_primary_polynomial_shape() -> Result<()> {
	let result =
		RemezRequest::binary64(function!(|x| x.exp()), ExactDomain::binary64(-1.0, 1.0), 3)
			.degree::<3>()
			.run()?;
	let _: &quest_polynomial::Polynomial<quest_polynomial::Chebyshev, f64, StaticShape<4>> =
		result.polynomial();
	expect_that!(result.polynomial().shape().coefficient_count(), eq(4));
	expect_that!(result.polynomial().coefficients().len(), eq(4));
	expect_that!(result.uniform_error().bound().upper(), lt(0.006));
	Ok(())
}

#[gtest]
fn const_metadata_is_structural() {
	const METADATA: quest_polynomial::ExpressionMetadata = {
		let f = function!(|x| (1.0 + x * x).ln());
		f.static_metadata()
	};
	expect_that!(METADATA.nodes, eq(6));
	expect_that!(METADATA.depth, eq(4));
	expect_that!(METADATA.operations, eq(3));
	expect_that!(METADATA.positive_jet_arguments, eq(1));
	expect_that!(METADATA.nonzero_denominators, eq(0));
}

#[gtest]
fn all_unary_rules_match_independent_analytic_derivatives() -> Result<()> {
	fn check<E: Expression>(f: &Function<E>, expected: [f64; 3]) -> Result<()> {
		let jet = f.jet(&mut F64Backend, 0.5)?;
		for (actual, value) in [jet.value, jet.first, jet.second].into_iter().zip(expected) {
			expect_that!(actual, near(value, 2e-14));
		}
		let interval = f.jet(&mut Interval64Backend, Interval::point(0.5)?)?;
		for (bound, value) in [interval.value, interval.first, interval.second]
			.into_iter()
			.zip(expected)
		{
			expect_true!(bound.contains(value));
		}
		Ok(())
	}
	let x = 0.5_f64;
	check(&function!(|x| x.exp()), [x.exp(), x.exp(), x.exp()])?;
	check(&function!(|x| x.ln()), [x.ln(), 2.0, -4.0])?;
	check(&function!(|x| x.sin()), [x.sin(), x.cos(), -x.sin()])?;
	check(&function!(|x| x.cos()), [x.cos(), -x.sin(), -x.cos()])?;
	check(
		&function!(|x| x.sqrt()),
		[x.sqrt(), 0.5 / x.sqrt(), -0.25 / (x * x.sqrt())],
	)?;
	check(
		&function!(|x| -(x - 1.0) / (x + 1.0)),
		[1.0 / 3.0, -2.0 / 2.25, 4.0 / 3.375],
	)?;
	Ok(())
}

struct MoveOnlyExponential(Box<u8>);
impl GenericFunction for MoveOnlyExponential {
	fn evaluate<B: Backend>(
		&self,
		backend: &mut B,
		x: B::Scalar,
	) -> std::result::Result<B::Scalar, B::Error> {
		std::hint::black_box(&self.0);
		backend.exp(x)
	}
}
#[gtest]
fn move_only_extension_retains_assumption_and_ad() -> Result<()> {
	let premise = ConsistencyAssumption::SameFunctionAndValidEnclosures;
	let function = AssumedFunction::new(MoveOnlyExponential(Box::new(7)), premise);
	let jet = function.jet(&mut F64Backend, 0.5)?;
	expect_that!(jet.first, eq(0.5_f64.exp()));
	expect_that!(jet.second, eq(0.5_f64.exp()));
	let result = RemezRequest::binary64(function, ExactDomain::binary64(-1.0, 1.0), 3).run()?;
	expect_that!(result.uniform_error().bound().upper(), lt(0.006));
	expect_that!(result.target().assumption(), eq(premise));
	Ok(())
}

#[gtest]
fn constant_and_derivative_domain_contracts() -> Result<()> {
	const C: Function<quest_polynomial::typed::Typed<quest_polynomial::typed::ConstantNode>> =
		function!(|x| 0.5);
	expect_that!(C.evaluate(&mut F64Backend, 4.0)?, eq(0.5));
	expect_true!(
		function!(|x| f64::NAN)
			.evaluate(&mut F64Backend, 0.0)
			.is_err()
	);
	expect_true!(
		function!(|x| x.sqrt())
			.evaluate(&mut F64Backend, 0.0)
			.is_ok()
	);
	expect_true!(function!(|x| x.sqrt()).jet(&mut F64Backend, 0.0).is_err());
	let f = function!(|x| (x.sqrt() + x.ln()) / (x + 1.0));
	expect_that!(f.metadata().positive_jet_arguments, eq(2));
	expect_that!(f.metadata().nonzero_denominators, eq(1));
	Ok(())
}

#[gtest]
fn evaluations_and_exchange_respect_work_budget() -> Result<()> {
	let function = function!(|x| x.exp());
	let budget = Budget::new(1);
	expect_true!(
		function
			.jet(&mut BudgetedBackend::new(&mut F64Backend, &budget), 0.5)
			.is_err()
	);
	for max_work in [0, 1, 130] {
		let options = RemezOptions {
			max_iterations: 1,
			limits: Limits {
				max_work,
				..Limits::default()
			},
			..RemezOptions::default()
		};
		let Err(failure) =
			RemezRequest::binary64(function!(|x| x.exp()), ExactDomain::binary64(-1.0, 1.0), 3)
				.options(options)
				.run()
		else {
			return fail!("insufficient work admitted");
		};
		expect_true!(matches!(
			failure.error(),
			quest_polynomial::Error::Budget(_)
				| quest_polynomial::Error::Arithmetic(
					quest_numerics::arithmetic::ArithmeticError::Budget(_)
				)
		));
	}
	Ok(())
}

#[gtest]
fn failures_retain_original_request_constants_shape_and_premise() -> Result<()> {
	let constant = 0.1_f64;
	let target = function!(|x| x * 0.25 + constant);
	let metadata = target.metadata();
	let options = RemezOptions {
		limits: Limits {
			max_work: 0,
			..Limits::default()
		},
		..RemezOptions::default()
	};
	let Err(failure) = RemezRequest::binary64(target, ExactDomain::binary64(-1.0, 1.0), 3)
		.degree::<3>()
		.options(options.clone())
		.run()
	else {
		return fail!("zero budget admitted");
	};
	expect_that!(
		failure
			.request()
			.target()
			.evaluate(&mut F64Backend, 0.0)?
			.to_bits(),
		eq(constant.to_bits())
	);
	expect_that!(failure.request().target().metadata(), eq(metadata));
	expect_that!(failure.request().requested_degree(), eq(Some(3)));
	expect_that!(failure.request().shape().coefficient_count(), eq(4));
	let premise = ConsistencyAssumption::SameFunctionAndValidEnclosures;
	let target = AssumedFunction::new(MoveOnlyExponential(Box::new(9)), premise);
	let Err(failure) = RemezRequest::binary64(target, ExactDomain::binary64(-1.0, 1.0), 3)
		.options(options)
		.run()
	else {
		return fail!("zero budget admitted");
	};
	expect_that!(failure.request().target().assumption(), eq(premise));
	expect_that!(
		failure.request().target().evaluate(&mut F64Backend, 0.0)?,
		eq(1.0)
	);
	Ok(())
}
