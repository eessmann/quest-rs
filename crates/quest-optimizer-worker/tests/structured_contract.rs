#![cfg(all(target_os = "linux", any(feature = "synthesis", feature = "zx")))]
use googletest::prelude::*;
use quest_circuit::{
    StructuredProgram,
    language::{
        self, GateKind as G,
        vm::{GateRequest, Interpreter, QuantumBackend, RunInputs},
    },
};
use quest_math::{Gate as M, Limits, Operation, Sequence};
use quest_optimizer_client::{Client, WorkerLimits};
fn worker() -> Result<Client> {
    Ok(Client::new(
        env!("CARGO_BIN_EXE_quest-optimizer-worker"),
        WorkerLimits::default(),
    )?)
}
#[derive(Default)]
struct Backend {
    operations: Vec<Operation>,
    effects: Vec<&'static str>,
}
impl QuantumBackend for Backend {
    type Error = std::io::Error;
    fn apply_gate(&mut self, request: GateRequest<'_>) -> std::result::Result<(), Self::Error> {
        let gate = match (request.gate, request.inverse) {
            (G::H, _) => M::H,
            (G::X | G::Cx | G::Ccx, _) => M::X,
            (G::Y | G::Cy, _) => M::Y,
            (G::Z | G::Cz, _) => M::Z,
            (G::Swap, _) => M::Swap,
            (G::S, false) | (G::Sdg, true) => M::S,
            (G::S, true) | (G::Sdg, false) => M::Sdg,
            (G::T, false) | (G::Tdg, true) => M::T,
            (G::T, true) | (G::Tdg, false) => M::Tdg,
            _ => return Err(std::io::Error::other("not an exact fixture gate")),
        };
        self.operations.push(Operation {
            gate,
            targets: request.targets.to_vec(),
            controls: request
                .controls
                .iter()
                .map(|c| quest_math::Control {
                    qubit: c.qubit,
                    positive: c.positive,
                })
                .collect(),
        });
        Ok(())
    }
    fn measure(&mut self, _: usize) -> std::result::Result<bool, Self::Error> {
        self.effects.push("measure");
        Ok(false)
    }
    fn reset(&mut self, _: usize) -> std::result::Result<(), Self::Error> {
        self.effects.push("reset");
        Ok(())
    }
    fn barrier(&mut self, _: &[usize]) -> std::result::Result<(), Self::Error> {
        self.effects.push("barrier");
        Ok(())
    }
}
#[cfg(feature = "synthesis")]
#[gtest]
fn structured_synthesis_preserves_signed_control_phase_and_source_export() -> Result<()> {
    let original =
        StructuredProgram::parse("qubit[2] q; negctrl @ rz(0.17) q[1],q[0];", "rotation.qasm")?
            .verify()?;
    let syntax = quest_circuit::qasm::export_syntax(
        original.clone().lower()?.plan()?.syntax(),
        quest_circuit::qasm::ExportLimits::default(),
    )?;
    let (candidate, report) =
        original.synthesize_rotations(&worker()?, 1e-12, 42, Limits::default())?;
    expect_eq!(report.rotations.len(), 1);
    expect_true!(report.operator_error_bound.is_some());
    let certificate = &report
        .rotations
        .first()
        .expect("one certificate")
        .certificate;
    let mut backend = Backend::default();
    Interpreter::default().run(candidate.ssa(), &mut backend, &RunInputs::default(), &[])?;
    quest_math::verify_exact(
        &Sequence {
            qubits: 2,
            operations: backend.operations,
        },
        certificate.sequence(),
        Limits {
            gates: 8192,
            ..Limits::default()
        },
    )?;
    expect_eq!(
        quest_circuit::qasm::export_syntax(
            candidate.lower()?.plan()?.syntax(),
            quest_circuit::qasm::ExportLimits::default()
        )?,
        syntax
    );
    Ok(())
}
#[cfg(feature = "synthesis")]
#[gtest]
fn structured_synthesis_bounds_exclusive_paths_and_proves_finite_loop_counts() -> Result<()> {
    let source =
        "qubit q; input bool choice; if(choice) { rz(0.17) q; } else { rx(0.23) q; ry(0.31) q; }";
    let (_, report) = StructuredProgram::parse(source, "branches.qasm")?
        .verify()?
        .synthesize_rotations(&worker()?, 1e-12, 42, Limits::default())?;
    let epsilon = quest_math::dyadic_from_bits(1e-12_f64.to_bits(), Limits::default())?;
    expect_eq!(
        report.operator_error_bound,
        Some(std::ops::Mul::mul(
            epsilon,
            quest_math::Rational::from_integer(2.into())
        ))
    );
    let (_, report) = StructuredProgram::parse(
        "qubit q; int n=0; while(n<2){rz(0.17) q;n+=1;}",
        "loop.qasm",
    )?
    .verify()?
    .synthesize_rotations(&worker()?, 1e-12, 42, Limits::default())?;
    expect_eq!(
        report.operator_error_bound,
        Some(std::ops::Mul::mul(
            quest_math::dyadic_from_bits(1e-12_f64.to_bits(), Limits::default())?,
            quest_math::Rational::from_integer(2.into()),
        ))
    );
    expect_eq!(report.rotations.len(), 1);
    let (_, dynamic) = StructuredProgram::parse(
        "qubit q; input int count; int n=0; while(n<count){rz(0.17) q;n+=1;}",
        "dynamic_loop.qasm",
    )?
    .verify()?
    .synthesize_rotations(&worker()?, 1e-12, 42, Limits::default())?;
    expect_true!(dynamic.operator_error_bound.is_none());
    let unbound =
        StructuredProgram::parse("qubit q; input float theta; rz(theta) q;", "dynamic.qasm")?
            .verify()?;
    expect_true!(
        unbound
            .synthesize_rotations(&worker()?, 1e-12, 42, Limits::default())
            .is_err()
    );
    Ok(())
}
#[cfg(feature = "zx")]
#[gtest]
fn structured_zx_retains_effects_cfg_and_failed_candidates() -> Result<()> {
    let original = StructuredProgram::parse("qubit[2] q; output int count=0; while(count<2){h q[1];h q[1];cx q[1],q[0];cx q[1],q[0];count+=1;} bit b=measure q[0]; reset q[1];","zx.qasm")?.verify()?;
    let missing = Client::new("/quest-worker-missing", WorkerLimits::default())?;
    let (retained, failed) = original
        .clone()
        .optimize_zx(&missing, 1, Limits::default())?;
    expect_eq!(retained.ssa().program(), original.ssa().program());
    expect_false!(failed.skipped.is_empty());
    let (candidate, report) = original
        .clone()
        .optimize_zx(&worker()?, 1, Limits::default())?;
    expect_false!(report.accepted.is_empty());
    expect_eq!(
        candidate.ssa().blocks().len(),
        original.ssa().blocks().len()
    );
    let mut before = Backend::default();
    let mut after = Backend::default();
    let before_output =
        Interpreter::default().run(original.ssa(), &mut before, &RunInputs::default(), &[])?;
    let after_output =
        Interpreter::default().run(candidate.ssa(), &mut after, &RunInputs::default(), &[])?;
    expect_eq!(before_output.outputs, after_output.outputs);
    expect_eq!(before.effects, after.effects);
    quest_math::verify_exact(
        &Sequence {
            qubits: 2,
            operations: before.operations,
        },
        &Sequence {
            qubits: 2,
            operations: after.operations,
        },
        Limits::default(),
    )?;
    expect_true!(
        candidate
            .ssa()
            .blocks()
            .iter()
            .flat_map(|b| &b.instructions)
            .all(|i| !matches!(i.kind, language::ssa::InstructionKind::Gate { .. }))
    );
    Ok(())
}

