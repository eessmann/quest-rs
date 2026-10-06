use googletest::prelude::*;
use quest_language::{
	SourceId, SourceSnapshot,
	semantic::{CompileLimits, admit},
	syntax::parse_source,
};

fn compile(
	text: &str,
) -> std::result::Result<quest_language::ssa::VerifiedProgram, Box<dyn std::error::Error>> {
	let source = SourceSnapshot::new(SourceId::new(7), "ssa", text);
	Ok(admit(parse_source(&source)?, CompileLimits::default())?.into_ssa()?)
}

#[gtest]
fn constant_boolean_short_circuit_skips_unexecuted_domain_errors() -> Result<()> {
	for expression in ["false && (1 / 0 == 0)", "true || (1 / 0 == 0)"] {
		let source = format!("const bool answer = {expression}; qubit q;");
		compile(&source).map_err(|error| std::io::Error::other(error.to_string()))?;
	}
	for expression in ["true && (1 / 0 == 0)", "false || (1 / 0 == 0)"] {
		let source = format!("const bool answer = {expression}; qubit q;");
		verify_that!(compile(&source), err(anything()))?;
	}
	for expression in ["false && 1", "true || missing", "false && sin(true)"] {
		verify_that!(
			compile(&format!("const bool answer = {expression}; qubit q;")),
			err(anything())
		)?;
	}
	Ok(())
}

#[gtest]
fn branch_heavy_region_verifies_with_linear_dominance_storage() -> Result<()> {
	let mut source = String::from("input bool flag; qubit q;");
	for _ in 0..150 {
		source.push_str("if (flag) { x q; } else { h q; }");
	}
	let program = compile(&source).map_err(|error| std::io::Error::other(error.to_string()))?;
	verify_that!(program.blocks().len(), gt(400))?;
	verify_that!(program.retained_bytes()?, lt(4 * 1024 * 1024))?;
	Ok(())
}
#[gtest]
fn verifies_loop_back_edges_and_preserves_gate_occurrences() -> Result<()> {
	let program = compile("qubit q; int n = 3; while (n > 0) { x q; x q; n -= 1; }")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	verify_that!(program.blocks().len(), gt(2))?;
	verify_eq!(
		program
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.filter(|instruction| matches!(
				instruction.kind,
				quest_language::ssa::InstructionKind::Gate { .. }
			))
			.count(),
		2
	)?;
	Ok(())
}
#[gtest]
fn rejects_reads_not_assigned_on_every_branch() {
	let result = compile("input bool flag; int x; if (flag) { x = 2; } int y = x;");
	verify_that!(result, err(anything())).and_log_failure();
}
#[gtest]
fn rejects_recursive_calls_and_aliased_gate_operands() {
	for text in ["def f() { f(); } f();", "qubit q; cx q, q;"] {
		verify_that!(compile(text), err(anything())).and_log_failure();
	}
}

#[gtest]
fn creates_scalar_block_arguments_for_branch_and_loop_carried_values() -> Result<()> {
	let program = compile("input bool flag; int x = 1; if (flag) { x = 2; } while (x < 4) { x += 1; } output int answer = x;")
        .map_err(|error| std::io::Error::other(error.to_string()))?;
	verify_that!(
		program
			.blocks()
			.iter()
			.filter(|block| block.arguments.len() > 1)
			.count(),
		gt(1)
	)?;
	verify_that!(
		program
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.filter(|item| matches!(item.kind, quest_language::ssa::InstructionKind::Load { .. }))
			.count(),
		le(1)
	)?;
	Ok(())
}

