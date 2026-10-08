//! Darwin exact-dylib ABI fixture for workspace native-link tests.
unsafe extern "C" {
    fn native_value() -> i32;
}
fn main() {
    // SAFETY: The fixture dylib defines this exact C ABI and has no pointer
    // arguments or initialization preconditions.
    assert_eq!(unsafe { native_value() }, 73);
}
