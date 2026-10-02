use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};
struct Counter;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static PHASE_ACTIVE: AtomicBool = AtomicBool::new(false);
static PHASE_BASE: AtomicUsize = AtomicUsize::new(0);
static PHASE_START_ALLOCS: AtomicUsize = AtomicUsize::new(0);
static PHASE_PEAK: AtomicUsize = AtomicUsize::new(0);
static PHASE_ALLOCS: AtomicUsize = AtomicUsize::new(0);
fn observe_peak(live: usize) {
    PEAK.fetch_max(live, Ordering::Relaxed);
    if PHASE_ACTIVE.load(Ordering::Relaxed) {
        PHASE_PEAK.fetch_max(live, Ordering::Relaxed);
    }
}
/// One sequential completion phase nested inside the outer sample.
pub fn phase_begin() {
    assert!(!PHASE_ACTIVE.load(Ordering::Relaxed));
    let base = LIVE.load(Ordering::Relaxed);
    PHASE_BASE.store(base, Ordering::Relaxed);
    PHASE_PEAK.store(base, Ordering::Relaxed);
    PHASE_START_ALLOCS.store(ALLOCS.load(Ordering::Relaxed), Ordering::Relaxed);
    PHASE_ACTIVE.store(true, Ordering::Relaxed);
}
pub fn phase_end() {
    assert!(PHASE_ACTIVE.swap(false, Ordering::Relaxed));
    PHASE_ALLOCS.store(
        ALLOCS.load(Ordering::Relaxed) - PHASE_START_ALLOCS.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
}
pub fn phase_observation() -> (usize, usize) {
    assert!(!PHASE_ACTIVE.load(Ordering::Relaxed));
    (
        PHASE_ALLOCS.load(Ordering::Relaxed),
        PHASE_PEAK
            .load(Ordering::Relaxed)
            .saturating_sub(PHASE_BASE.load(Ordering::Relaxed)),
    )
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            let n = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            observe_peak(n);
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
            observe_peak(v);
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
