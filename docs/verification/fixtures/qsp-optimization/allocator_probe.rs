//! Standalone nested-stage allocation regression; run with rustc, without Cargo.
#[path = "allocator.rs"]
mod allocator;
use std::alloc::{Layout, alloc, dealloc, realloc};
use std::hint::black_box;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_, calls, peak) = allocator::sample(1, || {
        // Direct allocator calls avoid optimizer removal of unused collections.
        unsafe {
            let outer_layout = Layout::from_size_align(1024, 8)?;
            let outer = black_box(alloc(outer_layout));
            assert!(!outer.is_null());
            allocator::phase_begin();
            let layout = Layout::from_size_align(2048, 8)?;
            let inner = black_box(alloc(layout));
            assert!(!inner.is_null());
            let inner = black_box(realloc(inner, layout, 4096));
            assert!(!inner.is_null());
            dealloc(inner, Layout::from_size_align(4096, 8)?);
            allocator::phase_end();
            assert_eq!(allocator::phase_observation(), (2, 4096));
            dealloc(outer, outer_layout);
        }
        Ok(())
    })?;
    assert_eq!((calls, peak), (3, 5120));
    allocator::phase_begin();
    unsafe {
        let layout = Layout::from_size_align(512, 8)?;
        let value = black_box(alloc(layout));
        assert!(!value.is_null());
        allocator::phase_end();
        assert_eq!(allocator::phase_observation(), (1, 512));
        dealloc(value, layout);
    }
    println!("nested allocation accounting passed");
    Ok(())
}
