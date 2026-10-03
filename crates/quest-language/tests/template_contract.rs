#![cfg(feature = "templates")]
use googletest::prelude::*;
use quest_language::{
	SourceId, SourceSnapshot,
	semantic::{self, CompileLimits, template},
	syntax,
};

#[gtest]
fn template_reuses_checked_graph_and_rejects_mutated_effects() -> Result<()> {
	let source = SourceSnapshot::new(
		SourceId::new(1),
		"template",
		"qubit q; bit c; c = measure q; if (bool(c)) { x q; }",
	);
	let module = semantic::admit(syntax::parse_source(&source)?, CompileLimits::default())?;
	let encoded = template::encode(&module)?;
	let loaded = template::load(&encoded, CompileLimits::default())?;
	expect_eq!(loaded.syntax(), module.syntax());
	let tampered = encoded.replace("\"effect\":\"Observe\"", "\"effect\":\"Pure\"");
	expect_true!(template::load(&tampered, CompileLimits::default()).is_err());
	Ok(())
}

#[gtest]
fn serialized_scalar_cannot_bypass_checked_widths_and_payloads() {
	use quest_language::classical::ScalarValue;
	expect_true!(
		serde_json::from_str::<ScalarValue>(r#"{"ty":{"Int":0},"data":{"Bits":0}}"#).is_err()
	);
	expect_true!(
		serde_json::from_str::<ScalarValue>(r#"{"ty":{"Int":2},"data":{"Bits":7}}"#).is_err()
	);
	expect_true!(
		serde_json::from_str::<ScalarValue>(r#"{"ty":"Bool","data":{"Bits":1}}"#).is_err()
	);
}

#[gtest]
fn previous_template_version_is_rejected_without_legacy_decode() -> Result<()> {
	let source = SourceSnapshot::new(SourceId::new(2), "template", "qubit q; h q;");
	let module = semantic::admit(syntax::parse_source(&source)?, CompileLimits::default())?;
	let encoded = template::encode(&module)?;
	let mut value: serde_json::Value = serde_json::from_str(&encoded)?;
	expect_eq!(value["version"].as_u64(), Some(2));
	value["version"] = 1.into();
	expect_true!(
		template::load(&serde_json::to_string(&value)?, CompileLimits::default()).is_err()
	);
	Ok(())
}
