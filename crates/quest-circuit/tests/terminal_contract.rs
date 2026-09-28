use googletest::{Result, prelude::*};
use quest_circuit::*;

fn ledger() -> BudgetLedger {
    BudgetLedger::new(OptimizationLimits::default())
}
fn target(width: usize) -> quest_circuit::Result<OptimizationTarget> {
    OptimizationTarget::once(
        DeploymentSnapshot::new(
            DeploymentKind::StateVector,
            width,
            false,
            false,
            false,
            0,
            1,
            1 << width,
        )?,
        CostProfile::NativeV1,
    )
}

#[gtest]
fn all_pair_precedence_prevents_nontransitive_commutation() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    let a = b.gate(Gate::Rx(Angle::pi(1, 7)?), &[b.qubit(0)?], &[])?;
    let middle = b.gate(Gate::Rz(Angle::pi(1, 5)?), &[b.qubit(1)?], &[])?;
    let c = b.gate(Gate::Rz(Angle::pi(1, 3)?), &[b.qubit(0)?], &[])?;
    let p = b.finish()?.bind(&[])?;
    let schedule = p.commutation_schedule(TerminalOptions::default(), &ledger())?;
    let order = schedule.order();
    expect_lt!(
        order.iter().position(|id| *id == a),
        order.iter().position(|id| *id == c)
    );
    expect_eq!(order.len(), 3);
    expect_true!(order.contains(&middle));
    Ok(())
}

#[gtest]
fn mandatory_edges_remain_ordered_and_prevent_contraction() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    let a = b.gate(Gate::H, &[b.qubit(0)?], &[])?;
    let middle = b.gate(Gate::H, &[b.qubit(1)?], &[])?;
    let c = b.gate(Gate::H, &[b.qubit(0)?], &[])?;
    b.depend(a, middle)?;
    b.depend(middle, c)?;
    let p = b.finish()?.bind(&[])?;
    let outcome = p.schedule_and_fuse(TerminalOptions::default(), &target(2)?, &ledger())?;
    expect_eq!(outcome.program().instructions().len(), 3);
    expect_eq!(outcome.program().instructions()[0].id(), a);
    expect_eq!(outcome.program().instructions()[2].id(), c);
    outcome.into_program().plan()?;
    Ok(())
}

#[gtest]
fn terminal_fusion_improves_native_score_and_marks_rounding() -> Result<()> {
    let mut b = ProgramBuilder::new(1, 0)?;
    let q = b.qubit(0)?;
    b.gate(Gate::X, &[q], &[])?;
    b.gate(Gate::Z, &[q], &[])?;
    let outcome = b.finish()?.bind(&[])?.schedule_and_fuse(
        TerminalOptions::default(),
        &target(1)?,
        &ledger(),
    )?;
    expect_eq!(outcome.program().instructions().len(), 1);
    expect_true!(outcome.report().rounding_changed());
    if let Operation::Numerical { matrix, .. } = outcome.program().instructions()[0].operation() {
        expect_eq!(matrix.view()[(0, 1)].re, 1.0);
        expect_eq!(matrix.view()[(1, 0)].re, -1.0);
    } else {
        fail!("expected full-phase product Z*X")?;
    }
    Ok(())
}

#[gtest]
fn scheduling_is_stable_and_favors_compatible_ready_gates() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    let a = b.gate(Gate::H, &[b.qubit(0)?], &[])?;
    let middle = b.gate(Gate::H, &[b.qubit(1)?], &[])?;
    let c = b.gate(Gate::X, &[b.qubit(0)?], &[])?;
    let p = b.finish()?.bind(&[])?;
    let one = p.commutation_schedule(TerminalOptions::default(), &ledger())?;
    let two = p.commutation_schedule(TerminalOptions::default(), &ledger())?;
    expect_eq!(one.order(), &[a, c, middle]);
    expect_eq!(one.order(), two.order());
    expect_eq!(one.snapshot_id(), p.snapshot_id());
    Ok(())
}

#[gtest]
fn negative_control_cross_target_is_not_a_commutation_proof() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    let q = b.qubit(0)?;
    let r = b.qubit(1)?;
    let a = b.gate(Gate::X, &[q], &[Control::new(r, ControlState::Zero)])?;
    let middle = b.gate(Gate::X, &[r], &[Control::new(q, ControlState::One)])?;
    let c = b.gate(Gate::X, &[q], &[Control::new(r, ControlState::Zero)])?;
    let p = b.finish()?.bind(&[])?;
    expect_eq!(
        p.commutation_schedule(TerminalOptions::default(), &ledger())?
            .order(),
        &[a, middle, c]
    );
    Ok(())
}

