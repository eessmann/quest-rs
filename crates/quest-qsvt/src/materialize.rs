use crate::{Complex64, Error, NumericalPolicy, OracleFragment, Result, matrix};
use faer::{Mat, MatRef};
use quest_compile::{BoundRegion, Control, ControlState, Operation, QuantumRegionBuilder, QubitId};
use std::ops::{Add, Mul};

/// Independently materialize a coherent circuit using column-state matrix products.
/// This is a bounded construction/reference operation, never a runtime fallback.
///
/// # Errors
/// Rejects noncoherent operations, size overflow, and allocation limits.
pub fn materialize_program(
    program: &BoundRegion,
    policy: NumericalPolicy,
) -> Result<Mat<Complex64>> {
    let mut output = identity(program.num_qubits(), policy)?;
    for instruction in program.instructions() {
        apply(instruction.operation(), &mut output, policy)?;
    }
    Ok(output)
}

/// Independently materialize an immutable oracle, including its view orientation.
///
/// # Errors
/// Rejects dimension overflow and construction allocation limits.
pub fn materialize_oracle(
    oracle: &OracleFragment,
    policy: NumericalPolicy,
) -> Result<Mat<Complex64>> {
    let mut builder = QuantumRegionBuilder::new(oracle.num_qubits(), 0)?;
    let targets = (0..oracle.num_qubits())
        .map(|index| builder.qubit(index))
        .collect::<quest_compile::Result<Vec<_>>>()?;
    builder.oracle(oracle, &targets, &[])?;
    materialize_program(&builder.finish()?.bind(&[])?, policy)
}

fn identity(qubits: usize, policy: NumericalPolicy) -> Result<Mat<Complex64>> {
    let dimension = 1usize
        .checked_shl(u32::try_from(qubits).map_err(|_| Error::Budget("qubit width"))?)
        .ok_or(Error::Budget("Hilbert dimension"))?;
    policy.check(dimension, dimension, 6)?;
    matrix::allocate(dimension, dimension, policy, |row, col| {
        Complex64::new(if row == col { 1.0 } else { 0.0 }, 0.0)
    })
}

fn apply(
    operation: &Operation,
    output: &mut Mat<Complex64>,
    policy: NumericalPolicy,
) -> Result<()> {
    match operation {
        Operation::Gate {
            gate,
            targets,
            controls,
        } => apply_matrix(
            gate.matrix(policy.matrix_policy())?.view(),
            targets,
            controls,
            output,
            policy,
        ),
        Operation::Numerical {
            matrix,
            targets,
            controls,
        } => apply_matrix(matrix.view(), targets, controls, output, policy),
        Operation::GlobalPhase { radians, controls } => {
            let phase = Complex64::from_polar(1.0, *radians);
            for row in 0..output.nrows() {
                if selected(row, controls)? {
                    for col in 0..output.ncols() {
                        output[(row, col)] = output[(row, col)].mul(phase);
                    }
                }
            }
            Ok(())
        }
        Operation::Oracle {
            fragment,
            targets,
            controls,
        } => {
            for child in fragment.decompose(targets, controls, policy.matrix_policy())? {
                apply(&child, output, policy)?;
            }
            Ok(())
        }
        Operation::Barrier { .. } => Ok(()),
        _ => Err(Error::Encoding(
            "materialization requires coherent operations",
        )),
    }
}

fn selected(index: usize, controls: &[Control]) -> Result<bool> {
    for control in controls {
        let bit = bit(control.qubit().index())?;
        if (index & bit != 0) != (control.state() == ControlState::One) {
            return Ok(false);
        }
    }
    Ok(true)
}
fn bit(index: usize) -> Result<usize> {
    1usize
        .checked_shl(u32::try_from(index).map_err(|_| Error::Budget("bit position"))?)
        .ok_or(Error::Budget("bit position"))
}
fn local_index(index: usize, targets: &[QubitId]) -> Result<usize> {
    let mut local = 0usize;
    for (position, target) in targets.iter().enumerate() {
        if index & bit(target.index())? != 0 {
            local |= bit(position)?;
        }
    }
    Ok(local)
}
fn replace_index(index: usize, local: usize, targets: &[QubitId]) -> Result<usize> {
    let mut result = index;
    for (position, target) in targets.iter().enumerate() {
        let target_bit = bit(target.index())?;
        result &= !target_bit;
        if local & bit(position)? != 0 {
            result |= target_bit;
        }
    }
    Ok(result)
}
fn apply_matrix(
    operator: MatRef<'_, Complex64>,
    targets: &[QubitId],
    controls: &[Control],
    output: &mut Mat<Complex64>,
    policy: NumericalPolicy,
) -> Result<()> {
    let previous = matrix::snapshot(output.as_ref(), policy)?;
    for row in 0..output.nrows() {
        if selected(row, controls)? {
            let local_row = local_index(row, targets)?;
            for col in 0..output.ncols() {
                let mut value = Complex64::new(0.0, 0.0);
                for local_col in 0..operator.ncols() {
                    value = value.add(
                        operator[(local_row, local_col)]
                            .mul(previous[(replace_index(row, local_col, targets)?, col)]),
                    );
                }
                output[(row, col)] = value;
            }
        }
    }
    Ok(())
}
