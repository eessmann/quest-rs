use googletest::{Result, prelude::*};
use quest_circuit::language::ssa::InstructionKind;
use quest_circuit::{StructuredProgram, StructuredQuantumOptions};

fn gates(program: &quest_circuit::VerifiedStructuredProgram) -> usize {
    program
        .ssa()
        .blocks()
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|item| matches!(item.kind, InstructionKind::Gate { .. }))
        .count()
}
#[gtest]
fn exact_quantum_windows_cancel_across_constant_indices_and_keep_loop_cfg() -> Result<()> {
    let source = "qubit[3] q; int i=0; while(i<3) { h q[0]; x q[2]; h q[0]; cx q[0],q[1]; cx q[0],q[1]; i+=1; }";
    let original = StructuredProgram::parse(source, "loop.qasm")?.verify()?;
    let blocks = original.ssa().blocks().len();
    let (optimized, report) = original.optimize_quantum(StructuredQuantumOptions::default())?;
    expect_eq!(gates(&optimized), 1);
    expect_eq!(optimized.ssa().blocks().len(), blocks);
    expect_eq!(report.before_gates, 5);
    expect_eq!(report.after_gates, 1);
    optimized.lower()?.plan()?;
    Ok(())
}
#[gtest]
fn float_parameters_only_allow_structural_inverse_and_never_ideal_pi_folding() -> Result<()> {
    let source = "qubit q; rz(pi) q; rz(pi) q; barrier q; rx(0.2) q; inv @ rx(0.2) q;";
    let original = StructuredProgram::parse(source, "float.qasm")?.verify()?;
    let (optimized, _) = original.optimize_quantum(StructuredQuantumOptions::default())?;
    expect_eq!(gates(&optimized), 2);
    Ok(())
}

#[derive(Default)]
struct ExactBackend {
    operations: Vec<quest_math::Operation>,
    effects: Vec<&'static str>,
}
impl quest_circuit::language::vm::QuantumBackend for ExactBackend {
    type Error = std::io::Error;
    fn apply_gate(
        &mut self,
        request: quest_circuit::language::vm::GateRequest<'_>,
    ) -> std::result::Result<(), Self::Error> {
        use quest_circuit::language::GateKind as G;
        use quest_math::Gate as M;
        let gate = match (request.gate, request.inverse) {
            (G::Id, _) => return Ok(()),
            (G::H, _) => M::H,
            (G::X | G::Cx | G::Ccx, _) => M::X,
            (G::Y | G::Cy, _) => M::Y,
            (G::Z | G::Cz, _) => M::Z,
            (G::Swap, _) => M::Swap,
            (G::S, false) | (G::Sdg, true) => M::S,
            (G::Sdg, false) | (G::S, true) => M::Sdg,
            (G::T, false) | (G::Tdg, true) => M::T,
            (G::Tdg, false) | (G::T, true) => M::Tdg,
            _ => return Err(std::io::Error::other("unsupported exact fixture gate")),
        };
        self.operations.push(quest_math::Operation {
            gate,
            targets: request.targets.to_vec(),
            controls: request
                .controls
                .iter()
                .map(|control| quest_math::Control {
                    qubit: control.qubit,
                    positive: control.positive,
                })
                .collect(),
        });
        Ok(())
    }
    fn measure(&mut self, _qubit: usize) -> std::result::Result<bool, Self::Error> {
        self.effects.push("measure");
        Ok(false)
    }
    fn reset(&mut self, _qubit: usize) -> std::result::Result<(), Self::Error> {
        self.effects.push("reset");
        Ok(())
    }
    fn barrier(&mut self, _qubits: &[usize]) -> std::result::Result<(), Self::Error> {
        self.effects.push("barrier");
        Ok(())
    }
}
fn execute(
    program: &quest_circuit::VerifiedStructuredProgram,
) -> Result<(ExactBackend, quest_circuit::language::vm::RunOutput)> {
    use quest_circuit::language::vm::{Interpreter, RunInputs};
    let mut backend = ExactBackend::default();
    let output =
        Interpreter::default().run(program.ssa(), &mut backend, &RunInputs::default(), &[])?;
    Ok((backend, output))
}
#[gtest]
fn vm_executed_loop_windows_and_signed_controls_have_exact_full_matrix_equality() -> Result<()> {
    let source = "qubit[3] q; output int i=0; while(i<3) { h q[0]; x q[2]; h q[0]; cx q[0],q[1]; cx q[0],q[1]; negctrl @ y q[1],q[2]; negctrl @ y q[1],q[2]; i+=1; }";
    let original = StructuredProgram::parse(source, "vm-loop.qasm")?.verify()?;
    let (optimized, report) = original
        .clone()
        .optimize_quantum(StructuredQuantumOptions::default())?;
    let (before, before_output) = execute(&original)?;
    let (after, after_output) = execute(&optimized)?;
    expect_eq!(before_output.outputs, after_output.outputs);
    expect_gt!(report.before_gates, report.after_gates);
    quest_math::verify_exact(
        &quest_math::Sequence {
            qubits: 3,
            operations: before.operations,
        },
        &quest_math::Sequence {
            qubits: 3,
            operations: after.operations,
        },
        quest_math::Limits::default(),
    )?;
    Ok(())
}
#[gtest]
fn parity_scalar_omega_is_encoded_as_a_proved_exact_clifford_word() -> Result<()> {
    let source = format!("qubit q; {}", "x q; t q; x q; t q; ".repeat(9));
    let original = StructuredProgram::parse(&source, "scalar.qasm")?.verify()?;
    let (optimized, report) = original
        .clone()
        .optimize_quantum(StructuredQuantumOptions::default())?;
    expect_eq!(report.after_gates, 6);
    let (before, _) = execute(&original)?;
    let (after, _) = execute(&optimized)?;
    let target = quest_math::Sequence {
        qubits: 1,
        operations: vec![quest_math::Operation {
            gate: quest_math::Gate::W,
            targets: vec![],
            controls: vec![],
        }],
    };
    for operations in [before.operations, after.operations] {
        quest_math::verify_exact(
            &quest_math::Sequence {
                qubits: 1,
                operations,
            },
            &target,
            quest_math::Limits::default(),
        )?;
    }
    Ok(())
}
#[gtest]
fn dynamic_indices_reference_parameters_and_effects_guard_windows() -> Result<()> {
    let source = "def pair(qubit a, qubit b) { x a; x b; } qubit[2] q; input int index; x q[index]; x q[index]; pair(q[0],q[1]); barrier q; bit b=measure q[0]; reset q[1];";
    let original = StructuredProgram::parse(source, "guards.qasm")?.verify()?;
    let before = original.ssa().program().clone();
    let (optimized, report) = original.optimize_quantum(StructuredQuantumOptions::default())?;
    expect_eq!(optimized.ssa().program(), &before);
    expect_eq!(report.before_gates, report.after_gates);
    Ok(())
}
#[gtest]
fn posttransform_compile_and_work_limits_are_enforced() -> Result<()> {
    let original = StructuredProgram::parse("qubit q; h q; h q;", "limits.qasm")?.verify()?;
    for options in [
        StructuredQuantumOptions {
            work: 0,
            ..StructuredQuantumOptions::default()
        },
        StructuredQuantumOptions {
            storage_bytes: 0,
            ..StructuredQuantumOptions::default()
        },
        StructuredQuantumOptions {
            compile: quest_circuit::language::semantic::CompileLimits {
                blocks: 0,
                ..quest_circuit::language::semantic::CompileLimits::default()
            },
            ..StructuredQuantumOptions::default()
        },
    ] {
        expect_true!(original.clone().optimize_quantum(options).is_err());
    }
    Ok(())
}

