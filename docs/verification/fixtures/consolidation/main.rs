//! Standalone allocation/capacity probe. Not a workspace crate or production allocator.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::Instant,
};

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
fn added(bytes: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(bytes, Relaxed);
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            added(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe {
            System.dealloc(pointer, layout);
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, old, size) };
        if !result.is_null() {
            LIVE.fetch_sub(old.size(), Relaxed);
            added(size);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
type Error = Box<dyn std::error::Error>;
fn measure<T>(case: &str, scale: usize, action: impl FnOnce() -> Result<T, Error>) {
    let baseline = LIVE.load(Relaxed);
    PEAK.store(baseline, Relaxed);
    ALLOCATIONS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    let start = Instant::now();
    let result = action();
    let nanos = start.elapsed().as_nanos();
    let allocations = ALLOCATIONS.load(Relaxed);
    let requested = REQUESTED.load(Relaxed);
    let retained = LIVE.load(Relaxed).saturating_sub(baseline);
    let peak = PEAK.load(Relaxed).saturating_sub(baseline);
    println!(
        "{{\"case\":\"{case}\",\"scale\":{scale},\"ok\":{},\"allocations\":{allocations},\"requested_bytes\":{requested},\"retained_bytes\":{retained},\"peak_bytes\":{peak},\"elapsed_ns\":{nanos}}}",
        result.is_ok()
    );
    if let Err(error) = &result {
        eprintln!("{case}({scale}): {error}");
    }
    drop(std::hint::black_box(result));
}
fn main() {
    // Initialize standard output before measuring its first use.
    println!(
        "{{\"measurement\":\"requested allocator bytes, excludes allocator metadata and RSS\"}}"
    );
    for count in [1024, 2048, 4096] {
        measure("exact_merge_chain", count, || {
            let mut builder = quest_circuit::ProgramBuilder::new(1, 0)?;
            let qubit = builder.qubit(0)?;
            for _ in 0..count {
                builder.gate(
                    quest_circuit::Gate::Rz(quest_circuit::Angle::pi(1, 7)?),
                    &[qubit],
                    &[],
                )?;
            }
            let result = builder.finish()?.optimize_exact()?;
            assert_eq!(result.0.schedule().len(), 1);
            Ok(result)
        });
    }
    for depth in [10, 14] {
        measure("shared_expression_construction", depth, || {
            let builder = quest_language::semantic::builder::Builder::new()?;
            let mut value = builder.floating::<64>(0.125)?;
            for _ in 0..depth {
                value = value.add(&value)?;
            }
            Ok((builder, value))
        });
    }
    for branches in [30, 150] {
        measure("branch_heavy_ssa_admission", branches, || {
            use quest_language::{
                SourceId, SourceSnapshot,
                semantic::{CompileLimits, admit},
                syntax::parse_source,
            };
            let text = format!(
                "input bool flag; qubit q; {}",
                "if(flag){x q;}else{h q;}".repeat(branches)
            );
            let source = SourceSnapshot::new(SourceId::new(1), "measurement", text);
            Ok(admit(parse_source(&source)?, CompileLimits::default())?.into_ssa()?)
        });
    }
}
