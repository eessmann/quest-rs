use googletest::prelude::*;
use quest_polynomial::{Chebyshev, Laurent, Limits, Polynomial};
use quest_qsp::{Complex64, Policy, SynthesisBuilder};
use std::ops::{Add, Mul, Sub};

fn close(actual: Complex64, expected: Complex64) {
	expect_that!(actual.sub(expected).norm(), le(1e-11));
}

#[gtest]
fn empty_generalized_laurent_is_zero_at_zero_and_positive_offset() -> Result<()> {
	for offset in [0, 3] {
		let target = Polynomial::new(Laurent::new(offset), vec![], Limits::default())?;
		let admitted = SynthesisBuilder::new()
			.unit_circle_response(&target)?
			.admit()?;
		expect_that!(
			admitted.source_coefficients().len(),
			eq(usize::try_from(offset)?.saturating_add(1))
		);
		expect_true!(
			admitted
				.source_coefficients()
				.iter()
				.all(|v| *v == Complex64::new(0.0, 0.0))
		);
	}
	Ok(())
}

#[gtest]
fn generalized_constant_keeps_complex_phase_and_final_convention_factor() -> Result<()> {
	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.3, -0.4)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	expect_that!(candidate.controls().len(), eq(1));
	let matrix = candidate.evaluate(Complex64::new(0.0, 1.0))?;
	let [
		[response, complement],
		[conjugate_complement, conjugate_response],
	] = matrix;
	close(response, Complex64::new(0.3, -0.4));
	close(complement, Complex64::new(-0.75_f64.sqrt(), 0.0));
	close(conjugate_complement, Complex64::new(0.75_f64.sqrt(), 0.0));
	close(conjugate_response, Complex64::new(0.3, 0.4));
	Ok(())
}

#[gtest]
fn generalized_linear_response_matches_direct_polynomial_on_the_circle() -> Result<()> {
	let coefficients = vec![Complex64::new(0.1, 0.2), Complex64::new(-0.3, 0.1)];
	let target = Polynomial::new(Laurent::new(0), coefficients, Limits::default())?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	for index in 0_u32..37 {
		let z = Complex64::from_polar(1.0, std::f64::consts::TAU.mul(f64::from(index)) / 37.0);
		let [[response, _], [_, _]] = candidate.evaluate(z)?;
		close(response, target.evaluate(z)?);
	}
	Ok(())
}

