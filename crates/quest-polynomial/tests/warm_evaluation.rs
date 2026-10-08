use googletest::prelude::*;
use quest_numerics::arithmetic::{F64Backend, Interval64Backend};
use quest_polynomial::{
	Chebyshev, Complex64, GenericFunction, Interval, Limits, Polynomial, function,
};
#[gtest]
#[expect(
	clippy::arithmetic_side_effects,
	reason = "Operators construct an expression before allocator tracking starts"
)]
fn warmed_scalar_interval_and_derivative_evaluation_allocate_nothing() -> Result<()> {
	let function = function!(|x| (x * x + 1.0).ln());
	let polynomial = Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.3, 0.0); 32],
		Limits::default(),
	)?;
	let interval = Interval::new(0.2, 0.3)?;
	let execute = || -> quest_polynomial::Result<()> {
		for _ in 0..32 {
			std::hint::black_box(function.evaluate(&mut F64Backend, 0.3)?);
			std::hint::black_box(function.jet(&mut F64Backend, 0.3)?);
			std::hint::black_box(function.evaluate(&mut Interval64Backend, interval)?);
			std::hint::black_box(function.jet(&mut Interval64Backend, interval)?);
			std::hint::black_box(polynomial.evaluate_real(0.3)?);
			std::hint::black_box(polynomial.jet_interval(interval)?);
		}
		Ok(())
	};
	execute()?;
	let mut result = Ok(());
	let stats = allocation_counter::measure(|| {
		result = execute();
	});
	result?;
	expect_that!(stats.count_total, eq(0));
	Ok(())
}
