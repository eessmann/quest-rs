use googletest::prelude::*;
use quest_polynomial::{Complex64, Laurent, Limits, Polynomial};

#[gtest]
fn zero_padding_does_not_create_a_pole_or_underflow() -> Result<()> {
	let polynomial = Polynomial::new(
		Laurent::new(-2),
		vec![
			Complex64::new(0.0, 0.0),
			Complex64::new(0.0, 0.0),
			Complex64::new(1.0, 0.0),
		],
		Limits::default(),
	)?;
	for x in [0.0, 1e-200, 1e200] {
		expect_eq!(
			polynomial.evaluate(Complex64::new(x, 0.0))?,
			Complex64::new(1.0, 0.0)
		);
		expect_eq!(polynomial.evaluate_real(x)?, 1.0);
		let jet = polynomial.jet_interval(quest_polynomial::Interval::point(x)?)?;
		expect_true!(jet.value.contains(1.0));
		expect_eq!(jet.first.lower(), 0.0);
		expect_eq!(jet.first.upper(), 0.0);
		expect_eq!(jet.second.lower(), 0.0);
		expect_eq!(jet.second.upper(), 0.0);
	}
	expect_eq!(polynomial.effective_support(), Some((0, 0)));
	expect_eq!(polynomial.coefficients().len(), 3);
	Ok(())
}

#[gtest]
fn complex_and_generic_evaluation_share_shift_work_admission() -> Result<()> {
	let polynomial = Polynomial::new(
		Laurent::new(1024),
		vec![Complex64::new(1.0, 0.0)],
		Limits {
			shapes: (Limits::default()).shapes,
			resources: quest_numerics::ResourceLimits {
				max_work_units: 3,
				..(Limits::default()).resources
			},
		},
	)?;
	expect_true!(matches!(
		polynomial.evaluate(Complex64::new(1.0, 0.0)),
		Err(quest_polynomial::Error::Interval(
			quest_numerics::Error::Resource(_)
		))
	));
	expect_true!(matches!(
		polynomial.evaluate_real(1.0),
		Err(quest_polynomial::Error::Interval(
			quest_numerics::Error::Resource(_)
		))
	));
	Ok(())
}

#[gtest]
fn positive_powers_preserve_representable_horner_scaling() -> Result<()> {
	for (coefficient, argument, expected) in [(1e-300, 1e200, 1e100), (1e300, 1e-200, 1e-100)] {
		let polynomial = Polynomial::new(
			quest_polynomial::Monomial,
			vec![
				Complex64::new(0.0, 0.0),
				Complex64::new(0.0, 0.0),
				Complex64::new(coefficient, 0.0),
			],
			Limits::default(),
		)?;
		let real = polynomial.evaluate_real(argument)?;
		let complex = polynomial.evaluate(Complex64::new(argument, 0.0))?;
		expect_that!(std::ops::Div::div(real, expected), near(1.0, 1e-14));
		expect_that!(std::ops::Div::div(complex.re, expected), near(1.0, 1e-14));
	}
	Ok(())
}

#[gtest]
fn negative_powers_use_a_scaled_reciprocal() -> Result<()> {
	let polynomial = Polynomial::new(
		Laurent::new(-1),
		vec![Complex64::new(1.0, 0.0)],
		Limits::default(),
	)?;
	for x in [1e-200, 1e200] {
		let value = polynomial.evaluate(Complex64::new(x, 0.0))?;
		expect_that!(value.re * x, near(1.0, 1e-14));
		expect_eq!(value.im, 0.0);
	}
	expect_true!(polynomial.evaluate(Complex64::new(0.0, 0.0)).is_err());
	let value = polynomial.evaluate(Complex64::new(f64::MAX, f64::MAX))?;
	expect_true!(value.re > 0.0);
	expect_eq!(value.re, -value.im);
	expect_that!(value.re * f64::MAX, near(0.5, 1e-14));
	Ok(())
}
