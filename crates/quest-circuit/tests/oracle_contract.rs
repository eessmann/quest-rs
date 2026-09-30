use googletest::prelude::*;
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::{
    Angle, BoundGate, Control, ControlState, Gate, MatrixPolicy, Operation, OracleFragment,
    QuantumRegionBuilder,
};

fn fragment() -> quest_circuit::Result<OracleFragment> {
    let mut body = QuantumRegionBuilder::new(2, 0)?;
    body.gate(Gate::X, &[body.qubit(0)?], &[])?;
    body.gate(Gate::S, &[body.qubit(1)?], &[])?;
    body.global_phase(Angle::radians(0.25)?, &[])?;
    OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()
}

#[gtest]
fn retained_calls_share_storage_but_keep_distinct_occurrences() -> googletest::Result<()> {
    let fragment = fragment()?;
    let mut builder = QuantumRegionBuilder::new(3, 0)?;
    let targets = [builder.qubit(2)?, builder.qubit(0)?];
    let first = builder.oracle(&fragment, &targets, &[])?;
    let second = builder.oracle(&fragment, &targets, &[])?;
    expect_ne!(first, second);
    let plan = builder.finish()?.bind(&[])?.plan()?;
    expect_eq!(plan.instructions().len(), 2);
    for instruction in plan.instructions() {
        let Operation::Oracle {
            fragment: retained, ..
        } = instruction.operation()
        else {
            return fail!("oracle call was expanded");
        };
        expect_true!(retained.shares_storage_with(&fragment));
    }
    expect_eq!(plan.oracle_query_count()?, 2);
    Ok(())
}

#[gtest]
fn adjoint_reverses_operations_and_controls_global_phase() -> googletest::Result<()> {
    let fragment = fragment()?;
    let builder = QuantumRegionBuilder::new(3, 0)?;
    let targets = [builder.qubit(2)?, builder.qubit(0)?];
    let control = Control::new(builder.qubit(1)?, ControlState::Zero);
    let adjoint = fragment.adjoint();
    expect_true!(adjoint.shares_storage_with(&fragment));
    let operations = adjoint.decompose(&targets, &[control], MatrixPolicy::default())?;
    expect_true!(
        matches!(operations.first().ok_or_else(|| std::io::Error::other("missing operation"))?, Operation::GlobalPhase { radians, controls } if radians.to_bits() == (-0.25f64).to_bits() && controls.as_ref() == [control])
    );
    expect_true!(
        matches!(operations.get(1).ok_or_else(|| std::io::Error::other("missing operation"))?, Operation::Gate { gate: BoundGate::Sdg, targets: mapped, .. } if mapped.as_ref() == [targets[1]])
    );
    expect_true!(
        matches!(operations.get(2).ok_or_else(|| std::io::Error::other("missing operation"))?, Operation::Gate { gate: BoundGate::X, targets: mapped, .. } if mapped.as_ref() == [targets[0]])
    );
    expect_true!(
        fragment
            .decompose(&targets[..1], &[], MatrixPolicy::default())
            .is_err()
    );
    expect_true!(
        fragment
            .decompose(&[targets[0], targets[0]], &[], MatrixPolicy::default())
            .is_err()
    );
    expect_true!(
        fragment
            .decompose(
                &targets,
                &[Control::new(targets[0], ControlState::One)],
                MatrixPolicy::default()
            )
            .is_err()
    );
    Ok(())
}

