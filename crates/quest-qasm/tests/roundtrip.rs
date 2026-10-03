use googletest::{Result, prelude::*};
use quest_language::{SourceId, SourceSnapshot, syntax};
use quest_qasm::{ExportLimits, export_syntax};
mod support;

#[gtest]
fn canonical_export_preserves_every_structured_syntax_variant() -> Result<()> {
	let source = SourceSnapshot::new(
		SourceId::new(1),
		"all.qasm",
		r#"
OPENQASM 3.1;
include "library.inc";
input float[64] theta;
output bit[2] result;
const uint[8] two = 2;
array[int[8], 2] data = {1, 2};
qubit[2] q;
let alias = q[0];
gate local(a) target { inv @ ctrl(2) @ pow(3) @ U(a, -pi/2, pi/2) q[0], q[1], target; gphase(a); }
def helper(readonly array[int[8], 2] x, mutable array[int[8], 2] y, qubit target) -> int[8] {
    y[0] += x[1];
    return y[0];
}
int[8] value = int[8](2 ** 3 + -4 * 5 / 2 % 3);
bool truth = !false && true || value <= 4 && value != 2;
value <<= 1;
value = (~value & 3) | (4 ^ 2);
if (truth) { result[0] = measure q[0]; } else { reset alias; }
switch (value) { case 0, 1 { x q[0]; } case 2 { z q[1]; } default { barrier q; } }
for int[8] i in [0:2:4] { if (i == 2) { continue; } }
for int[8] j in {1, 3, 5} { if (j > 3) { break; } }
for int[8] k in data { value += k; }
while (value > 0) { value -= 1; }
negctrl @ local(theta) q[0], q[1];
helper(data, data, q[0]);
measure q[1];
barrier;
end;
"#,
	);
	let original = syntax::parse_source(&source)?;
	let exported = export_syntax(&original, ExportLimits::default())?;
	expect_true!(exported.starts_with("OPENQASM 3.1;\n"));
	let second = syntax::parse_source(&SourceSnapshot::new(
		SourceId::new(2),
		"canonical.qasm",
		exported.clone(),
	))?;
	expect_eq!(
		support::normalize(original),
		support::normalize(second.clone())
	);
	expect_eq!(export_syntax(&second, ExportLimits::default())?, exported);
	Ok(())
}

#[gtest]
fn exporter_rejects_unresolved_captures_and_lexical_injection() -> Result<()> {
	use syntax::{Expression, ExpressionKind, Module, Statement, StatementKind};
	for value in [
		ExpressionKind::Capture(0),
		ExpressionKind::Name("a; reset q".into()),
	] {
		let module = Module {
			statements: vec![Statement {
				kind: StatementKind::Alias {
					name: "alias".into(),
					value: Expression {
						kind: value,
						span: None,
					},
				},
				span: None,
			}],
		};
		expect_true!(export_syntax(&module, ExportLimits::default()).is_err());
	}
	let module = syntax::parse_source(&SourceSnapshot::new(
		SourceId::new(3),
		"small.qasm",
		"qubit q;",
	))?;
	expect_true!(
		export_syntax(
			&module,
			ExportLimits {
				bytes: 4,
				..ExportLimits::default()
			}
		)
		.is_err()
	);
	Ok(())
}

#[gtest]
fn export_preserves_measurement_index_precedence_and_rejects_nontextual_types() -> Result<()> {
	let original = syntax::parse_source(&SourceSnapshot::new(
		SourceId::new(4),
		"precedence.qasm",
		"let selected = (measure q)[0];",
	))?;
	let text = export_syntax(&original, ExportLimits::default())?;
	let second = syntax::parse_source(&SourceSnapshot::new(SourceId::new(5), "again.qasm", text))?;
	expect_eq!(support::normalize(original), support::normalize(second));
	for expression in [
		syntax::ExpressionKind::Name("true".into()),
		syntax::ExpressionKind::Cast(
			syntax::Type::Qubit(None),
			Box::new(syntax::Expression {
				kind: syntax::ExpressionKind::Name("q".into()),
				span: None,
			}),
		),
	] {
		let module = syntax::Module {
			statements: vec![syntax::Statement {
				kind: syntax::StatementKind::Alias {
					name: "a".into(),
					value: syntax::Expression {
						kind: expression,
						span: None,
					},
				},
				span: None,
			}],
		};
		expect_true!(export_syntax(&module, ExportLimits::default()).is_err());
	}
	Ok(())
}

#[gtest]
fn export_budgets_report_exact_resource_usage() -> Result<()> {
	let empty = syntax::Module { statements: vec![] };
	let error = export_syntax(
		&empty,
		ExportLimits {
			bytes: 4,
			nesting: 4,
		},
	)
	.err()
	.ok_or_else(|| std::io::Error::other("header exceeds budget"))?;
	expect_eq!(
		error.cause,
		quest_language::DiagnosticCause::ResourceLimit(quest_language::ResourceUsage {
			resource: quest_language::ResourceKind::ExportBytes,
			requested: 14,
			limit: 4
		})
	);
	let module = syntax::parse_source(&SourceSnapshot::new(SourceId::new(1), "small", "qubit q;"))?;
	let error = export_syntax(
		&module,
		ExportLimits {
			nesting: 0,
			..ExportLimits::default()
		},
	)
	.err()
	.ok_or_else(|| std::io::Error::other("statement exceeds nesting budget"))?;
	expect_eq!(
		error.cause,
		quest_language::DiagnosticCause::ResourceLimit(quest_language::ResourceUsage {
			resource: quest_language::ResourceKind::ExportNesting,
			requested: 1,
			limit: 0
		})
	);
	Ok(())
}

#[gtest]
fn gate_operands_cannot_be_reinterpreted_as_parameters() -> Result<()> {
	let original = syntax::parse_source(&SourceSnapshot::new(
		SourceId::new(1),
		"operand.qasm",
		"x() (-q); inv @ local(0.25); pow(2) @ empty();",
	))?;
	let text = export_syntax(&original, ExportLimits::default())?;
	let second = syntax::parse_source(&SourceSnapshot::new(SourceId::new(2), "again.qasm", text))?;
	expect_eq!(support::normalize(original), support::normalize(second));
	for name in ["reset", "gphase"] {
		let module = syntax::Module {
			statements: vec![syntax::Statement {
				kind: syntax::StatementKind::Expression(syntax::Expression {
					kind: syntax::ExpressionKind::Call(name.into(), vec![]),
					span: None,
				}),
				span: None,
			}],
		};
		expect_true!(export_syntax(&module, ExportLimits::default()).is_err());
	}
	Ok(())
}
