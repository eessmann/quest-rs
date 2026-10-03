use googletest::prelude::*;
use quest_language::{
	SourceId, SourceSnapshot,
	syntax::{BinaryOperator, ExpressionKind, StatementKind, parse_source},
};

#[gtest]
fn preserves_integer_precedence_and_structured_control() -> Result<()> {
	let source = SourceSnapshot::new(
		SourceId::new(91),
		"control.qasm",
		"OPENQASM 3.1; qubit[2] q; int x = 1 + 2 * 3; while (x > 0) { if (x == 2) { break; } x -= 1; } h q[0];",
	);
	let module = parse_source(&source)?;
	expect_eq!(module.statements.len(), 4);
	let declaration = &module.statements[1];
	let StatementKind::Declare {
		initializer: Some(expression),
		..
	} = &declaration.kind
	else {
		fail!("declaration expected")?;
		return Ok(());
	};
	expect_true!(matches!(
		expression.kind,
		ExpressionKind::Binary(BinaryOperator::Add, _, _)
	));
	expect_true!(matches!(
		module.statements[2].kind,
		StatementKind::While { .. }
	));
	Ok(())
}

#[gtest]
fn retains_gate_and_subroutine_declarations() -> Result<()> {
	let source = SourceSnapshot::new(
		SourceId::new(92),
		"defs.qasm",
		"gate turn(a) q { rx(a) q; } def sample(qubit q) -> bit { return measure q; } qubit q; turn(pi/7) q;",
	);
	let module = parse_source(&source)?;
	expect_true!(matches!(
		module.statements[0].kind,
		StatementKind::GateDeclaration { .. }
	));
	expect_true!(matches!(
		module.statements[1].kind,
		StatementKind::Subroutine { .. }
	));
	Ok(())
}

#[gtest]
fn rejects_truncated_syntax_and_hardware_capabilities() {
	for text in [
		"qubit[ q;",
		"defcal x $0 {}",
		"delay[10ns] q;",
		"while (true) {",
		"qubit q; x $0;",
	] {
		let source = SourceSnapshot::new(SourceId::new(93), "invalid.qasm", text);
		expect_true!(parse_source(&source).is_err(), "{text}");
	}
}

#[gtest]
fn comma_indices_match_nested_indices_and_scalar_gate_modifiers_survive() -> Result<()> {
	let source = SourceSnapshot::new(
		SourceId::new(94),
		"indices.qasm",
		"array[int,2,2] a; a[0,1] = 3; inv @ scalar(pi); scalar(pi);",
	);
	let module = parse_source(&source)?;
	let StatementKind::Assign { target, .. } = &module.statements[1].kind else {
		fail!("assignment expected")?;
		return Ok(());
	};
	let ExpressionKind::Index(base, _) = &target.kind else {
		fail!("index expected")?;
		return Ok(());
	};
	expect_true!(matches!(base.kind, ExpressionKind::Index(_, _)));
	expect_true!(matches!(&module.statements[2].kind,
        StatementKind::Gate { operands, modifiers, .. } if operands.is_empty() && modifiers.len() == 1));
	expect_true!(matches!(
		&module.statements[3].kind,
		StatementKind::Expression(_)
	));
	Ok(())
}