#[cfg(feature = "macros")]
#[gtest]
fn structured_oracle_captures_once_and_preserves_calls() -> googletest::Result<()> {
    let mut captures = 0usize;
    let body = fragment()?;
    let program = quest_circuit::circuit! {
        oracle block[2] = ${{ captures = captures.saturating_add(1); body.clone() }};
        qubit[3] q;
        block q[2], q[0];
        adjoint @ negctrl @ block q[1], q[2], q[0];
    }?;
    expect_eq!(captures, 1);
    let plan = program.verify()?.lower()?.plan()?;
    expect_eq!(plan.oracle_captures().len(), 1);
    expect_true!(
        plan.oracle_captures()
            .get(&0)
            .is_some_and(|item| item.shares_storage_with(&body))
    );
    expect_eq!(
        plan.ssa()
            .blocks()
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| matches!(
                instruction.kind,
                quest_circuit::language::ssa::InstructionKind::Call { .. }
            ))
            .count(),
        2
    );
    let error = quest_circuit::qasm::export_syntax(
        plan.syntax(),
        quest_circuit::qasm::ExportLimits::default(),
    )
    .err()
    .ok_or_else(|| std::io::Error::other("capture has no text representation"))?;
    expect_true!(matches!(
        error.cause,
        quest_circuit::language::DiagnosticCause::UnsupportedCapability { .. }
    ));
    Ok(())
}

#[gtest]
fn numerical_adjoint_is_conjugate_transpose_without_exact_admission() -> googletest::Result<()> {
    let matrix = faer::mat![
        [
            num_complex::Complex64::new(0.0, 0.0),
            num_complex::Complex64::new(0.0, 1.0)
        ],
        [
            num_complex::Complex64::new(1.0, 0.0),
            num_complex::Complex64::new(0.0, 0.0)
        ]
    ];
    let matrix = quest_circuit::NumericalOperator::from_view(&matrix, MatrixPolicy::default())?;
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    builder.numerical(matrix.clone(), &[builder.qubit(0)?], &[])?;
    let program = builder.finish()?;
    expect_true!(program.clone().into_unitary().is_err());
    let fragment =
        OracleFragment::from_program(program.bind(&[])?, 1e-12, MatrixPolicy::default())?;
    let target = QuantumRegionBuilder::new(1, 0)?;
    let operations =
        fragment
            .adjoint()
            .decompose(&[target.qubit(0)?], &[], MatrixPolicy::default())?;
    let Operation::Numerical {
        matrix: adjoint, ..
    } = operations
        .first()
        .ok_or_else(|| std::io::Error::other("missing operation"))?
    else {
        return fail!("missing numerical adjoint");
    };
    for row in 0..2 {
        for col in 0..2 {
            expect_eq!(adjoint.view()[(row, col)], matrix.view()[(col, row)].conj());
        }
    }
    expect_true!(
        fragment
            .export_qasm(quest_circuit::qasm::ExportLimits::default())
            .is_err()
    );
    Ok(())
}

#[gtest]
fn portable_oracle_export_retains_global_phase_and_target_order() -> googletest::Result<()> {
    let text = fragment()?
        .adjoint()
        .export_qasm(quest_circuit::qasm::ExportLimits::default())?;
    expect_true!(text.contains("gphase((-(0.25)))"));
    expect_true!(text.contains("sdg() q[1]"));
    expect_true!(text.contains("x() q[0]"));
    Ok(())
}

#[cfg(feature = "macros")]
type RecordedOracle = (usize, Vec<usize>, Vec<(usize, bool)>, bool);
#[cfg(feature = "macros")]
#[derive(Default)]
struct OracleRecorder {
    calls: Vec<RecordedOracle>,
}
#[cfg(feature = "macros")]
impl quest_circuit::language::vm::QuantumBackend for OracleRecorder {
    type Error = std::io::Error;
    fn apply_oracle(
        &mut self,
        request: quest_circuit::language::vm::OracleRequest<'_>,
    ) -> Option<std::result::Result<(), Self::Error>> {
        self.calls.push((
            request.capture,
            request.targets.to_vec(),
            request
                .controls
                .iter()
                .map(|c| (c.qubit, c.positive))
                .collect(),
            request.adjoint,
        ));
        Some(Ok(()))
    }
    fn apply_gate(
        &mut self,
        _: quest_circuit::language::vm::GateRequest<'_>,
    ) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
    fn measure(&mut self, _: usize) -> std::result::Result<bool, Self::Error> {
        Ok(false)
    }
    fn reset(&mut self, _: usize) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
    fn barrier(&mut self, _: &[usize]) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}
