use googletest::prelude::*;
use quest_polynomial::{Chebyshev, Complex64, Interval, Limits, Polynomial, function};
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
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Operators construct an expression before allocator tracking starts"
)]
fn warmed_scalar_interval_and_derivative_evaluation_allocate_nothing() -> Result<()> {
    let function = function!(|x| (x.clone() * x + 1.0).ln());
    let polynomial = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.3, 0.0); 32],
        Limits::default(),
    )?;
    let interval = Interval::new(0.2, 0.3)?;
    let execute = || -> quest_polynomial::Result<()> {
        for _ in 0..32 {
            std::hint::black_box(function.evaluate(0.3)?);
            std::hint::black_box(function.jet(0.3)?);
            std::hint::black_box(function.evaluate_interval(interval)?);
            std::hint::black_box(function.jet_interval(interval)?);
            std::hint::black_box(polynomial.evaluate_real(0.3)?);
            std::hint::black_box(polynomial.jet_interval(interval)?);
        }
        Ok(())
    };
    execute()?;
    ALLOCATIONS.store(0, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let result = execute();
    TRACK.store(false, Ordering::SeqCst);
    result?;
    expect_that!(ALLOCATIONS.load(Ordering::SeqCst), eq(0));
    Ok(())
}
