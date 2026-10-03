use googletest::prelude::*;
use quest_compile::classical::{OptimizationLimits, optimize};
use quest_language::{
	SourceId, SourceSnapshot,
	semantic::{CompileLimits, admit},
	ssa::{InstructionKind as K, VerifiedProgram},
	syntax::parse_source,
};
fn compile(source: &str) -> Result<VerifiedProgram> {
	Ok(admit(
		parse_source(&SourceSnapshot::new(SourceId::new(91), "optimizer", source))?,
		CompileLimits::default(),
	)?
	.into_ssa()?)
}
#[gtest]
fn folds_constants_and_executable_branches_without_removing_gate_occurrences() -> Result<()> {
	let original = compile("qubit q;int n=1+2;if(n==3){x q;x q;}else{z q;}output int result=n;")?;
	let (optimized, report) = optimize(original, OptimizationLimits::default())?;
	verify_that!(report.constants_folded, gt(0))?;
	verify_that!(report.branches_simplified, gt(0))?;
	verify_eq!(
		optimized
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.filter(|item| matches!(item.kind, K::Gate { .. }))
			.count(),
		2
	)?;
	Ok(())
}
#[gtest]
fn cse_keeps_first_potential_trap_and_does_not_remove_unused_division() -> Result<()> {
	let original =
		compile("input int n;int x=n;int a=10/x;int b=10/x;int unused=1/x;output int result=a+b;")?;
	let (optimized, report) = optimize(original, OptimizationLimits::default())?;
	verify_that!(report.common_expressions_removed, gt(0))?;
	verify_eq!(
		optimized
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.filter(|item| matches!(
				item.kind,
				K::Binary {
					operator: quest_language::syntax::BinaryOperator::Divide,
					..
				}
			))
			.count(),
		2
	)?;
	Ok(())
}
#[gtest]
fn loops_with_different_incoming_values_remain_dynamic() -> Result<()> {
	let original = compile("int n=0;while(n<3){n+=1;}output int result=n;")?;
	let (optimized, _) = optimize(original, OptimizationLimits::default())?;
	verify_that!(optimized.blocks().len(), gt(1))?;
	Ok(())
}
#[gtest]
fn optimization_work_and_storage_limits_fail_transactionally() -> Result<()> {
	let program = compile("output int n=1+2;")?;
	let limits = OptimizationLimits {
		work: 0,
		..OptimizationLimits::default()
	};
	verify_that!(optimize(program.clone(), limits), err(anything()))?;
	let limits = OptimizationLimits {
		storage_bytes: 1,
		..OptimizationLimits::default()
	};
	verify_that!(optimize(program, limits), err(anything()))?;
	Ok(())
}

