use googletest::prelude::*;
use quest_language::{
	SourceId, SourceSnapshot,
	semantic::{CompileLimits, finite::FiniteOperation},
	syntax::{self, Expression, ExpressionKind, Statement, StatementKind},
};
use quest_qasm::{ExportLimits, ImportLimits};

#[gtest]
fn both_finite_conditional_polarities_export_as_admitted_qasm() -> Result<()> {
	for expected in [false, true] {
		let mut module = syntax::parse_source(&SourceSnapshot::new(
			SourceId::new(1),
			"source.qasm",
			"OPENQASM 3.1; qubit q; bit b; b = measure q;",
		))?;
		let name = |name: &str| Expression {
			kind: ExpressionKind::Name(name.into()),
			span: None,
		};
		module.statements.push(Statement {
			kind: StatementKind::Finite {
				operations: vec![FiniteOperation::Conditional {
					bit: 0,
					expected,
					operation: Box::new(FiniteOperation::Reset(0)),
				}],
				qubits: vec![name("q")],
				bits: vec![name("b")],
			},
			span: None,
		});
		let text = quest_qasm::export_syntax(&module, ExportLimits::default())?;
		let imported = quest_qasm::import(
			SourceSnapshot::new(SourceId::new(2), "export.qasm", text.clone()),
			&mut quest_qasm::NoIncludes,
			ImportLimits::default(),
			CompileLimits::default(),
		)?;
		imported.into_typed().into_ssa()?;
		expect_true!(text.contains(if expected { "bool(b)" } else { "!bool(b)" }));
	}
	Ok(())
}
