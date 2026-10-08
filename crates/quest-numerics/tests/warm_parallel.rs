#![cfg(feature = "rayon")]
use googletest::prelude::*;
use quest_numerics::{Complex64, ConvolutionWorkspace, ExecutionPolicy, FftBackend, Limits};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use std::alloc::System;

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[gtest]
fn warmed_pool_fft_pairs_reuse_storage_and_measure_scheduler_allocations() -> Result<()> {
	for workers in [1, 4] {
		let calls = if workers == 1 { 1600 } else { 256 };
		let pool = rayon::ThreadPoolBuilder::new()
			.num_threads(workers)
			.build()?;
		let execution = ExecutionPolicy::Rayon(&pool);
		let left = vec![Complex64::new(0.01, 0.02); 1025];
		let right = vec![Complex64::new(0.03, -0.01); 769];
		let mut workspace = ConvolutionWorkspace::new_with_policy(
			left.len(),
			right.len(),
			FftBackend::Scalar,
			Limits::default(),
			execution,
		)?;
		// Exclude external job injection. Multithreaded Rayon can still lazily
		// allocate OS sleep primitives under contention; warmup cannot promise
		// that every scheduling path has occurred. Keep those measurements
		// separate from the strict single-worker kernel allocation contract.
		let (result, allocations) = pool.install(|| {
			for _ in 0..4 {
				workspace.convolve_with_policy(&left, &right, execution)?;
			}
			let region = Region::new(ALLOCATOR);
			let result = (|| -> quest_numerics::Result<_> {
				let mut first = Complex64::new(0.0, 0.0);
				let mut length = 0;
				let mut pointer = None;
				let mut reused = true;
				for _ in 0..calls {
					let output = workspace.convolve_with_policy(&left, &right, execution)?;
					first = output
						.first()
						.copied()
						.ok_or(quest_numerics::Error::Length("empty output"))?;
					length = output.len();
					reused &= pointer.is_none_or(|previous| previous == output.as_ptr());
					pointer = Some(output.as_ptr());
					std::hint::black_box(output);
				}
				Ok((first, length, reused))
			})();
			let stats = region.change();
			Ok::<_, quest_numerics::Error>((
				result,
				stats.allocations.saturating_add(stats.reallocations),
			))
		})?;
		let (first, length, reused) = result?;
		expect_that!(first.re, near(0.0005, 1e-14));
		expect_that!(first.im, near(0.0005, 1e-14));
		expect_that!(length, eq(1793));
		expect_true!(reused);
		if workers == 1 {
			expect_that!(allocations, eq(0));
		}
		eprintln!(
			"warmed in-pool convolutions: workers={workers}, calls={calls}, allocations={allocations}"
		);
	}
	Ok(())
}