#[derive(Default)]
struct Backend {
	events: Vec<String>,
}
impl quest_language::vm::QuantumBackend for Backend {
	type Error = std::convert::Infallible;
	fn apply_gate(
		&mut self,
		request: quest_language::vm::GateRequest<'_>,
	) -> std::result::Result<(), Self::Error> {
		self.events
			.push(format!("{:?}:{:?}", request.gate, request.targets));
		Ok(())
	}
	fn measure(&mut self, qubit: usize) -> std::result::Result<bool, Self::Error> {
		self.events.push(format!("measure:{qubit}"));
		Ok(true)
	}
	fn reset(&mut self, qubit: usize) -> std::result::Result<(), Self::Error> {
		self.events.push(format!("reset:{qubit}"));
		Ok(())
	}
	fn barrier(&mut self, qubits: &[usize]) -> std::result::Result<(), Self::Error> {
		self.events.push(format!("barrier:{qubits:?}"));
		Ok(())
	}
}
#[gtest]
fn optimization_preserves_loops_joins_outputs_and_quantum_memory_effects() -> Result<()> {
	use quest_language::vm::{ClassicalValue, Interpreter, RunInputs};
	for source in [
		"qubit q;int n=0;while(n<3){x q;n+=1;}output int result=n;",
		"qubit q;input bool flag;int n;if(flag){n=3;}else{n=3;}if(n==3){x q;}else{z q;}output int result=n;",
		"qubit q;bit measured=measure q;reset q;barrier q;output int result=uint[1](measured);",
		"qubit q;def modify(mutable array[int,2] a){a[0]+=1;}array[int,2] a={1,2};modify(a);x q;output int result=a[0];",
	] {
		let original = compile(source)?;
		let (optimized, _) = optimize(original.clone(), OptimizationLimits::default())?;
		let mut inputs = RunInputs::default();
		if source.contains("input bool") {
			inputs.insert(
				"flag",
				ClassicalValue::Scalar(quest_language::classical::ScalarValue::boolean(false)),
			)?;
		}
		let mut left = Backend::default();
		let mut right = Backend::default();
		let before = Interpreter::default()
			.run(&original, &mut left, &inputs, &[])
			.map_err(|error| std::io::Error::other(format!("{source}: {error}")))?;
		let after = Interpreter::default().run(&optimized, &mut right, &inputs, &[])?;
		verify_eq!(before.outputs, after.outputs)?;
		verify_eq!(left.events, right.events)?;
	}
	Ok(())
}
#[gtest]
fn constant_and_dynamic_failures_keep_the_same_completed_effect_prefix() -> Result<()> {
	use quest_language::vm::{ClassicalValue, Interpreter, RunInputs};
	for source in [
		"qubit q;input int divisor;x q;int unused=1/divisor;z q;",
		"qubit q;x q;int unused=9223372036854775807+1;z q;",
		"qubit q;x q;float unused=sqrt(-1.0);z q;",
	] {
		let original = compile(source)?;
		let (optimized, _) = optimize(original.clone(), OptimizationLimits::default())?;
		let mut inputs = RunInputs::default();
		if source.contains("input int") {
			inputs.insert(
				"divisor",
				ClassicalValue::Scalar(quest_language::classical::ScalarValue::signed(
					quest_language::classical::Width::new(64)?,
					0,
				)?),
			)?;
		}
		let mut left = Backend::default();
		let mut right = Backend::default();
		let before = Interpreter::default()
			.run(&original, &mut left, &inputs, &[])
			.err()
			.ok_or_else(|| std::io::Error::other("original must trap"))?;
		let after = Interpreter::default()
			.run(&optimized, &mut right, &inputs, &[])
			.err()
			.ok_or_else(|| std::io::Error::other("optimized must trap"))?;
		verify_eq!(before.cause.to_string(), after.cause.to_string())?;
		verify_eq!(before.span, after.span)?;
		verify_eq!(left.events, right.events)?;
		verify_eq!(after.completed_quantum, 1)?;
	}
	Ok(())
}
#[gtest]
fn signed_zero_is_not_merged_across_constants_or_phi_inputs() -> Result<()> {
	use quest_language::{
		classical::ScalarValue,
		vm::{ClassicalValue, Interpreter, RunInputs},
	};
	let program = compile(
		"input bool flag;float value;if(flag){value=0.0;}else{value=-0.0;}output float result=value;",
	)?;
	let (optimized, _) = optimize(program, OptimizationLimits::default())?;
	let mut inputs = RunInputs::default();
	inputs.insert("flag", ClassicalValue::Scalar(ScalarValue::boolean(false)))?;
	let output = Interpreter::default().run(&optimized, &mut Backend::default(), &inputs, &[])?;
	let bits = output
		.outputs
		.get("result")
		.and_then(ClassicalValue::as_scalar)
		.ok_or_else(|| std::io::Error::other("output"))?
		.to_f64()?
		.to_bits();
	verify_eq!(bits, (-0.0f64).to_bits())?;
	Ok(())
}

#[gtest]
fn removes_dead_proven_nontrapping_computation() -> Result<()> {
	let program =
		compile("input bool flag;bool unused=flag==flag;int unused_number=1+2;qubit q;x q;")?;
	let (optimized, report) = optimize(program, OptimizationLimits::default())?;
	verify_that!(report.dead_instructions_removed, gt(0))?;
	verify_eq!(
		optimized
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.filter(|item| matches!(item.kind, K::Binary { .. }))
			.count(),
		0
	)?;
	Ok(())
}

#[gtest]
fn constant_gate_parameter_interpretation_folds_without_a_forbidden_source_cast() -> Result<()> {
	let source = "gate turn(theta) q { rx(theta) q; } qubit q; angle[8] theta = pi; turn(theta) q;";
	let original = compile(source)?;
	verify_that!(
		original
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.any(|item| matches!(item.kind, K::GateParameter { .. })),
		eq(true)
	)?;
	let (optimized, report) = optimize(original, OptimizationLimits::default())?;
	verify_that!(report.constants_folded, gt(0))?;
	verify_that!(
		optimized
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.any(|item| matches!(item.kind, K::GateParameter { .. })),
		eq(false)
	)?;
	verify_that!(optimized.blocks().iter().flat_map(|block| &block.instructions).any(|item| matches!(item.kind, K::Constant(value) if value.to_f64().ok() == Some(std::f64::consts::PI))), eq(true))?;
	Ok(())
}