#[cfg(feature = "macros")]
#[gtest]
fn nested_call_adjoint_reverses_oracle_order_and_preserves_controls() -> googletest::Result<()> {
    let body = fragment()?;
    let plan = quest_circuit::circuit! {
        oracle first[2] = ${body.clone()};
        oracle second[2] = ${body};
        gate pair a, b { first a, b; second b, a; }
        qubit[3] q;
        adjoint @ negctrl @ pair q[1], q[2], q[0];
    }?
    .verify()?
    .lower()?
    .plan()?;
    let mut backend = OracleRecorder::default();
    let result = quest_circuit::language::vm::Interpreter::default().run(
        plan.ssa(),
        &mut backend,
        &quest_circuit::language::vm::RunInputs::default(),
        plan.captures(),
    )?;
    expect_eq!(result.completed_quantum, 2);
    expect_eq!(
        backend.calls,
        vec![
            (1, vec![0, 2], vec![(1, false)], true),
            (0, vec![2, 0], vec![(1, false)], true)
        ]
    );
    Ok(())
}

#[cfg(feature = "macros")]
#[gtest]
fn inverse_of_a_gate_containing_a_numerical_oracle_fails_before_dispatch() -> googletest::Result<()>
{
    let body = fragment()?;
    let plan = quest_circuit::circuit! {
        oracle block[2] = ${body};
        gate wrapper a, b { block a, b; }
        qubit[2] q;
        inv @ wrapper q[0], q[1];
    }?
    .verify()?
    .lower()?
    .plan()?;
    let mut backend = OracleRecorder::default();
    let result = quest_circuit::language::vm::Interpreter::default().run(
        plan.ssa(),
        &mut backend,
        &quest_circuit::language::vm::RunInputs::default(),
        plan.captures(),
    );
    expect_true!(result.is_err());
    expect_true!(backend.calls.is_empty());
    Ok(())
}

#[cfg(feature = "macros")]
#[gtest]
fn capture_arity_is_checked_before_a_verified_plan_exists() -> googletest::Result<()> {
    let body = fragment()?;
    let program = quest_circuit::circuit! {
        oracle wrong[1] = ${body};
        qubit q;
        wrong q;
    }?;
    expect_true!(program.verify().is_err());
    Ok(())
}

#[cfg(feature = "macros")]
#[gtest]
fn negative_runtime_power_of_oracle_never_dispatches() -> googletest::Result<()> {
    let body = fragment()?;
    let plan = quest_circuit::circuit! {
        oracle block[2] = ${body};
        qubit[2] q;
        int n = -1;
        pow(n) @ block q[0], q[1];
    }?
    .verify()?
    .lower()?
    .plan()?;
    let mut backend = OracleRecorder::default();
    let result = quest_circuit::language::vm::Interpreter::default().run(
        plan.ssa(),
        &mut backend,
        &quest_circuit::language::vm::RunInputs::default(),
        plan.captures(),
    );
    expect_true!(result.is_err());
    expect_true!(backend.calls.is_empty());
    Ok(())
}

#[gtest]
fn storage_accounting_deduplicates_shared_bodies_without_deduplicating_queries()
-> googletest::Result<()> {
    let body = fragment()?;
    let adjoint = body.adjoint();
    expect_eq!(
        OracleFragment::shared_storage_bytes([&body])?,
        OracleFragment::shared_storage_bytes([&body, &adjoint])?
    );
    let mut builder = QuantumRegionBuilder::new(2, 0)?;
    let targets = [builder.qubit(0)?, builder.qubit(1)?];
    builder.oracle(&body, &targets, &[])?;
    builder.oracle(&body, &targets, &[])?;
    let outer = OracleFragment::builder(builder.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    expect_eq!(outer.query_count(), 3);
    expect_eq!(
        OracleFragment::shared_storage_bytes([&outer])?,
        OracleFragment::shared_storage_bytes([&outer, &body])?
    );
    Ok(())
}
