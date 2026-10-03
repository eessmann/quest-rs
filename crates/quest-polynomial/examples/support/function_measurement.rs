//! Small reproducible measurement, separate from correctness tests.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Bounded benchmark loops and expression construction"
)]
use quest_numerics::arithmetic::{F64Backend, Interval64Backend};
use quest_polynomial::{Expression, Function, GenericFunction, Interval};
use std::{
	alloc::{GlobalAlloc, Layout, System},
	hint::black_box,
	sync::atomic::{AtomicBool, AtomicUsize, Ordering},
	time::Instant,
};
struct CountingAllocator;
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
// SAFETY: Allocation and deallocation arguments are delegated unchanged to System.
unsafe impl GlobalAlloc for CountingAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		if TRACK.load(Ordering::Relaxed) {
			ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
		}
		// SAFETY: The caller supplies the valid allocator layout.
		unsafe { System.alloc(layout) }
	}
	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		// SAFETY: The caller supplies the matching pointer and original layout.
		unsafe { System.dealloc(ptr, layout) }
	}
}
fn measure(
	label: &str,
	phase: &str,
	count: usize,
	mut action: impl FnMut() -> quest_polynomial::Result<()>,
) -> quest_polynomial::Result<()> {
	action()?;
	ALLOCATIONS.store(0, Ordering::Relaxed);
	let started = Instant::now();
	TRACK.store(true, Ordering::Relaxed);
	let result: quest_polynomial::Result<()> = (|| {
		for _ in 0..count {
			action()?;
		}
		Ok(())
	})();
	TRACK.store(false, Ordering::Relaxed);
	let elapsed = started.elapsed().as_nanos();
	result?;
	println!(
		"{label},{phase},{count},{elapsed},{}",
		ALLOCATIONS.load(Ordering::Relaxed)
	);
	Ok(())
}
pub fn run<E: Expression>(
	label: &str,
	create: impl Fn() -> Function<E>,
) -> Result<(), Box<dyn std::error::Error>> {
	println!("representation,phase,iterations,nanoseconds,allocations");
	let function = black_box(create());
	measure(label, "construct", 10_000, || {
		black_box(create());
		Ok(())
	})?;
	measure(label, "value", 1_000_000, || {
		black_box(function.evaluate(&mut F64Backend, black_box(0.3))?);
		Ok(())
	})?;
	measure(label, "jet", 1_000_000, || {
		black_box(function.jet(&mut F64Backend, black_box(0.3))?);
		Ok(())
	})?;
	let domain = Interval::new(0.2, 0.3)?;
	measure(label, "interval_jet", 10_000, || {
		black_box(function.jet(&mut Interval64Backend, black_box(domain))?);
		Ok(())
	})?;
	Ok(())
}
