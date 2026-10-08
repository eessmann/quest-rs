use googletest::prelude::*;
use quest_numerics::{
	Complex64, ConvolutionWorkspace, Error, FftBackend, FftDirection, FftWorkspace, Interval,
	Limits, Normalization,
};
use std::ops::{Add, Mul, Sub};

fn close(actual: Complex64, expected: Complex64) {
	expect_that!(actual.sub(expected).norm(), le(1e-11));
}

#[gtest]
fn fft_matches_independent_four_point_dft_and_normalized_round_trip() -> Result<()> {
	let original = [
		Complex64::new(1.0, 2.0),
		Complex64::new(-3.0, 1.0),
		Complex64::new(2.0, -4.0),
		Complex64::new(0.5, 0.0),
	];
	let mut values = original;
	let mut fft = FftWorkspace::new(4, FftBackend::Scalar, Limits::default())?;
	fft.transform(&mut values, FftDirection::Forward, Normalization::None)?;
	for (k, actual) in (0_u32..4).zip(&values) {
		let expected =
			(0_u32..4)
				.zip(&original)
				.fold(Complex64::new(0.0, 0.0), |sum, (j, value)| {
					let angle = -std::f64::consts::TAU.mul(f64::from(k)).mul(f64::from(j)) / 4.0;
					sum.add(value.mul(Complex64::from_polar(1.0, angle)))
				});
		close(*actual, expected);
	}
	fft.transform(&mut values, FftDirection::Inverse, Normalization::ByLength)?;
	for (actual, expected) in values.into_iter().zip(original) {
		close(actual, expected);
	}
	Ok(())
}

#[gtest]
fn complex_convolution_has_exact_support_and_reuses_buffers() -> Result<()> {
	let left = [
		Complex64::new(1.0, 1.0),
		Complex64::new(2.0, -1.0),
		Complex64::new(-1.0, 0.5),
	];
	let right = [Complex64::new(0.5, -2.0), Complex64::new(-3.0, 1.0)];
	let mut workspace = ConvolutionWorkspace::new(3, 2, FftBackend::Scalar, Limits::default())?;
	let output = workspace.convolve(&left, &right)?;
	expect_that!(output.len(), eq(4));
	let mut expected = [Complex64::new(0.0, 0.0); 4];
	for (i, a) in left.iter().enumerate() {
		for (j, b) in right.iter().enumerate() {
			let index = i
				.checked_add(j)
				.ok_or(Error::Length("test index overflow"))?;
			let target = expected
				.get_mut(index)
				.ok_or(Error::Length("test support"))?;
			*target = target.add(a.mul(b));
		}
	}
	for (actual, wanted) in output.iter().zip(expected) {
		close(*actual, wanted);
	}
	let first_pointer = output.as_ptr();
	let shorter = workspace.convolve(
		left.get(..1).ok_or(Error::Length("left prefix"))?,
		right.get(..1).ok_or(Error::Length("right prefix"))?,
	)?;
	expect_that!(shorter.len(), eq(1));
	expect_that!(shorter.as_ptr(), eq(first_pointer));
	close(
		*shorter
			.first()
			.ok_or(Error::Length("missing coefficient"))?,
		Complex64::new(2.5, -1.5),
	);
	Ok(())
}

#[gtest]
fn fft_rejects_invalid_lengths_budgets_and_nonfinite_inputs() -> Result<()> {
	expect_true!(FftWorkspace::new(0, FftBackend::Scalar, Limits::default()).is_err());
	expect_true!(
		ConvolutionWorkspace::new(usize::MAX, 2, FftBackend::Scalar, Limits::default()).is_err()
	);
	for limits in [
		Limits {
			shapes: (Limits::default()).shapes,
			resources: quest_numerics::ResourceLimits {
				max_peak_bytes: 1,
				..(Limits::default()).resources
			},
		},
		Limits {
			shapes: (Limits::default()).shapes,
			resources: quest_numerics::ResourceLimits {
				max_work_units: 1,
				..(Limits::default()).resources
			},
		},
		Limits {
			shapes: quest_numerics::ShapeLimits {
				max_coefficients: 1,
				max_fft_len: 1,
				..(Limits::default()).shapes
			},
			resources: (Limits::default()).resources,
		},
	] {
		expect_true!(FftWorkspace::new(8, FftBackend::Scalar, limits).is_err());
	}
	let mut fft = FftWorkspace::new(2, FftBackend::Scalar, Limits::default())?;
	let mut invalid = [
		Complex64::new(f64::NAN, 0.0),
		Complex64::new(f64::INFINITY, 0.0),
	];
	expect_that!(
		fft.transform(&mut invalid, FftDirection::Forward, Normalization::None),
		err(eq(&Error::NonFinite { index: 0 }))
	);
	expect_true!(
		fft.transform(&mut [], FftDirection::Forward, Normalization::None)
			.is_err()
	);
	Ok(())
}

