#![cfg(feature = "macros")]
#[allow(unused_imports)]
use quest_compile::prelude::*;

#[test]
fn invalid_dsl_and_semantic_capabilities_are_rejected() {
	let cases = trybuild::TestCases::new();
	cases.pass("tests/ui-pass/*.rs");
	cases.compile_fail("tests/ui/*.rs");
}