#[gtest]
fn nonsymmetric_cancellation_heavy_target_and_support_offset_are_retained() -> Result<()> {
	let target = Polynomial::new(
		Laurent::new(2),
		vec![
			Complex64::new(0.2, -0.1),
			Complex64::new(-0.2, 0.1),
			Complex64::new(0.05, 0.03),
		],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	for index in 0_u32..29 {
		let z = Complex64::from_polar(1.0, std::f64::consts::TAU.mul(f64::from(index)) / 29.0);
		let [[response, _], [_, _]] = candidate.evaluate(z)?;
		close(response, target.evaluate(z)?);
	}
	Ok(())
}

#[gtest]
fn canonical_degree_one_has_the_wx_imaginary_response_convention() -> Result<()> {
	let target = Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.real_parity_wx(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let [first, last] = candidate.phases() else {
		return fail!("degree-one target needs two phases");
	};
	expect_that!(*first, near(0.6_f64.asin() / 2.0, 1e-11));
	expect_that!(*last, near(*first, 1e-11));
	for x in [-1.0, -0.2, 0.0, 0.3, 1.0] {
		expect_that!(candidate.response(x)?, near(0.6 * x, 1e-11));
	}
	Ok(())
}

#[gtest]
fn target_admission_rejects_negative_support_and_noncontractivity() -> Result<()> {
	let negative = Polynomial::new(
		Laurent::new(-1),
		vec![Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	expect_true!(
		SynthesisBuilder::new()
			.unit_circle_response(&negative)
			.is_err()
	);
	let large = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(1.0, 0.0)],
		Limits::default(),
	)?;
	expect_true!(
		SynthesisBuilder::new()
			.unit_circle_response(&large)?
			.admit()
			.is_err()
	);
	let invalid = Policy {
		response_tolerance: 0.0,
		..Policy::default()
	};
	let small = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	expect_true!(
		SynthesisBuilder::new()
			.policy(invalid)
			.unit_circle_response(&small)?
			.admit()
			.is_err()
	);
	Ok(())
}

#[gtest]
fn frozen_generalized_matrix_product_matches_independent_expansion() -> Result<()> {
	let target = Polynomial::new(
		Laurent::new(0),
		vec![
			Complex64::new(0.25, 0.0),
			Complex64::new(0.0, 0.2),
			Complex64::new(-0.1, 0.1),
		],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.unit_circle_response(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let z = Complex64::new(0.6, 0.8);
	let mut reference = [
		[Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)],
		[Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0)],
	];
	for (index, control) in candidate.controls().iter().enumerate() {
		if index > 0 {
			for row in &mut reference {
				row[0] = row[0].mul(z);
			}
		}
		let old = reference;
		for row in 0..2 {
			for col in 0..2 {
				reference[row][col] = old[row][0]
					.mul(control[0][col])
					.add(old[row][1].mul(control[1][col]));
			}
		}
	}
	let actual = candidate.evaluate(z)?;
	for (a, b) in actual
		.into_iter()
		.flatten()
		.zip(reference.into_iter().flatten())
	{
		close(a, b);
	}
	Ok(())
}

#[gtest]
fn canonical_freezing_checks_actual_exported_phase_trigonometry() -> Result<()> {
	let target = Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.013_671_875, 0.0)],
		Limits::default(),
	)?;
	let policy = Policy {
		response_tolerance: 1e-18,
		max_completion_grid: 32,
		..Policy::default()
	};
	let completed = SynthesisBuilder::new()
		.policy(policy)
		.real_parity_wx(&target)?
		.admit()?
		.complete()?;
	expect_true!(matches!(
		completed.synthesize(),
		Err(quest_qsp::Error::NotEstablished {
			stage: "binary64 reconstruction",
			..
		})
	));

	// Trigonometric rounding can make one reconstructed value equal its target exactly.
	// Keep independent constants so at least one exposes reconstruction error.
	let mut largest_actual = 0.0_f64;
	for value in [0.013_671_875, 0.505_859_375] {
		let target = Polynomial::new(
			Chebyshev,
			vec![Complex64::new(value, 0.0)],
			Limits::default(),
		)?;
		let candidate = SynthesisBuilder::new()
			.real_parity_wx(&target)?
			.admit()?
			.complete()?
			.synthesize()?;
		let actual = (candidate.response(0.0)? - value).abs();
		expect_that!(
			candidate
				.reconstruction_residual()
				.ok_or(quest_qsp::Error::Target("missing production diagnostic"))?,
			ge(actual)
		);
		largest_actual = largest_actual.max(actual);
	}
	expect_that!(largest_actual, gt(0.0));
	Ok(())
}

#[gtest]
fn target_admission_rechecks_retained_storage_after_policy_changes() -> Result<()> {
	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.1, 0.0); 4],
		Limits::default(),
	)?;
	let mut policy = Policy::default();
	policy.limits.max_len = 1;
	expect_true!(
		SynthesisBuilder::new()
			.policy(policy)
			.unit_circle_response(&target)
			.is_err()
	);
	expect_true!(
		SynthesisBuilder::new()
			.unit_circle_response(&target)?
			.policy(policy)
			.admit()
			.is_err()
	);

	let target = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	policy = Policy::default();
	policy.limits.max_bytes = size_of::<Complex64>();
	// The converted target and the retained original source are separate owners.
	expect_true!(
		SynthesisBuilder::new()
			.policy(policy)
			.unit_circle_response(&target)
			.is_err()
	);
	expect_true!(
		SynthesisBuilder::new()
			.unit_circle_response(&target)?
			.policy(policy)
			.admit()
			.is_err()
	);
	Ok(())
}

#[gtest]
fn completion_accounts_for_live_payload_beside_fft_plan_and_repeated_work() -> googletest::Result<()>
{
	let target = Polynomial::new(Chebyshev, vec![Complex64::new(0.1, 0.0)], Limits::default())?;
	let mut policy = Policy::default();
	policy.limits.max_bytes = 32_768;
	let admitted = SynthesisBuilder::new()
		.policy(policy)
		.real_parity_wx(&target)?
		.admit()?;
	expect_true!(admitted.complete().is_err());
	let mut policy = Policy::default();
	policy.limits.max_work = 1_280; // One size32 FFT fits; four transforms do not.
	let admitted = SynthesisBuilder::new()
		.policy(policy)
		.real_parity_wx(&target)?
		.admit()?;
	expect_true!(admitted.complete().is_err());
	Ok(())
}
