#![forbid(unsafe_code)]
#![allow(
	clippy::unwrap_used,
	clippy::float_cmp,
	reason = "Exact profile and binary64 fixtures fail immediately on violated contracts"
)]
use mathcore::{
	arithmetic::ExactConstant,
	dynamic::{DynamicExpression, ExpressionLimits},
	multivariate::PolynomialLimits,
};
use quest_numerics::arithmetic::{MpBackend, Precision};

#[test]
fn lowered_constants_reject_a_changed_precision_before_execution() {
	let source =
		DynamicExpression::constant(ExactConstant::Rational(1, 3), ExpressionLimits::default())
			.unwrap();
	let mut low = MpBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})
	.unwrap();
	let kernel = source.lower(&mut low, &[]).unwrap();
	let polynomial = source
		.polynomial(&[], PolynomialLimits::default())
		.unwrap()
		.lower(&mut low)
		.unwrap();
	let mut high = MpBackend::new(Precision {
		bits: 256,
		..Precision::default()
	})
	.unwrap();
	assert!(
		kernel.evaluate(&mut high, &[]).is_err(),
		"a low-precision stored constant cannot execute under a high-precision profile"
	);
	assert!(polynomial.evaluate(&mut high, &[]).is_err());
}

#[test]
fn neutral_errors_retain_structured_sources() {
	use std::error::Error;
	let core = mathcore::arithmetic::ArithmeticError::Domain("ln");
	let error = quest_numerics::arithmetic::ArithmeticError::from(core.clone());
	assert!(
		matches!(error, quest_numerics::arithmetic::ArithmeticError::Core(ref cause) if cause == &core)
	);
	assert_eq!(error.source().unwrap().to_string(), core.to_string());
}

#[test]
fn warm_binary64_workspace_evaluation_has_no_scratch_allocations() {
	use mathcore::{
		identity::{Owner, Symbol},
		scalar::BinaryOperation,
	};
	use quest_numerics::arithmetic::{Budget, BudgetedBackend, F64Backend};
	let symbol = Symbol::new(Owner::new(5), 0);
	let x = DynamicExpression::variable(symbol, ExpressionLimits::default()).unwrap();
	let source = x.binary(BinaryOperation::Multiply, &x).unwrap();
	let mut backend = F64Backend;
	let kernel = source.lower(&mut backend, &[symbol]).unwrap();
	let mut workspace = kernel.workspace().unwrap();
	let measured = allocation_counter::measure(|| {
		for _ in 0..100 {
			assert_eq!(
				kernel
					.evaluate_with_workspace(&mut backend, &[3.0], &mut workspace)
					.unwrap(),
				9.0
			);
		}
	});
	assert_eq!(measured.count_total, 0);
	let allocating = allocation_counter::measure(|| {
		std::hint::black_box(kernel.evaluate(&mut backend, &[3.0]).unwrap());
	});
	assert!(
		allocating.count_total > 0,
		"convenience evaluation is an allocating positive control"
	);
	let budget = Budget::new(100);
	let mut budgeted = BudgetedBackend::new(&mut backend, &budget);
	assert_eq!(
		kernel
			.evaluate_with_workspace(&mut budgeted, &[2.0], &mut workspace)
			.unwrap(),
		4.0
	);
	assert!(budget.used() > 0);
	let exhausted = Budget::new(0);
	assert!(
		kernel
			.evaluate_with_workspace(
				&mut BudgetedBackend::new(&mut backend, &exhausted),
				&[2.0],
				&mut workspace
			)
			.is_err()
	);
	assert_eq!(
		kernel
			.evaluate_with_workspace(&mut backend, &[2.0], &mut workspace)
			.unwrap(),
		4.0
	);
}

#[test]
fn changing_resource_limits_does_not_change_scalar_policy_and_relowering_preserves_precision() {
	use quest_numerics::arithmetic::{Backend, ExactConstant};
	let source =
		DynamicExpression::constant(ExactConstant::Rational(1, 3), ExpressionLimits::default())
			.unwrap();
	let mut low = MpBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})
	.unwrap();
	let kernel = source.lower(&mut low, &[]).unwrap();
	let mut limited = MpBackend::new(Precision {
		bits: 64,
		max_operations: 100,
		max_abs_exponent: 100,
	})
	.unwrap();
	assert!(kernel.evaluate(&mut limited, &[]).is_ok());
	let mut high = MpBackend::new(Precision {
		bits: 256,
		..Precision::default()
	})
	.unwrap();
	let work_before = high.operations();
	assert!(kernel.evaluate(&mut high, &[]).is_err());
	assert_eq!(high.operations(), work_before);
	let relowered = source.lower(&mut high, &[]).unwrap();
	let expected = high.constant(&ExactConstant::Rational(1, 3)).unwrap();
	assert_eq!(relowered.evaluate(&mut high, &[]).unwrap(), expected);
	assert_ne!(kernel.evaluate(&mut low, &[]).unwrap(), expected);
}

#[test]
fn ad_profiles_preserve_precision_and_distinguish_scalar_shapes() {
	use quest_numerics::arithmetic::{Backend, FirstBackend, GradientBackend, JetBackend};
	let source =
		DynamicExpression::constant(ExactConstant::Rational(1, 3), ExpressionLimits::default())
			.unwrap();
	let mut low = MpBackend::new(Precision {
		bits: 64,
		..Precision::default()
	})
	.unwrap();
	let mut high = MpBackend::new(Precision {
		bits: 256,
		..Precision::default()
	})
	.unwrap();
	let point_profile = low.profile();
	let first_profile = FirstBackend(&mut low).profile();
	let jet_profile = JetBackend(&mut low).profile();
	let gradient_profile = GradientBackend::<_, 2>(&mut low).profile();
	assert_eq!(first_profile.precision_bits(), 64);
	assert_eq!(FirstBackend(&mut high).profile().precision_bits(), 256);
	assert_ne!(point_profile, first_profile);
	assert_ne!(first_profile, jet_profile);
	assert_ne!(first_profile, gradient_profile);
	assert_ne!(
		gradient_profile,
		GradientBackend::<_, 3>(&mut low).profile()
	);
	let first = source.lower(&mut FirstBackend(&mut low), &[]).unwrap();
	assert!(first.evaluate(&mut FirstBackend(&mut high), &[]).is_err());
	let jet = source.lower(&mut JetBackend(&mut low), &[]).unwrap();
	assert!(jet.evaluate(&mut JetBackend(&mut high), &[]).is_err());
	let gradient = source
		.lower(&mut GradientBackend::<_, 2>(&mut low), &[])
		.unwrap();
	assert!(
		gradient
			.evaluate(&mut GradientBackend::<_, 2>(&mut high), &[])
			.is_err()
	);
}
