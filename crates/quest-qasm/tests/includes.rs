use googletest::{Result, prelude::*};
use quest_language::{DiagnosticCause, SourceId, SourceSnapshot};
use quest_qasm::{ImportLimits, IncludeResolver, ResolveError, parse};
use std::collections::BTreeMap;
mod support;
struct Resolver(BTreeMap<String, SourceSnapshot>);
impl IncludeResolver for Resolver {
	fn resolve(
		&mut self,
		_: &SourceSnapshot,
		path: &str,
	) -> std::result::Result<SourceSnapshot, ResolveError> {
		self.0
			.get(path)
			.cloned()
			.ok_or_else(|| ResolveError::new("not supplied"))
	}
}
fn source(id: u64, name: &str, text: &str) -> SourceSnapshot {
	SourceSnapshot::new(SourceId::new(id), name, text)
}
#[gtest]
fn includes_preserve_order_and_own_all_source_snapshots() -> Result<()> {
	let root = source(1, "main.qasm", "include \"a.inc\"; qubit q; local q;");
	let mut resolver = Resolver(BTreeMap::from([
		(
			"a.inc".into(),
			source(2, "a.inc", "include \"b.inc\"; gate local t { base t; }"),
		),
		("b.inc".into(), source(3, "b.inc", "gate base t { x t; }")),
	]));
	let parsed = parse(root, &mut resolver, ImportLimits::default())?;
	drop(resolver);
	expect_eq!(parsed.sources().iter().count(), 3);
	expect_eq!(parsed.includes().len(), 2);
	let expected = quest_language::syntax::parse_source(&source(
		8,
		"expected",
		"gate base t { x t; } gate local t { base t; } qubit q; local q;",
	))?;
	expect_eq!(
		support::normalize(parsed.expanded().clone()),
		support::normalize(expected)
	);
	for edge in parsed.includes() {
		expect_true!(parsed.sources().slice(edge.span).is_ok());
	}
	Ok(())
}
#[gtest]
fn missing_cycle_and_include_budgets_return_owned_diagnostics() -> Result<()> {
	let root = source(1, "main.qasm", "include \"a.inc\";");
	let mut missing = Resolver(BTreeMap::new());
	let error = parse(root.clone(), &mut missing, ImportLimits::default())
		.err()
		.ok_or_else(|| ResolveError::new("expected missing include"))?;
	expect_true!(matches!(
		error.cause,
		DiagnosticCause::IncludeFailure { .. }
	));
	expect_eq!(
		error
			.sources
			.get(SourceId::new(1))
			.map(SourceSnapshot::text),
		Some("include \"a.inc\";")
	);
	expect_true!(error.validate_sources().is_ok());
	let mut cyclic = Resolver(BTreeMap::from([("a.inc".into(), root.clone())]));
	expect_true!(parse(root.clone(), &mut cyclic, ImportLimits::default()).is_err());
	let mut resolver = Resolver(BTreeMap::from([(
		"a.inc".into(),
		source(2, "a.inc", "qubit q;"),
	)]));
	for limits in [
		ImportLimits {
			include_count: 0,
			..ImportLimits::default()
		},
		ImportLimits {
			include_depth: 0,
			..ImportLimits::default()
		},
		ImportLimits {
			source_bytes: root.text().len(),
			..ImportLimits::default()
		},
	] {
		expect_true!(parse(root.clone(), &mut resolver, limits).is_err());
	}
	Ok(())
}
#[gtest]
fn conflicting_source_identity_and_nested_include_are_rejected() {
	let mut resolver = Resolver(BTreeMap::from([(
		"a.inc".into(),
		source(1, "other", "qubit q;"),
	)]));
	expect_true!(
		parse(
			source(1, "main", "include \"a.inc\";"),
			&mut resolver,
			ImportLimits::default()
		)
		.is_err()
	);
	expect_true!(
		parse(
			source(1, "main", "if (true) { include \"a.inc\"; }"),
			&mut resolver,
			ImportLimits::default()
		)
		.is_err()
	);
}

#[gtest]
fn parser_budgets_keep_exact_resource_metadata() -> Result<()> {
	use quest_language::{ResourceKind, ResourceUsage, syntax::ParseLimits};
	for (limits, resource, requested, limit) in [
		(
			ParseLimits {
				source_bytes: 4,
				..ParseLimits::default()
			},
			ResourceKind::SourceBytes,
			11,
			4,
		),
		(
			ParseLimits {
				tokens: 1,
				..ParseLimits::default()
			},
			ResourceKind::SyntaxTokens,
			2,
			1,
		),
		(
			ParseLimits {
				nesting: 0,
				..ParseLimits::default()
			},
			ResourceKind::SyntaxNesting,
			1,
			0,
		),
	] {
		let error = parse(
			source(1, "budget.qasm", "qubit[1] q;"),
			&mut quest_qasm::NoIncludes,
			ImportLimits {
				parse: limits,
				..ImportLimits::default()
			},
		)
		.err()
		.ok_or_else(|| std::io::Error::other("parser limit must fail"))?;
		expect_eq!(
			error.cause,
			DiagnosticCause::ResourceLimit(ResourceUsage {
				resource,
				requested,
				limit
			})
		);
		error.validate_sources()?;
	}
	Ok(())
}
