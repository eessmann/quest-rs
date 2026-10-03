use googletest::prelude::*;
use quest_numerics::{
	Complex64, ConvolutionWorkspace, Error, ExecutionPolicy, FftBackend, Limits,
	SharedConvolutionWorkspace,
};

fn same_bits(actual: &[Complex64], expected: &[Complex64]) {
	expect_that!(actual.len(), eq(expected.len()));
	for (a, b) in actual.iter().zip(expected) {
		expect_that!(a.re.to_bits(), eq(b.re.to_bits()));
		expect_that!(a.im.to_bits(), eq(b.im.to_bits()));
	}
}

fn schedule(backend: FftBackend, execution: ExecutionPolicy<'_>) -> Result<()> {
	let left: Vec<_> = (0_i32..1025)
		.map(|i| Complex64::new(f64::from(i) / 64.0, f64::from(i % 7) / 128.0))
		.collect();
	let right0: Vec<_> = (0_i32..769)
		.map(|i| Complex64::new(f64::from(i) / 128.0, -f64::from(i) / 256.0))
		.collect();
	let right1: Vec<_> = (0_i32..513)
		.map(|i| Complex64::new(-f64::from(i % 11) / 32.0, f64::from(i) / 64.0))
		.collect();
	let mut ordinary =
		ConvolutionWorkspace::new_with_policy(1025, 1025, backend, Limits::default(), execution)?;
	let mut shared = SharedConvolutionWorkspace::new_with_policy(
		1025,
		1025,
		backend,
		Limits::default(),
		execution,
	)?;
	expect_that!(shared.fft_len(), eq(ordinary.fft_len()));
	let mut session = shared.session([&right0, &right1], execution);
	let mut pointer = None;
	for (step, index) in [0, 1, 1, 0].into_iter().enumerate() {
		let input = if step >= 2 {
			left.get(..257).ok_or(Error::Length("fixture prefix"))?
		} else {
			&left
		};
		let right = if index == 0 { &right0 } else { &right1 };
		let expected = ordinary.convolve_with_policy(input, right, execution)?;
		let actual = session.product(input, index)?;
		same_bits(actual, expected);
		expect_true!(pointer.is_none_or(|previous| previous == actual.as_ptr()));
		pointer = Some(actual.as_ptr());
	}
	Ok(())
}

#[gtest]
fn shared_schedule_preserves_scalar_bits_and_output_storage() -> Result<()> {
	schedule(FftBackend::Scalar, ExecutionPolicy::Sequential)
}

#[cfg(not(feature = "simd"))]
#[gtest]
fn shared_simd_request_without_compiled_backend_is_rejected() {
	expect_that!(
		SharedConvolutionWorkspace::new(4, 4, FftBackend::Simd, Limits::default()),
		err(eq(&Error::BackendUnavailable))
	);
}

#[cfg(feature = "rayon")]
#[gtest]
fn shared_schedule_preserves_bits_in_one_two_and_four_worker_pools() -> Result<()> {
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		schedule(FftBackend::Scalar, ExecutionPolicy::Rayon(&pool))?;
	}
	Ok(())
}

#[cfg(feature = "simd")]
#[gtest]
fn shared_schedule_preserves_available_simd_bits() -> Result<()> {
	match SharedConvolutionWorkspace::new(2, 2, FftBackend::Simd, Limits::default()) {
		Err(Error::BackendUnavailable) => return Ok(()),
		result => {
			result?;
		}
	}
	schedule(FftBackend::Simd, ExecutionPolicy::Sequential)?;
	#[cfg(feature = "rayon")]
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		schedule(FftBackend::Simd, ExecutionPolicy::Rayon(&pool))?;
	}
	Ok(())
}

#[gtest]
fn shared_sessions_reset_spectra_and_zero_padding_for_shorter_inputs() -> Result<()> {
	let long = [Complex64::new(2.0, 1.0); 4];
	let one = [Complex64::new(3.0, -1.0)];
	let other = [Complex64::new(-2.0, 0.5)];
	let mut shared = SharedConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	let mut ordinary = ConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	{
		let mut session = shared.session([&long, &long], ExecutionPolicy::Sequential);
		same_bits(session.product(&long, 0)?, ordinary.convolve(&long, &long)?);
		same_bits(session.product(&long, 1)?, ordinary.convolve(&long, &long)?);
	}
	let mut session = shared.session([&one, &other], ExecutionPolicy::Sequential);
	expect_that!(session.work_for(0)?, eq(584));
	same_bits(session.product(&one, 0)?, ordinary.convolve(&one, &one)?);
	same_bits(session.product(&one, 1)?, ordinary.convolve(&one, &other)?);
	Ok(())
}