#[gtest]
fn intervals_enclose_arithmetic_and_check_elementary_domains() -> Result<()> {
	expect_true!(Interval::new(2.0, 1.0).is_err());
	expect_true!(Interval::point(f64::NAN).is_err());
	let x = Interval::new(-2.0, 3.0)?;
	let square = x.square()?;
	expect_that!(square.lower(), eq(0.0));
	expect_that!(square.upper(), ge(9.0));
	let product = x.checked_mul(Interval::new(4.0, 5.0)?)?;
	expect_that!(product.lower(), le(-10.0));
	expect_that!(product.upper(), ge(15.0));
	expect_true!(x.sqrt().is_err());
	expect_true!(x.ln().is_err());
	expect_true!(x.checked_div(Interval::new(-1.0, 1.0)?).is_err());
	let root = Interval::new(1.0, 4.0)?.sqrt()?;
	expect_that!(root.lower(), le(1.0));
	expect_that!(root.upper(), ge(2.0));
	let exponential = Interval::point(0.0)?.exp()?;
	expect_true!(exponential.contains(1.0));
	let logarithm = Interval::point(1.0)?.ln()?;
	expect_true!(logarithm.contains(0.0));
	let sine = Interval::new(0.0, 2.0)?.sin()?;
	expect_true!(sine.contains(0.0));
	expect_true!(sine.contains(1.0));
	Ok(())
}

#[gtest]
fn forward_normalization_and_inverse_sign_are_independent() -> Result<()> {
	let mut fft = FftWorkspace::new(4, FftBackend::Scalar, Limits::default())?;
	let impulse = [
		Complex64::new(0.0, 0.0),
		Complex64::new(1.0, 0.0),
		Complex64::new(0.0, 0.0),
		Complex64::new(0.0, 0.0),
	];
	let mut forward = impulse;
	fft.transform(&mut forward, FftDirection::Forward, Normalization::ByLength)?;
	let expected = [
		Complex64::new(0.25, 0.0),
		Complex64::new(0.0, -0.25),
		Complex64::new(-0.25, 0.0),
		Complex64::new(0.0, 0.25),
	];
	for (actual, wanted) in forward.into_iter().zip(expected) {
		close(actual, wanted);
	}
	let mut inverse = impulse;
	fft.transform(&mut inverse, FftDirection::Inverse, Normalization::None)?;
	let expected = [
		Complex64::new(1.0, 0.0),
		Complex64::new(0.0, 1.0),
		Complex64::new(-1.0, 0.0),
		Complex64::new(0.0, -1.0),
	];
	for (actual, wanted) in inverse.into_iter().zip(expected) {
		close(actual, wanted);
	}
	Ok(())
}

#[gtest]
fn prime_length_fft_matches_direct_scalar_dft() -> Result<()> {
	let original: Vec<_> = (0_u32..17)
		.map(|j| Complex64::new(f64::from(j) / 17.0, f64::from(j).sin()))
		.collect();
	let mut values = original.clone();
	let mut fft = FftWorkspace::new(17, FftBackend::Scalar, Limits::default())?;
	fft.transform(&mut values, FftDirection::Forward, Normalization::None)?;
	for (k, actual) in (0_u32..17).zip(&values) {
		let expected =
			(0_u32..17)
				.zip(&original)
				.fold(Complex64::new(0.0, 0.0), |sum, (j, value)| {
					let angle = -std::f64::consts::TAU.mul(f64::from(k)).mul(f64::from(j)) / 17.0;
					sum.add(value.mul(Complex64::from_polar(1.0, angle)))
				});
		close(*actual, expected);
	}
	Ok(())
}

