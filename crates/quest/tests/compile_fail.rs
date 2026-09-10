#[test]
fn runtime_ownership_and_kind_contracts() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/*.rs");
}