#[cfg(feature = "macros")]
#[gtest]
fn macro_captures_and_original_export_sources_survive_quantum_replacement() -> Result<()> {
    let mut reads = Vec::new();
    let source = quest_circuit::circuit! {
        qubit q;
        rx(${{ reads.push(()); 0.2 }}) q;
        h q; h q;
    }?;
    expect_eq!(reads.len(), 1);
    let original = source.verify()?;
    let original_plan = original.clone().lower()?.plan()?;
    let (optimized, report) = original.optimize_quantum(StructuredQuantumOptions::default())?;
    let plan = optimized.lower()?.plan()?;
    expect_eq!(report.before_gates, 3);
    expect_eq!(report.after_gates, 1);
    expect_eq!(plan.syntax(), original_plan.syntax());
    expect_eq!(plan.captures(), original_plan.captures());
    expect_eq!(plan.locations().len(), original_plan.locations().len());
    expect_eq!(
        plan.sources().iter().count(),
        original_plan.sources().iter().count()
    );
    let rewrite = report
        .rewrites
        .first()
        .ok_or_else(|| std::io::Error::other("missing cancellation provenance"))?;
    expect_eq!(rewrite.inputs.len(), 2);
    expect_true!(rewrite.outputs.is_empty());
    expect_true!(rewrite.inputs.iter().all(|input| input.span.is_some()));
    Ok(())
}

#[gtest]
fn classical_constants_enable_quantum_windows_without_crossing_trapping_work() -> Result<()> {
    let original = StructuredProgram::parse(
        "qubit[2] q; int index=1; h q[index]; h q[index];",
        "classical-then-quantum.qasm",
    )?
    .verify()?;
    let (classical, _) = original.optimize_classical(
        quest_circuit::language::ssa::optimization::OptimizationLimits::default(),
    )?;
    let (optimized, report) = classical.optimize_quantum(StructuredQuantumOptions::default())?;
    expect_eq!(report.after_gates, 0);
    optimized.lower()?.plan()?;
    let guarded = StructuredProgram::parse(
        "qubit q; input int divisor; h q; int value=1/divisor; h q;",
        "trap.qasm",
    )?
    .verify()?;
    let (unchanged, report) = guarded.optimize_quantum(StructuredQuantumOptions::default())?;
    expect_eq!(report.after_gates, 2);
    unchanged.lower()?.plan()?;
    Ok(())
}
