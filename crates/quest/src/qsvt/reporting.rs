//! Counts the fixed successful state-vector dispatch schedule before execution.
use crate::{Error, Result};
use quest_circuit::{BoundGate, ControlState, ExecutablePlan, Operation, dispatch_recipe};
use std::collections::BTreeMap;

/// Successful QSVT run calls into the native register API, per process/rank.
///
/// Counts native API dispatches, not backend kernels or MPI messages. Includes
/// projections, probability readout and Hadamard scratch operations. Excludes
/// preparation, run admission/fingerprint queries, MPI agreement, caller state
/// initialization, snapshots and later `condition()`. Errors publish no complete
/// count. The fixed schedule is checked during admission; execution has no counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeDispatchReport {
    circuit: usize,
    projection: usize,
    readout: usize,
    state_management: usize,
    total: usize,
}
impl NativeDispatchReport {
    pub(super) fn new(
        circuit: usize,
        projection: usize,
        overlap: bool,
        bridge: bool,
    ) -> Result<Self> {
        let (readout, state_management, projection) = if overlap {
            // Readout: four probabilities, two H and one phase gate. Scratch:
            // initial restore, readout clone, then clone+add per projection.
            let projections = 2_usize
                .checked_add(usize::from(bridge))
                .ok_or(Error::Overflow)?;
            let doubled = projections.checked_mul(2).ok_or(Error::Overflow)?;
            (
                7,
                doubled.checked_add(2).ok_or(Error::Overflow)?,
                projection.checked_add(doubled).ok_or(Error::Overflow)?,
            )
        } else {
            (
                3_usize
                    .checked_add(usize::from(bridge))
                    .ok_or(Error::Overflow)?,
                0,
                projection,
            )
        };
        let total = circuit
            .checked_add(projection)
            .and_then(|n| n.checked_add(readout))
            .and_then(|n| n.checked_add(state_management))
            .ok_or(Error::Overflow)?;
        Ok(Self {
            circuit,
            projection,
            readout,
            state_management,
            total,
        })
    }
    #[must_use]
    pub const fn total(self) -> usize {
        self.total
    }
    /// Gate, scalar-phase and numerical matrix applications in lowered circuits.
    #[must_use]
    pub const fn circuit(self) -> usize {
        self.circuit
    }
    /// Projectors, including the active/reference branch projectors for overlap.
    #[must_use]
    pub const fn projection(self) -> usize {
        self.projection
    }
    /// Probability queries and the gates used to read Hadamard interference.
    #[must_use]
    pub const fn readout(self) -> usize {
        self.readout
    }
    /// Register clones and additions used by overlap execution.
    #[must_use]
    pub const fn state_management(self) -> usize {
        self.state_management
    }
}

fn phase(zeros: usize) -> Result<usize> {
    dispatch_recipe::scalar_phase_recipe(0.0, zeros)
        .map(dispatch_recipe::GateRecipe::native_calls)
        .map_err(|_| Error::Overflow)
}
fn gate(gate: &BoundGate, zeros: usize) -> Result<usize> {
    dispatch_recipe::gate_recipe(gate, zeros)
        .map(dispatch_recipe::GateRecipe::native_calls)
        .map_err(|_| Error::Overflow)
}

pub(super) fn circuit(plan: &ExecutablePlan) -> Result<usize> {
    let mut memo = BTreeMap::new();
    plan.instructions()
        .iter()
        .try_fold(0_usize, |sum, instruction| {
            sum.checked_add(operation(instruction.operation(), 0, &mut memo)?)
                .ok_or(Error::Overflow)
        })
}
fn operation(
    op: &Operation,
    inherited: usize,
    memo: &mut BTreeMap<(usize, usize), usize>,
) -> Result<usize> {
    let zeros = |controls: &[quest_circuit::Control]| {
        inherited
            .checked_add(
                controls
                    .iter()
                    .filter(|c| c.state() == ControlState::Zero)
                    .count(),
            )
            .ok_or(Error::Overflow)
    };
    match op {
        Operation::Gate {
            gate: value,
            controls,
            ..
        } => gate(value, zeros(controls)?),
        Operation::GlobalPhase { controls, .. } => phase(zeros(controls)?),
        Operation::Numerical { .. } => Ok(1),
        Operation::Barrier { .. } => Ok(0),
        Operation::Oracle {
            fragment, controls, ..
        } => {
            let zeros = zeros(controls)?;
            let key = (fragment.operations().as_ptr().addr(), zeros);
            if let Some(&count) = memo.get(&key) {
                return Ok(count);
            }
            // Adjoint reverses order and gate angles, preserving dispatch count.
            let count = fragment.operations().iter().try_fold(0_usize, |sum, op| {
                sum.checked_add(operation(op, zeros, memo)?)
                    .ok_or(Error::Overflow)
            })?;
            memo.insert(key, count);
            Ok(count)
        }
        Operation::Conditional { .. }
        | Operation::Measure { .. }
        | Operation::Reset { .. }
        | Operation::Channel { .. } => {
            Err(Error::Unsupported("noncoherent QSVT dispatch schedule"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;
    use quest_circuit::{Control, Gate, OracleFragment, ProgramBuilder};

    #[gtest]
    fn unrepresentable_dispatch_totals_fail_before_execution() {
        expect_true!(matches!(phase(usize::MAX), Err(Error::Overflow)));
        expect_true!(matches!(
            NativeDispatchReport::new(usize::MAX, 0, false, false),
            Err(Error::Overflow)
        ));
        expect_true!(matches!(
            NativeDispatchReport::new(0, usize::MAX, true, true),
            Err(Error::Overflow)
        ));
    }

    #[gtest]
    fn nested_shared_adjoint_calls_count_decomposition_and_signed_controls()
    -> googletest::Result<()> {
        let mut body = ProgramBuilder::new(2, 0)?;
        let a = body.qubit(0)?;
        let b = body.qubit(1)?;
        body.gate(
            Gate::U {
                theta: quest_circuit::Angle::radians(0.2)?,
                phi: quest_circuit::Angle::radians(0.3)?,
                lambda: quest_circuit::Angle::radians(0.4)?,
            },
            &[b],
            &[Control::new(a, ControlState::Zero)],
        )?;
        body.global_phase(
            quest_circuit::Angle::radians(0.1)?,
            &[Control::new(b, ControlState::Zero)],
        )?;
        body.gate(Gate::Sx, &[a], &[])?;
        let fragment = OracleFragment::builder(body.finish()?.bind(&[])?)
            .matrix_tolerance(1e-12)?
            .build()?;
        let mut caller = ProgramBuilder::new(4, 0)?;
        let targets = [caller.qubit(2)?, caller.qubit(0)?];
        caller.oracle(
            &fragment,
            &targets,
            &[Control::new(caller.qubit(3)?, ControlState::Zero)],
        )?;
        caller.oracle(
            &fragment.adjoint(),
            &targets,
            &[Control::new(caller.qubit(1)?, ControlState::One)],
        )?;
        // Negative outer: U=16, global phase=5, Sx=4 =>25.
        // Positive outer: U=10, global phase=3, Sx=2 =>15.
        assert_that!(circuit(&caller.finish()?.bind(&[])?.plan()?)?, eq(40));
        assert_that!(
            NativeDispatchReport::new(40, 3, false, true)?.total(),
            eq(47)
        );
        // Three conditional projections add six projectors, eight clone/adds;
        // readout has seven calls, giving 40+3+6+8+7 =64.
        assert_that!(
            NativeDispatchReport::new(40, 3, true, true)?.total(),
            eq(64)
        );
        Ok(())
    }
}
