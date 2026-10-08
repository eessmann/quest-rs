#![cfg(feature = "benchmark-support")]
use googletest::prelude::*;
use quest_qsp::{Complex64, Policy, benchmark_support};
use std::ops::Sub;

#[gtest]
fn production_forward_inverse_recovers_complex_reflections() -> googletest::Result<()> {
	let expected = vec![
		Complex64::new(0.001, -0.002),
		Complex64::new(-0.003, 0.002),
		Complex64::new(0.004, 0.001),
	];
	let pair = benchmark_support::forward(&expected, Policy::default())?;
	let actual = benchmark_support::inverse(&pair, Policy::default())?;
	expect_that!(actual.len(), eq(expected.len()));
	for (actual, expected) in actual.iter().zip(expected) {
		expect_that!((*actual).sub(expected).norm(), lt(1e-14));
	}
	Ok(())
}

#[test]
fn benchmark_kernels_reject_empty_nonfinite_and_invalid_policy() {
	assert!(benchmark_support::forward(&[], Policy::default()).is_err());
	assert!(
		benchmark_support::forward(&[Complex64::new(f64::NAN, 0.0)], Policy::default()).is_err()
	);
	assert!(
		benchmark_support::complete(
			&[Complex64::new(0.2, 0.0)],
			Policy {
				accuracy: quest_qsp::AccuracyPolicy {
					response_tolerance: 0.0,
					..(Policy::default()).accuracy
				},
				..Policy::default()
			}
		)
		.is_err()
	);
}

#[gtest]
fn completion_helper_retains_only_the_two_returned_buffers() -> googletest::Result<()> {
	let pair = benchmark_support::complete(
		&[Complex64::new(0.2, 0.0)],
		Policy {
			algorithm: quest_qsp::SynthesisAlgorithm::RhwHalfCholesky,
			..Policy::default()
		},
	)?;
	let (_pair, ownership) = pair.into_parts();
	expect_that!(ownership.len(), eq(2));
	expect_that!(
		ownership
			.iter()
			.map(quest_numerics::MemoryReservation::bytes)
			.sum::<usize>(),
		eq(32)
	);
	Ok(())
}