#[gtest]
fn convolution_recovers_after_rejection_and_overflow() -> Result<()> {
	let mut workspace = ConvolutionWorkspace::new(2, 2, FftBackend::Scalar, Limits::default())?;
	let good = [Complex64::new(2.0, 0.0)];
	expect_true!(workspace.convolve(&[], &good).is_err());
	expect_true!(
		workspace
			.convolve(&[Complex64::new(0.0, 0.0); 3], &good)
			.is_err()
	);
	expect_that!(
		workspace.convolve(&[Complex64::new(f64::NAN, 0.0)], &good),
		err(eq(&Error::NonFinite { index: 0 }))
	);
	expect_true!(
		workspace
			.convolve(&[Complex64::new(f64::MAX, 0.0)], &good)
			.is_err()
	);
	let output = workspace.convolve(&good, &good)?;
	close(
		*output.first().ok_or(Error::Length("output"))?,
		Complex64::new(4.0, 0.0),
	);
	Ok(())
}

#[gtest]
fn resource_admission_includes_convolution_buffers_and_three_transforms() -> Result<()> {
	let workspace = ConvolutionWorkspace::new(4, 3, FftBackend::Scalar, Limits::default())?;
	let usage = workspace.resource_usage();
	let bytes = usage
		.buffer_bytes
		.checked_add(usage.planner_bytes_estimate)
		.ok_or(Error::Overflow)?;
	let exact = Limits {
		shapes: quest_numerics::ShapeLimits {
			max_coefficients: workspace.fft_len(),
			max_fft_len: workspace.fft_len(),
			..(quest_numerics::OperationLimits::default()).shapes
		},
		resources: quest_numerics::ResourceLimits {
			max_peak_bytes: bytes,
			max_work_units: usage.work_units,
		},
	};
	expect_true!(ConvolutionWorkspace::new(4, 3, FftBackend::Scalar, exact).is_ok());
	expect_true!(
		ConvolutionWorkspace::new(
			4,
			3,
			FftBackend::Scalar,
			Limits {
				shapes: (exact).shapes,
				resources: quest_numerics::ResourceLimits {
					max_peak_bytes: bytes.checked_sub(1).ok_or(Error::Overflow)?,
					..(exact).resources
				}
			}
		)
		.is_err()
	);
	expect_true!(
		ConvolutionWorkspace::new(
			4,
			3,
			FftBackend::Scalar,
			Limits {
				shapes: (exact).shapes,
				resources: quest_numerics::ResourceLimits {
					max_work_units: usage.work_units.checked_sub(1).ok_or(Error::Overflow)?,
					..(exact).resources
				}
			}
		)
		.is_err()
	);
	Ok(())
}

#[gtest]
fn interval_arithmetic_preserves_subnormals_and_rejects_overflow() -> Result<()> {
	let tiny = f64::from_bits(1);
	let half = Interval::point(tiny)?.checked_div(Interval::point(2.0)?)?;
	expect_that!(half.lower(), eq(0.0));
	expect_that!(half.upper(), eq(tiny));
	let sum = Interval::point(0.1)?.checked_add(Interval::point(0.2)?)?;
	expect_true!(sum.contains(0.3));
	expect_true!(sum.contains(0.1 + 0.2));
	let difference = Interval::new(1.0, 2.0)?.checked_sub(Interval::new(3.0, 4.0)?)?;
	expect_that!(difference.lower(), eq(-3.0));
	expect_that!(difference.upper(), eq(-1.0));
	let negated = difference.checked_neg()?;
	expect_that!(negated.lower(), eq(1.0));
	expect_that!(negated.upper(), eq(3.0));
	expect_true!(
		Interval::point(f64::MAX)?
			.checked_add(Interval::point(f64::MAX)?)
			.is_err()
	);
	expect_true!(Interval::point(1000.0)?.exp().is_err());
	expect_true!(Interval::point(0.0)?.ln().is_err());
	let cosine = Interval::new(-1.0, 1.0)?.cos()?;
	expect_true!(cosine.contains(1.0));
	expect_true!(cosine.contains(0.540_302_305_868_139_8));
	let broad = Interval::new(-f64::MAX, f64::MAX)?.sin()?;
	expect_true!(broad.contains(-1.0));
	expect_true!(broad.contains(1.0));
	Ok(())
}

