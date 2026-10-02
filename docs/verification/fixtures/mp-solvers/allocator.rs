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
pub fn sample(
    count: usize,
    mut run: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(u128, usize, usize), Box<dyn std::error::Error>> {
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let allocations = ALLOCS.load(Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..count {
        run()?;
    }
    Ok((
        start.elapsed().as_nanos(),
        ALLOCS.load(Ordering::Relaxed) - allocations,
        PEAK.load(Ordering::Relaxed).saturating_sub(base),
    ))
}