#[gtest]
fn shared_work_tracks_lazy_rhs_and_invalidates_cached_state_on_errors() -> Result<()> {
	let good = [Complex64::new(2.0, 0.0)];
	let mut shared = SharedConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	let mut session = shared.session([&good, &good], ExecutionPolicy::Sequential);
	expect_that!(session.work_for(0)?, eq(584));
	expect_that!(session.work_for(1)?, eq(584));
	session.product(&good, 0)?;
	expect_that!(session.work_for(0)?, eq(392));
	expect_that!(session.work_for(1)?, eq(584));
	session.product(&good, 1)?;
	expect_that!(session.work_for(1)?, eq(392));
	expect_true!(session.work_for(2).is_err());
	expect_true!(session.product(&good, 2).is_err());
	expect_that!(session.work_for(0)?, eq(584));
	expect_that!(session.work_for(1)?, eq(584));
	session.product(&good, 0)?;
	expect_true!(
		session
			.product(&[Complex64::new(f64::MAX, 0.0)], 0)
			.is_err()
	);
	expect_that!(session.work_for(0)?, eq(584));
	same_bits(session.product(&good, 0)?, &[Complex64::new(4.0, 0.0)]);
	Ok(())
}

#[gtest]
fn shared_products_preserve_shape_and_left_first_finite_error_order() -> Result<()> {
	let good = [Complex64::new(1.0, 0.0)];
	let bad_left = [Complex64::new(1.0, 0.0), Complex64::new(f64::NAN, 0.0)];
	let bad_right = [Complex64::new(f64::INFINITY, 0.0)];
	let mut shared = SharedConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	{
		let mut session = shared.session([&bad_right, &good], ExecutionPolicy::Sequential);
		expect_that!(
			session.product(&bad_left, 0),
			err(eq(&Error::NonFinite { index: 1 }))
		);
		expect_that!(
			session.product(&good, 0),
			err(eq(&Error::NonFinite { index: 0 }))
		);
		// The invalid unused RHS is never transformed or rejected.
		session.product(&good, 1)?;
	}
	for right in [&[][..], &[Complex64::new(0.0, 0.0); 5][..]] {
		let mut session = shared.session([right, &good], ExecutionPolicy::Sequential);
		expect_true!(matches!(
			session.product(&bad_left, 0),
			Err(Error::Length(_))
		));
	}
	let mut session = shared.session([&good, &good], ExecutionPolicy::Sequential);
	expect_true!(session.product(&[], 0).is_err());
	expect_true!(session.product(&[Complex64::new(0.0, 0.0); 5], 0).is_err());
	Ok(())
}

#[gtest]
fn shared_forward_errors_scan_left_before_rhs_and_do_not_mark_ready() -> Result<()> {
	let good = [Complex64::new(1.0, 0.0)];
	let huge = [Complex64::new(f64::MAX, 0.0); 4];
	let mut shared = SharedConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	let mut ordinary = ConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	let mut session = shared.session([&huge, &good], ExecutionPolicy::Sequential);
	let expected = ordinary.convolve(&huge, &huge).map(|_| ());
	expect_that!(session.product(&huge, 0).map(|_| ()), eq(&expected));
	expect_that!(session.work_for(0)?, eq(584));
	expect_true!(session.product(&good, 0).is_err());
	expect_that!(session.work_for(0)?, eq(584));
	session.product(&good, 1)?;
	Ok(())
}

#[gtest]
fn shared_admission_accounts_three_arrays_and_cold_product_work() -> Result<()> {
	let shared = SharedConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	let ordinary = ConvolutionWorkspace::new(4, 4, FftBackend::Scalar, Limits::default())?;
	let usage = shared.resource_usage();
	expect_that!(usage.work_units, eq(584));
	expect_that!(
		usage.buffer_bytes,
		eq(ordinary
			.resource_usage()
			.buffer_bytes
			.checked_add(128)
			.ok_or(Error::Overflow)?)
	);
	expect_that!(
		usage.planner_bytes_estimate,
		eq(ordinary.resource_usage().planner_bytes_estimate)
	);
	let exact = Limits {
		max_len: 8,
		max_bytes: usage
			.buffer_bytes
			.checked_add(usage.planner_bytes_estimate)
			.ok_or(Error::Overflow)?,
		max_work: 584,
	};
	expect_true!(SharedConvolutionWorkspace::new(4, 4, FftBackend::Scalar, exact).is_ok());
	for limits in [
		Limits {
			max_bytes: exact.max_bytes.checked_sub(1).ok_or(Error::Overflow)?,
			..exact
		},
		Limits {
			max_work: 583,
			..exact
		},
		Limits {
			max_len: 7,
			..exact
		},
	] {
		expect_true!(SharedConvolutionWorkspace::new(4, 4, FftBackend::Scalar, limits).is_err());
	}
	expect_true!(SharedConvolutionWorkspace::new(0, 4, FftBackend::Scalar, exact).is_err());
	expect_true!(
		SharedConvolutionWorkspace::new(usize::MAX, 4, FftBackend::Scalar, Limits::default())
			.is_err()
	);
	let mut singleton =
		SharedConvolutionWorkspace::new(1, 1, FftBackend::Scalar, Limits::default())?;
	expect_that!(singleton.resource_usage().work_units, eq(25));
	let one = [Complex64::new(1.0, 0.0)];
	let mut session = singleton.session([&one, &one], ExecutionPolicy::Sequential);
	session.product(&one, 0)?;
	expect_that!(session.work_for(0)?, eq(17));
	Ok(())
}

