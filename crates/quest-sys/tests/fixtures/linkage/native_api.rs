//! ABI fixture consumed by the workspace native-link regression tests.
//!
//! This tiny FFI boundary stays beside the audited quest-sys bindings.
unsafe extern "C" {
    fn fixture_value() -> i32;
}
pub fn value() -> i32 {
    // SAFETY: The regression compiler links the C fixture defining this exact
    // no-argument integer-returning function; it has no pointer preconditions.
    unsafe { fixture_value() }
}