#[cfg(not(feature = "simd"))]
#[gtest]
fn simd_request_without_compiled_backend_is_an_error() {
	expect_that!(
		FftWorkspace::new(4, FftBackend::Simd, Limits::default()),
		err(eq(&Error::BackendUnavailable))
	);
}

#[cfg(feature = "simd")]
#[gtest]
fn available_simd_matches_scalar_with_explicit_tolerance() -> Result<()> {
	let mut simd = match FftWorkspace::new(8, FftBackend::Simd, Limits::default()) {
		Ok(workspace) => workspace,
		Err(Error::BackendUnavailable) => return Ok(()),
		Err(error) => return Err(error.into()),
	};
	let mut scalar = FftWorkspace::new(8, FftBackend::Scalar, Limits::default())?;
	let mut expected = [Complex64::new(0.5, -2.0); 8];
	let mut actual = expected;
	scalar.transform(&mut expected, FftDirection::Forward, Normalization::None)?;
	simd.transform(&mut actual, FftDirection::Forward, Normalization::None)?;
	for (actual, expected) in actual.into_iter().zip(expected) {
		close(actual, expected);
	}
	Ok(())
}

#[cfg(feature = "rayon")]
#[gtest]
fn caller_owned_parallel_policy_preserves_results_and_error_order() -> Result<()> {
	use quest_numerics::ExecutionPolicy;
	let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build()?;
	let left: Vec<_> = (0_u32..33)
		.map(|i| Complex64::new(f64::from(i), f64::from(i).cos()))
		.collect();
	let right: Vec<_> = (0_u32..17)
		.map(|i| Complex64::new(f64::from(i).sin(), -0.125))
		.collect();
	let mut sequential = ConvolutionWorkspace::new(33, 17, FftBackend::Scalar, Limits::default())?;
	let expected = sequential.convolve(&left, &right)?;
	let mut parallel = ConvolutionWorkspace::new(33, 17, FftBackend::Scalar, Limits::default())?;
	let actual = parallel.convolve_with_policy(&left, &right, ExecutionPolicy::Rayon(&pool))?;
	for (actual, expected) in actual.iter().zip(expected) {
		expect_that!(actual.re.to_bits(), eq(expected.re.to_bits()));
		expect_that!(actual.im.to_bits(), eq(expected.im.to_bits()));
	}
	let bad = [
		Complex64::new(1.0, 0.0),
		Complex64::new(f64::NAN, 0.0),
		Complex64::new(f64::INFINITY, 0.0),
	];
	expect_that!(
		parallel.convolve_with_policy(&bad, &bad, ExecutionPolicy::Rayon(&pool)),
		err(eq(&Error::NonFinite { index: 1 }))
	);
	Ok(())
}

