#[test]
fn runtime_ownership_and_kind_contracts() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/*.rs");
}

#[test]
fn structured_preparation_is_not_send() {
    // A non-Send type has only the () implementation, so rustc can infer A.
    // If PreparedProgram becomes Send, both implementations apply and this
    // assertion fails to compile due to ambiguity. Unlike a diagnostic snapshot,
    // this checks the trait contract without depending on standard-library text.
    trait AmbiguousIfSend<A> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfSend<()> for T {}
    struct ImplementsSend;
    impl<T: ?Sized + Send> AmbiguousIfSend<ImplementsSend> for T {}

    let _ = <quest::PreparedProgram<'static> as AmbiguousIfSend<_>>::check;
}
