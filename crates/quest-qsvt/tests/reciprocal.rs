#![allow(
	clippy::arithmetic_side_effects,
	reason = "Independent numerical test comparisons"
)]
use quest_qsvt::{
	NumericalPolicy,
	reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence},
};

#[googletest::gtest]
fn reciprocal_is_odd_bounded_and_has_independent_error_bound() -> googletest::Result<()> {
	let bounds = SpectralBounds::new(
		0.5,
		1.0,
		SpectralEvidence::Analytic {
			description: "test diagonal spectrum".into(),
		},
	)?;
	let reciprocal =
		ReciprocalPolynomial::geometric(&bounds, 1.0, 1e-3, 255, NumericalPolicy::default())?;
	let c = reciprocal.scale();
	for i in 0..=200_u32 {
		let x = f64::from(i) / 200.0;
		let value = reciprocal.evaluate(x)?;
		googletest::expect_true!(value.abs() <= 0.500_000_01);
		googletest::expect_true!((value + reciprocal.evaluate(-x)?).abs() < 1e-13);
		if x >= 0.5 {
			googletest::expect_true!((value - c / x).abs() <= reciprocal.error_bound());
		}
	}
	googletest::expect_true!(reciprocal.error_bound() <= 1e-3);
	googletest::expect_true!(
		ReciprocalPolynomial::geometric(&bounds, 1.0, 1e-9, 3, NumericalPolicy::default()).is_err()
	);
	Ok(())
}
#[googletest::gtest]
fn reciprocal_rejects_inconsistent_spectrum_and_preserves_physical_scale() -> googletest::Result<()>
{
	googletest::expect_true!(
		SpectralBounds::new(
			0.0,
			1.0,
			SpectralEvidence::CallerPremise {
				description: "singular".into()
			}
		)
		.is_err()
	);
	let bounds = SpectralBounds::new(
		2.0,
		4.0,
		SpectralEvidence::CallerPremise {
			description: "declared physical bounds".into(),
		},
	)?;
	googletest::expect_true!(
		ReciprocalPolynomial::geometric(&bounds, 2.0, 0.1, 511, NumericalPolicy::default())
			.is_err()
	);
	let reciprocal =
		ReciprocalPolynomial::geometric(&bounds, 4.0, 0.01, 511, NumericalPolicy::default())?;
	googletest::expect_that!(
		reciprocal.physical_rescaling(3.0)?,
		googletest::matchers::eq(3.0 / (4.0 * reciprocal.scale()))
	);
	Ok(())
}
