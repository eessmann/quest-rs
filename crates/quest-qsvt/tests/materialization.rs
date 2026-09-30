use googletest::prelude::*;
use quest_circuit::{Angle, Control, ControlState, Gate, OracleFragment, QuantumRegionBuilder};
use quest_qsvt::{Complex64, NumericalPolicy, materialize_oracle};
use std::ops::Sub;

#[gtest]
fn independent_materialization_keeps_controlled_phase_and_adjoint_order() -> Result<()> {
    let mut body = QuantumRegionBuilder::new(2, 0)?;
    let q0 = body.qubit(0)?;
    let q1 = body.qubit(1)?;
    body.gate(Gate::H, &[q0], &[])?;
    body.gate(Gate::X, &[q1], &[Control::new(q0, ControlState::One)])?;
    body.global_phase(
        Angle::radians(0.37)?,
        &[Control::new(q1, ControlState::Zero)],
    )?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let matrix = materialize_oracle(&oracle, NumericalPolicy::default())?;
    let adjoint = materialize_oracle(&oracle.adjoint(), NumericalPolicy::default())?;
    let expected = Complex64::from_polar(std::f64::consts::FRAC_1_SQRT_2, 0.37);
    expect_that!(matrix[(0, 0)].sub(expected).norm(), lt(1e-14));
    expect_that!(
        matrix[(3, 0)].re,
        near(std::f64::consts::FRAC_1_SQRT_2, 1e-14)
    );
    for row in 0..4 {
        for col in 0..4 {
            expect_that!(
                adjoint[(row, col)].sub(matrix[(col, row)].conj()).norm(),
                lt(1e-14)
            );
        }
    }
    Ok(())
}
