//! A small complete cover must not allocate the configured worst-case queue.
use quest_numerics::Interval;
use quest_numerics::arithmetic::{First, Interval64Backend};
use quest_numerics::roots::{CoverLimits, Premise, cover};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct CountingAllocator;
static TRACK: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
// SAFETY: Each request is delegated unchanged to the system allocator.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: Caller supplied the allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Caller supplied the original allocation and matching layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        // SAFETY: Caller supplied the allocation, original layout and new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[test]
fn continuum_cover_allocates_only_the_used_queue() {
    let input = Interval::new(-1.0, 1.0).unwrap();
    TRACK.store(true, Ordering::Relaxed);
    let report = cover(
        &mut Interval64Backend,
        input,
        |_, _| {
            Ok(First {
                value: Interval::point(0.0)?,
                first: Interval::point(0.0)?,
            })
        },
        CoverLimits::default(),
        &1e-10,
        Premise::EnclosesContinuouslyDifferentiableFunction,
    );
    TRACK.store(false, Ordering::Relaxed);
    let report = report.unwrap();
    assert!(report.complete());
    assert!(report.covered[0].continuum);
    assert!(
        LARGEST.load(Ordering::Relaxed) <= 1024,
        "a one-box cover must not reserve its 10000-box limit"
    );
}
