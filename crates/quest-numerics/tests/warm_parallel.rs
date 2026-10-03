#![cfg(feature = "rayon")]
use googletest::prelude::*;
use quest_numerics::{Complex64, ConvolutionWorkspace, ExecutionPolicy, FftBackend, Limits};
use std::{
	alloc::{GlobalAlloc, Layout, System},
	sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
struct CountingAllocator;
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
// SAFETY: Every allocation and deallocation is delegated unchanged to System.
unsafe impl GlobalAlloc for CountingAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		if TRACK.load(Ordering::Relaxed) {
			ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
		}
		// SAFETY: The caller supplied the allocator contract's valid layout.
		unsafe { System.alloc(layout) }
	}
	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		// SAFETY: The caller supplies the pointer and matching original layout.
		unsafe { System.dealloc(ptr, layout) }
	}
}
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
			ALLOCATIONS.store(0, Ordering::SeqCst);
			TRACK.store(true, Ordering::SeqCst);
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
			TRACK.store(false, Ordering::SeqCst);
			Ok::<_, quest_numerics::Error>((result, ALLOCATIONS.load(Ordering::SeqCst)))
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
