use crate::Result;
use quest_language::{
	DiagnosticCause, GateKind, Label, LabelStyle, SourceMap, SourceSpan, Stage,
	syntax::{self, Module, Statement, StatementKind},
};
use std::collections::BTreeMap;

pub fn expand(module: Module, sources: &SourceMap) -> Result<Module> {
	let mut pinned = BTreeMap::new();
	let mut statements = Vec::new();
	for statement in module.statements {
		if let StatementKind::GateDeclaration {
			name,
			parameters,
			qubits,
			..
		} = &statement.kind
			&& let Some(kind) = GateKind::lookup(name)
			&& let Some(span) = statement.span
			&& let Some(source) = sources.get(span.source())
			&& source.text() == crate::STANDARD_GATES
		{
			if let std::collections::btree_map::Entry::Vacant(entry) = pinned.entry(source.id()) {
				let reference = syntax::parse_source(source)
					.map_err(|error| rejected(sources, error.span, &error.message))?;
				entry.insert(reference);
			}
			let canonical = pinned.get(&source.id()).is_some_and(|module| {
				module
					.statements
					.iter()
					.any(|candidate| candidate == &statement)
			});
			if !canonical {
				return Err(rejected(
					sources,
					Some(span),
					"declaration does not match its pinned standard-library source",
				));
			}
			let definition = kind.definition();
			let operands = definition
				.target_count
				.checked_add(definition.intrinsic_controls)
				.ok_or_else(|| rejected(sources, Some(span), "registry operand count overflow"))?;
			if parameters.len() != definition.parameter_count || qubits.len() != operands {
				return Err(rejected(
					sources,
					Some(span),
					"pinned declaration differs from registry signature",
				));
			}
		} else {
			push(&mut statements, statement, sources)?;
		}
	}
	Ok(Module { statements })
}
fn push(statements: &mut Vec<Statement>, statement: Statement, sources: &SourceMap) -> Result<()> {
	statements.try_reserve(1).map_err(|_| {
		let mut diagnostic = crate::failure(
			Stage::Admission,
			DiagnosticCause::ResourceFailure {
				reason: "registry expansion allocation failed".into(),
			},
			"registry expansion allocation failed",
		);
		diagnostic.sources = sources.clone();
		diagnostic
	})?;
	statements.push(statement);
	Ok(())
}
fn rejected(
	sources: &SourceMap,
	span: Option<SourceSpan>,
	message: &str,
) -> Box<quest_language::Diagnostic> {
	let mut error = crate::failure(
		Stage::Admission,
		DiagnosticCause::IncludeFailure {
			include: "stdgates.inc".into(),
			reason: message.into(),
		},
		message,
	);
	error.sources = sources.clone();
	if let Some(span) = span {
		error.labels.push(Label {
			span,
			style: LabelStyle::Primary,
			message: message.into(),
		});
	}
	error
}