#[gtest]
fn matrix_limit_includes_external_controls() -> Result<()> {
    let mut b = ProgramBuilder::new(5, 0)?;
    let controls = (1..5)
        .map(|i| Ok(Control::new(b.qubit(i)?, ControlState::Zero)))
        .collect::<quest_circuit::Result<Vec<_>>>()?;
    let q = b.qubit(0)?;
    b.gate(Gate::H, &[q], &controls)?;
    b.gate(Gate::X, &[q], &controls)?;
    let outcome = b.finish()?.bind(&[])?.schedule_and_fuse(
        TerminalOptions::default(),
        &target(5)?,
        &ledger(),
    )?;
    expect_eq!(outcome.program().instructions().len(), 2);
    expect_false!(outcome.report().rounding_changed());
    Ok(())
}

#[gtest]
fn opaque_matrices_are_schedule_and_fusion_boundaries() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    let q = b.qubit(0)?;
    let r = b.qubit(1)?;
    let a = b.gate(Gate::H, &[q], &[])?;
    let matrix = BoundGate::X.matrix(MatrixPolicy::default())?;
    let fence = b.numerical(matrix, &[r], &[])?;
    let c = b.gate(Gate::H, &[q], &[])?;
    let p = b.finish()?.bind(&[])?;
    expect_eq!(
        p.commutation_schedule(TerminalOptions::default(), &ledger())?
            .order(),
        &[a, fence, c]
    );
    let outcome = p.schedule_and_fuse(TerminalOptions::default(), &target(2)?, &ledger())?;
    expect_eq!(outcome.program().instructions().len(), 3);
    Ok(())
}

#[gtest]
fn unknown_mpi_cost_cannot_authorize_changed_dispatches() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    let q = b.qubit(0)?;
    b.gate(Gate::H, &[q], &[])?;
    b.gate(Gate::X, &[q], &[])?;
    let distributed = OptimizationTarget::once(
        DeploymentSnapshot::new(DeploymentKind::StateVector, 2, false, true, false, 0, 2, 2)?,
        CostProfile::NativeV1,
    )?;
    let p = b.finish()?.bind(&[])?;
    let snapshot = p.snapshot_id();
    let outcome = p.schedule_and_fuse(TerminalOptions::default(), &distributed, &ledger())?;
    expect_eq!(outcome.report().status(), TerminalStatus::Unscorable);
    expect_eq!(outcome.program().snapshot_id(), snapshot);
    expect_eq!(outcome.program().instructions().len(), 2);
    Ok(())
}

#[gtest]
fn deterministic_exhaustion_returns_original_valid_snapshot() -> Result<()> {
    let mut b = ProgramBuilder::new(1, 0)?;
    let q = b.qubit(0)?;
    b.gate(Gate::H, &[q], &[])?;
    b.gate(Gate::X, &[q], &[])?;
    let p = b.finish()?.bind(&[])?;
    let snapshot = p.snapshot_id();
    let small = BudgetLedger::new(OptimizationLimits::new(200, 256 * 1024 * 1024)?);
    let outcome = p.schedule_and_fuse(TerminalOptions::default(), &target(1)?, &small)?;
    expect_eq!(outcome.report().status(), TerminalStatus::WorkLimit);
    expect_eq!(outcome.program().snapshot_id(), snapshot);
    outcome.into_program().plan()?;
    expect_le!(small.usage().work, 200);
    Ok(())
}

#[gtest]
fn only_strict_improvement_over_identical_terminal_baseline_is_published() -> Result<()> {
    let mut b = ProgramBuilder::new(2, 0)?;
    b.gate(Gate::H, &[b.qubit(0)?], &[])?;
    b.gate(Gate::H, &[b.qubit(1)?], &[])?;
    b.gate(Gate::X, &[b.qubit(0)?], &[])?;
    let target = target(2)?;
    let options = TerminalOptions::new(64, 1, 32, 1024 * 1024, 64 * 1024 * 1024)?;
    let outcome = b
        .finish()?
        .bind(&[])?
        .schedule_and_fuse(options, &target, &ledger())?;
    expect_true!(outcome.report().reordered());
    expect_eq!(outcome.program().instructions().len(), 2);
    let report = outcome.report();
    expect_eq!(
        target.compare(
            report.published_cost().ok_or(Error::InvalidId)?,
            report.baseline_terminal_cost().ok_or(Error::InvalidId)?
        )?,
        CostComparison::Better
    );
    Ok(())
}

