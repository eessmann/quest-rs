#![cfg(feature = "macros")]
use googletest::prelude::*;
#[allow(unused_imports)]
use quest_compile::prelude::*;
use quest_compile::{
	circuit, circuit_file,
	language::{ssa, syntax},
};

#[gtest]
fn macro_uses_shared_types_declarations_and_loop_ssa() -> Result<()> {
	let mut visits = Vec::new();
	let program = circuit! {
		gate turn(a) q { rz(a) q; }
		def twice(qubit q, float a) { turn(a) q; turn(a) q; }
		qubit q;
		int count = 0;
		while (count < 3) {
			twice(q, ${{ visits.push(1); 0.25 }});
			count += 1;
		}
	}?;
	expect_eq!(visits, vec![1]);
	let plan = program.verify()?.lower()?.plan()?;
	expect_eq!(plan.num_qubits(), 1);
	expect_eq!(plan.captures().len(), 1);
	expect_false!(plan.locations().is_empty());
	expect_true!(
		plan.locations()
			.iter()
			.all(|location| location.file.ends_with("structured_macro.rs"))
	);
	expect_true!(
		plan.ssa()
			.blocks()
			.iter()
			.any(|block| block.arguments.len() > 1)
	);
	expect_true!(
		plan.syntax()
			.statements
			.iter()
			.any(|statement| matches!(statement.kind, syntax::StatementKind::While { .. }))
	);
	Ok(())
}
#[gtest]
fn macro_float_pi_and_integer_division_remain_language_values() -> Result<()> {
	let program = circuit! { qubit q; rx(1/2) q; rz(pi/3) q; }?;
	let plan = program.verify()?.lower()?.plan()?;
	expect_true!(
		plan.ssa()
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.any(|instruction| matches!(instruction.kind, ssa::InstructionKind::Binary { .. }))
	);
	expect_true!(circuit! {qubit q; rx(${f64::NAN}) q;}.is_err());
	Ok(())
}
#[gtest]
fn compiler_tracked_file_and_bundled_includes_share_admission() -> Result<()> {
	let program = circuit_file!("fixtures/structured.qasm")?;
	let plan = program.verify()?.lower()?.plan()?;
	expect_eq!(plan.num_qubits(), 2);
	expect_ge!(plan.sources().iter().count(), 2);
	let inline = circuit! { include "stdgates.inc"; qubit q; h q; }?;
	expect_eq!(inline.verify()?.lower()?.plan()?.num_qubits(), 1);
	Ok(())
}

#[gtest]
fn captures_do_not_collide_with_macro_local_names() -> Result<()> {
	let __captures = 0.125;
	let __tokens = 0.25;
	let __value = 0.5;
	let program = circuit! {qubit q; rx(${__captures}) q; ry(${__tokens}) q; rz(${__value}) q;}?;
	let plan = program.verify()?.lower()?.plan()?;
	expect_eq!(
		plan.captures()
			.iter()
			.map(quest_compile::language::classical::ScalarValue::to_f64)
			.collect::<std::result::Result<Vec<_>, _>>()?,
		vec![0.125, 0.25, 0.5]
	);
	Ok(())
}
