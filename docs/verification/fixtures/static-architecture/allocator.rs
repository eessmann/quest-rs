use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Counter;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            let n = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(n, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_sub(l.size(), Ordering::Relaxed);
            let v = LIVE.fetch_add(n, Ordering::Relaxed) + n;
            PEAK.fetch_max(v, Ordering::Relaxed);
        }
        q
    }
}
#[global_allocator]
static COUNTER: Counter = Counter;
fn measure(
    name: &str,
    n: usize,
    mut f: impl FnMut() -> Result<usize, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = f()?;
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let a = ALLOCS.load(Ordering::Relaxed);
    let t = Instant::now();
    for _ in 0..n {
        assert_eq!(f()?, output);
    }
    let ns = t.elapsed().as_nanos();
    let count = ALLOCS.load(Ordering::Relaxed) - a;
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    println!("{name},{n},{ns},{count},{peak},{output}");
    Ok(())
}
fn header() {
    println!("workload,iterations,nanoseconds,allocations,peak_live_extra_bytes,output_scalars");
}