#[cfg(feature = "synthesis")]
#[gtest]
fn structured_synthesis_maps_a_middle_target_and_mixed_controls() -> Result<()> {
    let original = StructuredProgram::parse(
        "qubit[3] q; negctrl @ ctrl @ ry(0.23) q[2],q[0],q[1];",
        "mapping.qasm",
    )?
    .verify()?;
    let (candidate, report) =
        original.synthesize_rotations(&worker()?, 1e-10, 17, Limits::default())?;
    let certificate = &report
        .rotations
        .first()
        .expect("one certificate")
        .certificate;
    // The certificate orders target first, then caller controls: physical [1, 2, 0].
    let mapping = [1usize, 2, 0];
    let mut expected = certificate.sequence().clone();
    for operation in &mut expected.operations {
        for target in &mut operation.targets {
            *target = *mapping.get(*target).expect("certified target");
        }
        for control in &mut operation.controls {
            control.qubit = *mapping.get(control.qubit).expect("certified control");
        }
    }
    let mut backend = Backend::default();
    Interpreter::default().run(candidate.ssa(), &mut backend, &RunInputs::default(), &[])?;
    quest_math::verify_exact(
        &Sequence {
            qubits: 3,
            operations: backend.operations,
        },
        &expected,
        Limits {
            gates: 8192,
            ..Limits::default()
        },
    )?;
    Ok(())
}

#[cfg(feature = "zx")]
#[gtest]
fn structured_zx_preserves_a_real_extracted_nonidentity_scalar() -> Result<()> {
    let original = StructuredProgram::parse(
        "qubit[2] q; h q[1]; s q[1]; h q[1]; s q[1]; h q[1]; s q[1]; h q[1]; h q[1];",
        "phase.qasm",
    )?
    .verify()?;
    let (candidate, report) = original
        .clone()
        .optimize_zx(&worker()?, 3, Limits::default())?;
    expect_eq!(report.accepted.len(), 1);
    let mut before = Backend::default();
    let mut after = Backend::default();
    Interpreter::default().run(original.ssa(), &mut before, &RunInputs::default(), &[])?;
    Interpreter::default().run(candidate.ssa(), &mut after, &RunInputs::default(), &[])?;
    let actual = Sequence {
        qubits: 2,
        operations: after.operations,
    };
    quest_math::verify_exact(
        &actual,
        &Sequence {
            qubits: 2,
            operations: before.operations,
        },
        Limits::default(),
    )?;
    expect_true!(
        quest_math::verify_exact(
            &actual,
            &Sequence {
                qubits: 2,
                operations: vec![]
            },
            Limits::default()
        )
        .is_err()
    );
    Ok(())
}
