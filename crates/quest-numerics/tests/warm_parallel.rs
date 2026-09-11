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
fn warmed_caller_pool_fft_pairs_allocate_nothing() -> Result<()> {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build()?;
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
    for _ in 0..4 {
        std::hint::black_box(workspace.convolve_with_policy(&left, &right, execution)?);
    }
    ALLOCATIONS.store(0, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let result = (|| -> quest_numerics::Result<()> {
        for _ in 0..16 {
            std::hint::black_box(workspace.convolve_with_policy(&left, &right, execution)?);
        }
        Ok(())
    })();
    TRACK.store(false, Ordering::SeqCst);
    result?;
    expect_that!(ALLOCATIONS.load(Ordering::SeqCst), eq(0));
    Ok(())
}
