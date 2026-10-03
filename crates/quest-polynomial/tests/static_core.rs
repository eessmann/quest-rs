#![allow(
	clippy::panic_in_result_fn,
	reason = "Mathematical regression assertions intentionally fail the test"
)]
#![feature(const_trait_impl, const_ops)]
#![allow(clippy::arithmetic_side_effects, clippy::float_cmp)]
use quest_numerics::arithmetic::{Backend, ExactConstant, F64Backend, MpBackend, Precision};
use quest_polynomial::{Function, GenericFunction, function};

#[test]
fn canonical_function_is_const_and_derivatives_share_its_body()
-> Result<(), Box<dyn std::error::Error>> {
	const F: Function<
		quest_polynomial::typed::Typed<
			quest_polynomial::typed::Product<
				quest_polynomial::typed::VariableNode<0>,
				quest_polynomial::typed::VariableNode<0>,
			>,
		>,
	> = function!(|x| x * x);
	let j = F.jet(&mut F64Backend, 3.0)?;
	assert_eq!((j.value, j.first, j.second), (9.0, 6.0, 2.0));
	assert_eq!(F.metadata().operations, 1);
	Ok(())
}

#[test]
fn owned_exact_decimal_is_not_first_rounded_to_binary64() -> Result<(), Box<dyn std::error::Error>>
{
	let constant = quest_polynomial::typed::exact(ExactConstant::Decimal(
		"1.000000000000000000000000000001".into(),
	));
	let f = function!(|x| x + constant);
	let mut backend = MpBackend::new(Precision::default())?;
	let zero = backend.point(0.0)?;
	let actual = f.evaluate(&mut backend, zero)?;
	let expected = backend.constant(&ExactConstant::Decimal(
		"1.000000000000000000000000000001".into(),
	))?;
	assert_eq!(actual, expected);
	assert!(actual > backend.point(1.0)?);
	Ok(())
}

#[test]
fn vector_function_derives_a_checked_jacobian() -> Result<(), Box<dyn std::error::Error>> {
	let f = function!(|x, y| [x * x + y, x * y]);
	let evaluated = f.jacobian(
		&mut F64Backend,
		[2.0, 3.0],
		quest_numerics::ad::JacobianLimits::default(),
	)?;
	assert_eq!(evaluated.value, [7.0, 6.0]);
	assert_eq!(evaluated.derivative.as_slice(), &[4.0, 1.0, 3.0, 2.0]);
	Ok(())
}

#[test]
fn first_derivative_does_not_request_a_second_derivative() -> Result<(), Box<dyn std::error::Error>>
{
	let f = function!(|x| x * x);
	let j = f.first(&mut F64Backend, 3.0)?;
	assert_eq!((j.value, j.first), (9.0, 6.0));
	Ok(())
}

#[test]
fn identity_and_constant_targets_admit_input_before_shortcuts() {
	assert!(
		function!(|x| x)
			.evaluate(&mut F64Backend, f64::NAN)
			.is_err()
	);
	assert!(
		function!(|x| 0.5)
			.evaluate(&mut F64Backend, f64::INFINITY)
			.is_err()
	);
}