#[gtest]
fn reuse_and_preparation_cost_can_reject_a_shorter_matrix_program() -> Result<()> {
    let mut b = ProgramBuilder::new(1, 0)?;
    b.gate(Gate::H, &[b.qubit(0)?], &[])?;
    b.gate(Gate::X, &[b.qubit(0)?], &[])?;
    let once = target(1)?;
    let rare = OptimizationTarget::new(
        once.deployment(),
        BigRational::new(1.into(), 100_000.into()),
        CostProfile::NativeV1,
    )?;
    let outcome =
        b.finish()?
            .bind(&[])?
            .schedule_and_fuse(TerminalOptions::default(), &rare, &ledger())?;
    expect_eq!(outcome.program().instructions().len(), 2);
    expect_false!(outcome.report().rounding_changed());
    Ok(())
}

// Independent small reference: construct each full operator directly by basis
// substitution, then multiply in chronological order. Does not use embedding
// or multiplication from the production fusion helpers.
#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "Test-only fixed three-wire exhaustive matrix witness"
)]
fn full_operator(program: &BoundProgram) -> quest_circuit::Result<Vec<num_complex::Complex64>> {
    use num_complex::Complex64 as C;
    let width = program.num_qubits();
    if width > 3 {
        return Err(Error::Budget("reference width"));
    }
    let dim = 1 << width;
    let mut total = vec![C::new(0.0, 0.0); dim * dim];
    for i in 0..dim {
        total[i * dim + i] = C::new(1.0, 0.0);
    }
    for instruction in program.instructions() {
        let (matrix, targets, controls, phase) = match instruction.operation() {
            Operation::Gate {
                gate,
                targets,
                controls,
            } => (
                Some(gate.matrix(MatrixPolicy::default())?),
                targets.as_ref(),
                controls.as_ref(),
                None,
            ),
            Operation::Numerical {
                matrix,
                targets,
                controls,
            } => (
                Some(matrix.clone()),
                targets.as_ref(),
                controls.as_ref(),
                None,
            ),
            Operation::GlobalPhase { radians, controls } => {
                (None, &[][..], controls.as_ref(), Some(*radians))
            }
            _ => return Err(Error::Unsupported("reference operation")),
        };
        let mut step = vec![C::new(0.0, 0.0); dim * dim];
        for col in 0..dim {
            if !controls
                .iter()
                .all(|c| ((col >> c.qubit().index()) & 1 == 1) == (c.state() == ControlState::One))
            {
                step[col * dim + col] = C::new(1.0, 0.0);
            } else if let Some(angle) = phase {
                step[col * dim + col] = C::from_polar(1.0, angle);
            } else {
                let matrix = matrix.as_ref().ok_or(Error::InvalidId)?;
                let local_col = targets
                    .iter()
                    .enumerate()
                    .fold(0usize, |v, (i, q)| v | (((col >> q.index()) & 1) << i));
                for local_row in 0..matrix.dimension() {
                    let row = targets.iter().enumerate().fold(col, |v, (i, q)| {
                        (v & !(1 << q.index())) | (((local_row >> i) & 1) << q.index())
                    });
                    step[row * dim + col] = matrix.view()[(local_row, local_col)];
                }
            }
        }
        let mut next = vec![C::new(0.0, 0.0); dim * dim];
        for row in 0..dim {
            for col in 0..dim {
                for k in 0..dim {
                    next[row * dim + col] += step[row * dim + k] * total[k * dim + col];
                }
            }
        }
        total = next;
    }
    Ok(total)
}

#[gtest]
fn fusion_preserves_ordered_targets_signed_controls_and_relative_global_phase() -> Result<()> {
    let mut b = ProgramBuilder::new(3, 0)?;
    let q = b.qubit(0)?;
    let r = b.qubit(1)?;
    let s = b.qubit(2)?;
    let negative = [Control::new(r, ControlState::Zero)];
    b.gate(Gate::Rz(Angle::pi(2, 1)?), &[q], &negative)?;
    b.gate(Gate::H, &[q], &negative)?;
    b.gate(Gate::X, &[s], &[Control::new(q, ControlState::One)])?;
    b.global_phase(Angle::pi(1, 3)?, &[Control::new(s, ControlState::Zero)])?;
    b.gate(Gate::Ry(Angle::pi(1, 7)?), &[r], &[])?;
    b.gate(Gate::X, &[q], &[Control::new(s, ControlState::Zero)])?;
    let p = b.finish()?.bind(&[])?;
    let before = full_operator(&p)?;
    let outcome = p.schedule_and_fuse(TerminalOptions::default(), &target(3)?, &ledger())?;
    expect_true!(outcome.report().rounding_changed());
    let after = full_operator(outcome.program())?;
    for (before, after) in before.iter().zip(&after) {
        expect_lt!(std::ops::Sub::sub(*before, *after).norm(), 1e-13);
    }
    outcome.into_program().plan()?;
    Ok(())
}