#[cfg(feature = "rayon")]
#[gtest]
fn shared_parallel_admission_matches_owned_scratch_and_exact_budget() -> Result<()> {
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		let execution = ExecutionPolicy::Rayon(&pool);
		let shared = SharedConvolutionWorkspace::new_with_policy(
			1025,
			1025,
			FftBackend::Scalar,
			Limits::default(),
			execution,
		)?;
		let ordinary = ConvolutionWorkspace::new_with_policy(
			1025,
			1025,
			FftBackend::Scalar,
			Limits::default(),
			execution,
		)?;
		let usage = shared.resource_usage();
		expect_that!(
			usage.buffer_bytes,
			eq(ordinary
				.resource_usage()
				.buffer_bytes
				.checked_add(
					shared
						.fft_len()
						.checked_mul(size_of::<Complex64>())
						.ok_or(Error::Overflow)?
				)
				.ok_or(Error::Overflow)?)
		);
		let exact = Limits {
			max_len: shared.fft_len(),
			max_bytes: usage
				.buffer_bytes
				.checked_add(usage.planner_bytes_estimate)
				.ok_or(Error::Overflow)?,
			max_work: usage.work_units,
		};
		expect_true!(
			SharedConvolutionWorkspace::new_with_policy(
				1025,
				1025,
				FftBackend::Scalar,
				exact,
				execution
			)
			.is_ok()
		);
		expect_true!(
			SharedConvolutionWorkspace::new_with_policy(
				1025,
				1025,
				FftBackend::Scalar,
				Limits {
					max_bytes: exact.max_bytes.checked_sub(1).ok_or(Error::Overflow)?,
					..exact
				},
				execution
			)
			.is_err()
		);
	}
	Ok(())
}

#[cfg(feature = "rayon")]
#[gtest]
fn shared_parallel_errors_preserve_order_and_allow_recovery() -> Result<()> {
	let good = vec![Complex64::new(0.01, -0.02); 1025];
	let huge = vec![Complex64::new(f64::MAX, 0.0); 1025];
	let mut bad_left = good.clone();
	*bad_left.get_mut(3).ok_or(Error::Length("fixture left"))? = Complex64::new(f64::NAN, 0.0);
	let mut bad_right = good.clone();
	*bad_right
		.first_mut()
		.ok_or(Error::Length("fixture right"))? = Complex64::new(f64::NAN, 0.0);
	for workers in [1, 2, 4] {
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		let execution = ExecutionPolicy::Rayon(&pool);
		let mut shared = SharedConvolutionWorkspace::new_with_policy(
			1025,
			1025,
			FftBackend::Scalar,
			Limits::default(),
			execution,
		)?;
		let mut ordinary = ConvolutionWorkspace::new_with_policy(
			1025,
			1025,
			FftBackend::Scalar,
			Limits::default(),
			execution,
		)?;
		{
			let mut session = shared.session([&bad_right, &good], execution);
			expect_that!(
				session.product(&bad_left, 0),
				err(eq(&Error::NonFinite { index: 3 }))
			);
			same_bits(
				session.product(&good, 1)?,
				ordinary.convolve_with_policy(&good, &good, execution)?,
			);
		}
		{
			let mut session = shared.session([&huge, &good], execution);
			let expected = ordinary
				.convolve_with_policy(&huge, &huge, execution)
				.map(|_| ());
			expect_true!(expected.is_err());
			expect_that!(session.product(&huge, 0).map(|_| ()), eq(&expected));
			// L=4096, T=8*4096*12: three transforms plus L products.
			expect_that!(session.work_for(0)?, eq(1_183_744));
			same_bits(
				session.product(&good, 1)?,
				ordinary.convolve_with_policy(&good, &good, execution)?,
			);
		}
		// A different policy can use prepared storage after the original pool
		// borrow has ended; no pool ownership belongs to the workspace.
		drop(pool);
		let mut session = shared.session([&good, &good], ExecutionPolicy::Sequential);
		same_bits(session.product(&good, 0)?, ordinary.convolve(&good, &good)?);
	}
	Ok(())
}