#[gtest]
fn power_of_two_fft_admits_logarithmic_work_budget_and_keeps_arbitrary_length_accounting()
-> Result<()> {
	let limits = Limits {
		shapes: (Limits::default()).shapes,
		resources: quest_numerics::ResourceLimits {
			max_work_units: 4_000_000,
			..(Limits::default()).resources
		},
	};
	let mut fft = FftWorkspace::new(32_768, FftBackend::Scalar, limits)?;
	expect_that!(
		fft.resource_usage().work_units,
		le(limits.resources.max_work_units)
	);
	let mut impulse = vec![Complex64::new(0.0, 0.0); 32_768];
	*impulse.first_mut().ok_or(Error::Length("impulse"))? = Complex64::new(1.0, 0.0);
	fft.transform(&mut impulse, FftDirection::Forward, Normalization::None)?;
	for sample in impulse.iter().step_by(1024) {
		close(*sample, Complex64::new(1.0, 0.0));
	}
	expect_true!(
		FftWorkspace::new(
			17,
			FftBackend::Scalar,
			Limits {
				shapes: (Limits::default()).shapes,
				resources: quest_numerics::ResourceLimits {
					max_work_units: 288,
					..(Limits::default()).resources
				}
			}
		)
		.is_err()
	);
	let prime = FftWorkspace::new(
		17,
		FftBackend::Scalar,
		Limits {
			shapes: (Limits::default()).shapes,
			resources: quest_numerics::ResourceLimits {
				max_work_units: 289,
				..(Limits::default()).resources
			},
		},
	)?;
	expect_that!(prime.resource_usage().work_units, eq(289));
	let convolution = ConvolutionWorkspace::new(
		16_384,
		16_384,
		FftBackend::Scalar,
		Limits {
			shapes: (Limits::default()).shapes,
			resources: quest_numerics::ResourceLimits {
				max_work_units: 12_000_000,
				..(Limits::default()).resources
			},
		},
	)?;
	expect_that!(convolution.fft_len(), eq(32_768));
	expect_that!(convolution.resource_usage().work_units, le(12_000_000));
	Ok(())
}
#[cfg(feature = "rayon")]
#[gtest]
fn prepared_parallel_fft_pair_preserves_bits_and_left_first_input_errors() -> Result<()> {
	let left: Vec<_> = (0_i32..1025)
		.map(|k| Complex64::new(f64::from(k) / 64.0, f64::from(k.rem_euclid(3)) / 128.0))
		.collect();
	let right: Vec<_> = (0_i32..769)
		.map(|k| Complex64::new(f64::from(k) / 128.0, -f64::from(k) / 256.0))
		.collect();
	let mut serial = ConvolutionWorkspace::new(
		left.len(),
		right.len(),
		FftBackend::Scalar,
		Limits::default(),
	)?;
	let expected = serial.convolve(&left, &right)?.to_vec();
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		let policy = quest_numerics::ExecutionPolicy::Rayon(&pool);
		let mut parallel = ConvolutionWorkspace::new_with_policy(
			left.len(),
			right.len(),
			FftBackend::Scalar,
			Limits::default(),
			policy,
		)?;
		let usage = parallel.resource_usage();
		expect_that!(usage.work_units, eq(serial.resource_usage().work_units));
		expect_that!(usage.buffer_bytes, ge(serial.resource_usage().buffer_bytes));
		let exact_bytes = usage
			.buffer_bytes
			.checked_add(usage.planner_bytes_estimate)
			.ok_or_else(|| std::io::Error::other("fixture size overflow"))?;
		expect_true!(
			ConvolutionWorkspace::new_with_policy(
				left.len(),
				right.len(),
				FftBackend::Scalar,
				Limits {
					shapes: (Limits::default()).shapes,
					resources: quest_numerics::ResourceLimits {
						max_peak_bytes: exact_bytes.saturating_sub(1),
						..(Limits::default()).resources
					}
				},
				policy
			)
			.is_err()
		);
		let actual = parallel.convolve_with_policy(&left, &right, policy)?;
		for (a, b) in actual.iter().zip(&expected) {
			expect_that!(a.re.to_bits(), eq(b.re.to_bits()));
			expect_that!(a.im.to_bits(), eq(b.im.to_bits()));
		}
		let mut bad_left = left.clone();
		let mut bad_right = right.clone();
		*bad_left
			.get_mut(3)
			.ok_or_else(|| std::io::Error::other("fixture"))? = Complex64::new(f64::NAN, 0.0);
		*bad_right
			.first_mut()
			.ok_or_else(|| std::io::Error::other("fixture"))? = Complex64::new(f64::NAN, 0.0);
		expect_true!(matches!(
			parallel.convolve_with_policy(&bad_left, &bad_right, policy),
			Err(quest_numerics::Error::NonFinite { index: 3 })
		));
	}
	Ok(())
}
