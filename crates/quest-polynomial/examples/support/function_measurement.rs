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
) -> Result<(), Box<dyn std::error::Error>> {
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
	let allocations = ALLOCATIONS.load(Ordering::Relaxed);
	write_measurement(
		std::io::stdout().lock(),
		label,
		phase,
		count,
		elapsed,
		allocations,
	)?;
	Ok(())
}
pub fn run<E: Expression>(
	label: &str,
	create: impl Fn() -> Function<E>,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut output = csv::WriterBuilder::new()
		.terminator(csv::Terminator::Any(b'\n'))
		.from_writer(std::io::stdout().lock());
	output.write_record([
		"representation",
		"phase",
		"iterations",
		"nanoseconds",
		"allocations",
	])?;
	output.flush()?;
	drop(output);
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

fn write_measurement(
	output: impl std::io::Write,
	label: &str,
	phase: &str,
	count: usize,
	elapsed: u128,
	allocations: usize,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut writer = csv::WriterBuilder::new()
		.terminator(csv::Terminator::Any(b'\n'))
		.from_writer(output);
	writer.serialize((label, phase, count, elapsed, allocations))?;
	writer.flush()?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::write_measurement;
	use googletest::prelude::*;

	#[gtest]
	fn labels_and_full_width_counters_round_trip() -> googletest::Result<()> {
		let label = "quoted \"label\", with a newline\n";
		let mut bytes = Vec::new();
		write_measurement(&mut bytes, label, "value", 1, u128::MAX, 7).or_fail()?;
		expect_that!(bytes.last(), some(eq(&b'\n')));
		let mut reader = csv::ReaderBuilder::new()
			.has_headers(false)
			.from_reader(bytes.as_slice());
		let record = reader.records().next().or_fail()?.or_fail()?;
		expect_that!(record.get(0), some(eq(label)));
		verify_that!(record.get(3), some(eq(u128::MAX.to_string().as_str())))
	}
}
