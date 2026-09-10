#![cfg(feature = "macros")]

#[test]
fn invalid_dsl_and_semantic_capabilities_are_rejected() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