#[gtest]
fn independently_rejects_foreign_ids_effect_tampering_and_unsealed_blocks() -> Result<()> {
	use quest_language::ssa::Effect;
	let valid =
		compile("qubit q; x q;").map_err(|error| std::io::Error::other(error.to_string()))?;
	let foreign =
		compile("qubit q; x q;").map_err(|error| std::io::Error::other(error.to_string()))?;
	let mut unsealed = valid.clone().into_unverified();
	unsealed
		.blocks
		.first_mut()
		.ok_or_else(|| std::io::Error::other("missing block"))?
		.sealed = false;
	verify_that!(unsealed.verify(CompileLimits::default()), err(anything()))?;
	let mut wrong_effect = valid.clone().into_unverified();
	wrong_effect
		.blocks
		.first_mut()
		.and_then(|block| block.instructions.first_mut())
		.ok_or_else(|| std::io::Error::other("missing instruction"))?
		.effect = Effect::Pure;
	verify_that!(
		wrong_effect.verify(CompileLimits::default()),
		err(anything())
	)?;
	let mut foreign_value = valid.into_unverified();
	foreign_value
		.blocks
		.first_mut()
		.and_then(|block| block.arguments.first_mut())
		.ok_or_else(|| std::io::Error::other("missing memory"))?
		.id = foreign
		.blocks()
		.first()
		.and_then(|block| block.arguments.first())
		.ok_or_else(|| std::io::Error::other("missing foreign memory"))?
		.id;
	verify_that!(
		foreign_value.verify(CompileLimits::default()),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn admits_mixed_functions_dynamic_parameters_aliases_and_typed_return() -> Result<()> {
	compile("def sample(qubit q) -> bit { return measure q; } gate turn(a) q { rx(a) q; } qubit[2] q; let first = q[0]; input float theta; turn(theta) first; bit result = sample(first);")
        .map_err(|error| std::io::Error::other(error.to_string()))?;
	Ok(())
}

#[gtest]
fn verifier_rejects_forged_readonly_quantum_reference_parameter() -> Result<()> {
	let valid = compile("def flip(qubit q) { x q; } qubit q;")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	let mut forged = valid.clone().into_unverified();
	let parameter = forged
		.slots
		.iter_mut()
		.find(|slot| slot.reference && matches!(slot.ty, quest_language::ssa::Type::Qubit(_)))
		.ok_or_else(|| std::io::Error::other("missing quantum parameter"))?;
	parameter.mutable = false;
	verify_that!(forged.verify(CompileLimits::default()), err(anything()))?;
	verify_that!(valid.regions().len(), eq(2))?;
	Ok(())
}

#[gtest]
fn rejects_invalid_mutability_return_width_and_control_context() {
	for text in [
		"const int n = 1; n = 2;",
		"int[0] n;",
		"float[16] x;",
		"break;",
		"continue;",
		"return 1;",
		"def f(bool x) -> int { if(x) { return 1; } }",
		"qubit q; if (true) { int local = 1; } int x = local;",
		"def wrong() -> bool { return; }",
		"input bool x; output int result; if(x) { result = 2; }",
	] {
		verify_that!(compile(text), err(anything())).and_log_failure();
	}
}

#[gtest]
fn permits_value_parameter_mutation_but_rejects_effectful_gate_bodies() -> Result<()> {
	compile("def f(int n) -> int { n += 1; return n; } int answer = f(2);")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	verify_that!(
		compile("def sample(qubit q) { reset q; } gate wrong q { sample(q); } qubit q; wrong q;"),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn controlled_user_gate_carries_external_controls_separately() -> Result<()> {
	let program = compile("gate turn(a) q { rx(a) q; } qubit[2] q; ctrl @ turn(pi/4) q[0], q[1];")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	verify_that!(program.regions().len(), eq(2))?;
	Ok(())
}

#[gtest]
fn lowers_for_ranges_sets_arrays_and_switch_with_control_transfers() -> Result<()> {
	for source in [
		"qubit q; for int i in [0:2] { if (i == 1) { continue; } x q; }",
		"qubit q; for int i in {1,2,3} { if (i == 2) { break; } x q; }",
		"array[int,3] xs = {1,2,3}; int sum = 0; for int x in xs { sum += x; } output int total = sum;",
		"input int x; int y; switch (x) { case 0 { y = 1; } case 1,2 { y = 2; } default { y = 3; } } output int result = y;",
	] {
		compile(source).map_err(|error| std::io::Error::other(format!("{source}: {error}")))?;
	}
	Ok(())
}

#[gtest]
fn independent_verifier_rejects_forged_immutable_reinitialization() -> Result<()> {
	let mut program = compile("array[int,2] a = {1,2}; a = {3,4};")
		.map_err(|error| std::io::Error::other(error.to_string()))?
		.into_unverified();
	program
		.slots
		.first_mut()
		.ok_or_else(|| std::io::Error::other("missing array"))?
		.mutable = false;
	for instruction in program
		.blocks
		.iter_mut()
		.flat_map(|block| &mut block.instructions)
	{
		if let quest_language::ssa::InstructionKind::Store { initializing, .. } =
			&mut instruction.kind
		{
			*initializing = true;
		}
	}
	verify_that!(program.verify(CompileLimits::default()), err(anything()))?;
	Ok(())
}

#[gtest]
fn compilation_budgets_precede_recursive_ast_traversal_and_large_storage() -> Result<()> {
	use quest_language::syntax::{Expression, ExpressionKind, Module, Statement, StatementKind};
	let mut expression = Expression {
		kind: ExpressionKind::Bool(true),
		span: None,
	};
	for _ in 0..128 {
		expression = Expression {
			kind: ExpressionKind::Unary(
				quest_language::syntax::UnaryOperator::Not,
				Box::new(expression),
			),
			span: None,
		};
	}
	let module = Module {
		statements: vec![Statement {
			kind: StatementKind::Expression(expression),
			span: None,
		}],
	};
	let limits = CompileLimits {
		nodes: 20,
		..CompileLimits::default()
	};
	verify_that!(admit(module, limits), err(anything()))?;
	verify_that!(compile("array[int,999999999] values;"), err(anything()))?;
	Ok(())
}

#[gtest]
fn quantum_dependency_graph_retains_occurrences_without_cfg_backedges() -> Result<()> {
	let program = compile("qubit a; qubit b; int n = 2; while(n > 0) { x a; h b; x a; n -= 1; }")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	let dag = program
		.blocks()
		.iter()
		.map(quest_language::ssa::Block::quantum_dag)
		.find(|dag| dag.nodes.len() == 3)
		.ok_or_else(|| std::io::Error::other("missing loop quantum DAG"))?;
	let first = dag
		.nodes
		.first()
		.ok_or_else(|| std::io::Error::other("missing first gate"))?;
	let middle = dag
		.nodes
		.get(1)
		.ok_or_else(|| std::io::Error::other("missing independent gate"))?;
	let last = dag
		.nodes
		.last()
		.ok_or_else(|| std::io::Error::other("missing repeated gate"))?;
	verify_that!(&first.predecessors, is_empty())?;
	verify_that!(&middle.predecessors, is_empty())?;
	verify_eq!(&last.predecessors, &vec![first.instruction])?;
	Ok(())
}

#[gtest]
fn constant_widths_are_available_in_forward_function_signatures() -> Result<()> {
	compile("const int width = 8; def identity(int[width] x) -> int[width] { return x; } int[width] answer = identity(7);")
        .map_err(|error| std::io::Error::other(error.to_string()))?;
	Ok(())
}

#[gtest]
fn contextual_array_literals_obey_element_width_and_const_admission() -> Result<()> {
	compile("array[int[8],2] a = {1,2}; a = {3,4}; def sum(readonly array[int[8],2] a) -> int { return a[0] + a[1]; } output int total = sum(a);")
        .map_err(|error| std::io::Error::other(error.to_string()))?;
	let narrowed = compile("array[int[8],2] a = {1,128};")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	verify_that!(narrowed.blocks().iter().flat_map(|block| &block.instructions).any(|item| matches!(&item.kind, quest_language::ssa::InstructionKind::Constant(value) if value.to_i128() == Ok(-128))), eq(true))?;
	verify_that!(
		compile("input int n; const array[int,2] a = {1,n};"),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn proves_partial_array_and_bit_initialization_across_branches() -> Result<()> {
	for source in [
		"array[int,2] a; a[0]=1; a[1]=2; output int sum = a[0]+a[1];",
		"input bool flag; array[int,2] a; if(flag) { a={1,2}; } else { a[0]=3; a[1]=4; } output int sum=a[0]+a[1];",
		"bit[2] a; a[0]=bit(true); a[1]=bit(false); output bit[2] value=a;",
		"array[int,2,2] a; a[0]={1,2}; a[1][0]=3; a[1][1]=4; output int sum=a[0][0]+a[1][1];",
	] {
		compile(source).map_err(|error| std::io::Error::other(format!("{source}: {error}")))?;
	}
	for source in [
		"array[int,2] a; a[0]=1; output int bad=a[1];",
		"input int i; array[int,2] a; a[i]=1; output int bad=a[0];",
		"input bool flag; array[int,2] a; if(flag) { a[0]=1; } output int bad=a[0];",
	] {
		verify_that!(compile(source), err(anything()))?;
	}
	Ok(())
}

#[gtest]
fn broadcasts_user_gates_but_keeps_mixed_reference_shapes_exact() -> Result<()> {
	compile("gate turn(a) q { rx(a) q; } qubit[3] q; turn(pi/4) q;")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	verify_that!(
		compile("def inspect(qubit q) -> bit { return measure q; } qubit[2] q; bit b=inspect(q);"),
		err(anything())
	)?;
	verify_that!(
		compile("gate pair a,b {cx a,b;} qubit[2] a; qubit[3] b; pair a,b;"),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn normalized_negative_indices_cannot_hide_mutable_aliases() -> Result<()> {
	verify_that!(compile("qubit[2] q; cx q[-1],q[1];"), err(anything()))?;
	verify_that!(
		compile(
			"def f(mutable array[int,2] x,readonly array[int,2] y) { x[0]=y[0]; } array[int,2,2] a={{1,2},{3,4}}; f(a[-1],a[1]);"
		),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn constant_control_paths_preserve_definite_assignment_and_total_returns() -> Result<()> {
	for source in [
		"int x; if(true) {x=1;} output int result=x;",
		"def f() -> int { if(true) {return 1;} } output int result=f();",
		"def f() -> int { while(true) {return 1;} } output int result=f();",
	] {
		compile(source).map_err(|error| std::io::Error::other(format!("{source}: {error}")))?;
	}
	Ok(())
}

#[gtest]
fn rejects_nonunitary_gate_effects_even_in_constant_dead_branches() -> Result<()> {
	for source in [
		"gate wrong q {if(false) {reset q;}} qubit q; wrong q;",
		"gate wrong q {end;} qubit q; wrong q;",
	] {
		verify_that!(compile(source), err(anything()))?;
	}
	Ok(())
}

#[gtest]
fn local_nonconstant_shadow_cannot_supply_global_constant_array_extent() -> Result<()> {
	verify_that!(
		compile("const int n=2; def f(int n) {array[int,n] values;} f(3);"),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn independent_verifier_rejects_stale_memory_wrong_results_and_missing_edge_arguments() -> Result<()>
{
	use quest_language::ssa::{InstructionKind as K, Terminator, Type};
	let original = compile("input bool flag; qubit q; if(flag) {x q;} else {z q;} h q;")
		.map_err(|error| std::io::Error::other(error.to_string()))?
		.into_unverified();
	let mut stale = original.clone();
	let entry_memory = stale
		.blocks
		.first()
		.and_then(|block| block.arguments.first())
		.ok_or_else(|| std::io::Error::other("entry memory"))?
		.id;
	let gate = stale
		.blocks
		.iter_mut()
		.flat_map(|block| &mut block.instructions)
		.find(|item| matches!(item.kind, K::Gate { .. }))
		.ok_or_else(|| std::io::Error::other("gate"))?;
	if let K::Gate { memory, .. } = &mut gate.kind {
		*memory = entry_memory;
	}
	verify_that!(stale.verify(CompileLimits::default()), err(anything()))?;
	let mut wrong = original.clone();
	let result = wrong
		.blocks
		.iter_mut()
		.flat_map(|block| &mut block.instructions)
		.flat_map(|item| &mut item.results)
		.next()
		.ok_or_else(|| std::io::Error::other("result"))?;
	result.ty = Type::Void;
	verify_that!(wrong.verify(CompileLimits::default()), err(anything()))?;
	let mut missing = original;
	let term = missing
		.blocks
		.iter_mut()
		.find_map(|block| match &mut block.terminator {
			Some(Terminator::Branch { then_edge, .. }) => Some(then_edge),
			_ => None,
		})
		.ok_or_else(|| std::io::Error::other("branch"))?;
	term.arguments.clear();
	verify_that!(missing.verify(CompileLimits::default()), err(anything()))?;
	Ok(())
}

#[gtest]
fn independent_verifier_rejects_forward_value_use() -> Result<()> {
	let mut program = compile("input int n; output int result=(n+1)*2;")
		.map_err(|error| std::io::Error::other(error.to_string()))?
		.into_unverified();
	let block = program
		.blocks
		.first_mut()
		.ok_or_else(|| std::io::Error::other("block"))?;
	let positions = block
		.instructions
		.iter()
		.enumerate()
		.filter(|(_, item)| {
			matches!(
				item.kind,
				quest_language::ssa::InstructionKind::Binary { .. }
			)
		})
		.map(|(index, _)| index)
		.collect::<Vec<_>>();
	let [first, second] = positions.as_slice() else {
		return Err(std::io::Error::other("two binary instructions").into());
	};
	block.instructions.swap(*first, *second);
	verify_that!(program.verify(CompileLimits::default()), err(anything()))?;
	Ok(())
}

#[gtest]
fn retained_bytes_include_nested_owned_capacity() -> Result<()> {
	let source = SourceSnapshot::new(SourceId::new(97), "accounting", "output int result=1;");
	let mut module = parse_source(&source)?;
	let typed = admit(module.clone(), CompileLimits::default())?;
	let baseline = typed.retained_bytes()?;
	module.statements.reserve(256);
	let larger = admit(module, CompileLimits::default())?;
	verify_that!(
		larger.retained_bytes()?,
		gt(baseline.saturating_add(
			128usize.saturating_mul(std::mem::size_of::<quest_language::syntax::Statement>())
		))
	)?;
	let mut program = typed.into_ssa()?.into_unverified();
	let baseline = program
		.clone()
		.verify(CompileLimits::default())?
		.retained_bytes()?;
	program
		.slots
		.first_mut()
		.ok_or_else(|| std::io::Error::other("slot"))?
		.name
		.reserve(4096);
	verify_that!(
		program.verify(CompileLimits::default())?.retained_bytes()?,
		gt(baseline.saturating_add(3000))
	)?;
	Ok(())
}

#[gtest]
fn quantum_declarations_are_restricted_to_global_scope() -> Result<()> {
	for source in [
		"def f(){qubit q;} f();",
		"if(true){qubit q;}",
		"for int i in [0:1]{qubit q;}",
		"gate f q {qubit private;} qubit q;f q;",
	] {
		verify_that!(compile(source), err(anything()))?;
	}
	Ok(())
}

#[gtest]
fn global_quantum_allocations_hoist_without_forward_name_visibility() -> Result<()> {
	let program = compile("input bool flag;if(flag){int n=1;}qubit q;x q;")
		.map_err(|error| std::io::Error::other(error.to_string()))?;
	let entry = program
		.regions()
		.first()
		.ok_or_else(|| std::io::Error::other("entry"))?
		.entry;
	for block in program.blocks() {
		if block.instructions.iter().any(|item| {
			matches!(
				item.kind,
				quest_language::ssa::InstructionKind::Allocate { .. }
			)
		}) {
			verify_eq!(block.id, entry)?;
		}
	}
	verify_that!(compile("x q;qubit q;"), err(anything()))?;
	Ok(())
}

#[gtest]
fn switch_labels_are_distinct_after_selector_width_conversion() -> Result<()> {
	verify_that!(
		compile("input uint[8] x;switch(x){case 0 {} case 256 {}}"),
		err(anything())
	)?;
	Ok(())
}

#[gtest]
fn implicit_conversions_do_not_grant_explicit_bit_or_angle_casts() -> Result<()> {
	for source in [
		"bit b=true;",
		"bit[8] b=\"00000001\";int n=b;",
		"angle a=1.0;float f=a;",
		"const bit b=\"1\";const int n=b;",
		"array[bit,1] bits={true};",
		"def f(int x){} bit b=\"1\";f(b);",
	] {
		verify_that!(compile(source), err(anything()))?;
	}
	for source in [
		"int[8] x=128;float f=x;bool b=f;",
		"angle[8] a=1.0;angle[16] b=a;",
		"bit b=bit(true);int n=uint[1](b);",
		"gate f(a) q{rx(a) q;}angle a=1.0;qubit q;f(a) q;",
	] {
		compile(source).map_err(|error| std::io::Error::other(format!("{source}: {error}")))?;
	}
	Ok(())
}

#[gtest]
fn gate_parameters_require_real_values_without_implicit_bit_interpretation() -> Result<()> {
	for source in ["qubit q;rx(\"1\") q;", "qubit q;pow(\"1\") @ x q;"] {
		verify_that!(compile(source), err(anything()))?;
	}
	Ok(())
}

#[gtest]
fn gate_powers_reject_implicit_noninteger_counts_during_admission() -> Result<()> {
	for count in ["2.0", "angle(0.5)", "true", "bit(true)"] {
		for operation in ["x", "turn"] {
			let source = format!("gate turn q {{ s q; }} qubit q; pow({count}) @ {operation} q;");
			verify_that!(compile(&source), err(anything()))?;
		}
	}
	compile("qubit q; pow(int(true)) @ x q; pow(uint[1](bit(true))) @ x q;").or_fail()?;
	Ok(())
}

#[gtest]
fn independent_verifier_rejects_forged_noninteger_power_values() -> Result<()> {
	use quest_language::ssa::{GateModifier, InstructionKind};
	for ty in ["float", "angle", "bool", "bit"] {
		let mut program = compile(&format!(
			"input {ty} bad; {ty} copy = bad; qubit q; pow(2) @ x q;"
		))
		.or_fail()?
		.into_unverified();
		let bad = program
			.blocks
			.iter()
			.flat_map(|block| &block.instructions)
			.flat_map(|instruction| &instruction.results)
			.find(|value| {
				matches!(
					value.ty,
					quest_language::ssa::Type::Scalar(
						quest_language::classical::ScalarType::Float(_)
							| quest_language::classical::ScalarType::Angle(_)
							| quest_language::classical::ScalarType::Bool
							| quest_language::classical::ScalarType::Bit(_)
					)
				)
			})
			.or_fail()?
			.id;
		let modifiers = program
			.blocks
			.iter_mut()
			.flat_map(|block| &mut block.instructions)
			.find_map(|instruction| match &mut instruction.kind {
				InstructionKind::Gate { modifiers, .. } => Some(modifiers),
				_ => None,
			})
			.or_fail()?;
		*modifiers.first_mut().or_fail()? = GateModifier::Power(bad);
		verify_that!(program.verify(CompileLimits::default()), err(anything()))?;
	}
	Ok(())
}

#[gtest]
fn independent_verifier_rejects_foreign_region_entry_identity() -> Result<()> {
	let mut program = compile("int n = 1;").or_fail()?.into_unverified();
	let foreign = compile("int n = 1;").or_fail()?;
	let foreign_entry = foreign.regions().first().or_fail()?.entry;
	program.regions.first_mut().or_fail()?.entry = foreign_entry;
	verify_that!(program.verify(CompileLimits::default()).is_err(), eq(true))?;
	Ok(())
}

#[gtest]
fn independent_verifier_rejects_duplicate_named_input_output_interfaces() -> Result<()> {
	for source in [
		"output int left = 1; output int right = 2;",
		"input int left; input int right;",
		"input int left; output int right = 1;",
	] {
		let mut program = compile(source).or_fail()?.into_unverified();
		let first = program.slots.first().or_fail()?.name.clone();
		program.slots.get_mut(1).or_fail()?.name = first;
		verify_that!(program.verify(CompileLimits::default()).is_err(), eq(true))?;
	}
	Ok(())
}

#[gtest]
fn dynamic_explicit_casts_reject_forbidden_type_pairs_before_execution() -> Result<()> {
	for source in [
		"input angle[8] a; int[8] n = int[8](a);",
		"input angle[8] a; float n = float(a);",
		"input bit[8] a; float n = float(a);",
		"input bit[8] a; uint[16] n = uint[16](a);",
		"input bool a; bit[8] n = bit[8](a);",
		"input int a; angle n = angle(a);",
	] {
		verify_that!(compile(source).is_err(), eq(true))?;
	}
	for source in [
		"input angle[8] a; bit[8] n = bit[8](a);",
		"input bit[8] a; angle[8] n = angle[8](a);",
		"input bit[8] a; int[8] n = int[8](a);",
		"input float a; int n = int(a);",
		"gate turn(theta) q { rx(theta) q; } input angle a; qubit q; turn(a) q;",
	] {
		compile(source).or_fail()?;
	}
	Ok(())
}

#[gtest]
fn independent_verifier_rejects_forged_cast_between_forbidden_categories() -> Result<()> {
	use quest_language::classical::{FloatWidth, ScalarType, ScalarValue};
	use quest_language::ssa::{InstructionKind, Type};
	let mut program = compile("input angle a; angle b = a; float n = 1.0;")
		.or_fail()?
		.into_unverified();
	let angle = program
		.blocks
		.iter()
		.flat_map(|block| &block.instructions)
		.flat_map(|item| &item.results)
		.find(|value| matches!(value.ty, Type::Scalar(ScalarType::Angle(_))))
		.or_fail()?
		.id;
	let constant = program.blocks.iter_mut().flat_map(|block| &mut block.instructions)
        .find(|item| matches!(item.kind, InstructionKind::Constant(value) if value.ty() == ScalarType::Float(FloatWidth::F64))).or_fail()?;
	constant.kind = InstructionKind::Cast {
		value: angle,
		ty: ScalarType::Float(FloatWidth::F64),
	};
	constant.effect = constant.kind.effect();
	constant.accesses = constant.kind.accesses();
	verify_that!(program.verify(CompileLimits::default()).is_err(), eq(true))?;
	// Legal floating-to-integer conversion may still trap for the actual value.
	let huge = ScalarValue::floating(FloatWidth::F64, f64::MAX)?;
	verify_that!(
		huge.cast(ScalarType::Int(quest_language::classical::Width::new(8)?))
			.is_err(),
		eq(true)
	)?;
	Ok(())
}

#[gtest]
fn independent_verifier_requires_a_parameterless_void_mixed_program_entry() -> Result<()> {
	for source in [
		"def f(int n) { int m = n; } f(1);",
		"def f() -> int { return 1; } int n = f();",
	] {
		let mut program = compile(source).or_fail()?.into_unverified();
		program.entry = program.regions.get(1).or_fail()?.id;
		verify_that!(program.verify(CompileLimits::default()).is_err(), eq(true))?;
	}
	let mut program = compile("").or_fail()?.into_unverified();
	program.regions.first_mut().or_fail()?.gate = true;
	verify_that!(program.verify(CompileLimits::default()).is_err(), eq(true))?;
	Ok(())
}
