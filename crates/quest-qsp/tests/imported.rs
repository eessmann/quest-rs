use googletest::prelude::*;
use quest_qsp::{ControlSequence, PhaseSequence, WxLaurent, WxSymmetric};

#[gtest]
fn symmetric_phase_admission_compares_actual_rotations_at_large_arguments() {
    let rounded_periods = std::f64::consts::TAU * 1_099_511_627_776.0;
    expect_true!(
        quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![0.0, rounded_periods])
            .build()
            .is_err()
    );
    expect_true!(
        quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![
            0.37,
            0.37 + std::f64::consts::TAU
        ])
        .build()
        .is_ok()
    );
    expect_true!(
        quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![f64::MAX, f64::MAX])
            .build()
            .is_ok()
    );
    expect_true!(
        quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![f64::MAX, -f64::MAX])
            .build()
            .is_err()
    );
}

#[gtest]
fn phase_convention_import_and_conversion_are_explicit() -> Result<()> {
    let phases = PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.3, 0.2]).build()?;
    expect_that!(phases.degree(), eq(2));
    expect_true!(
        PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.4])
            .build()
            .is_err()
    );
    let laurent = PhaseSequence::<WxLaurent>::builder(vec![0.0]).build()?;
    expect_that!(
        laurent.canonical().values()[0],
        eq(std::f64::consts::FRAC_PI_2)
    );
    Ok(())
}

#[gtest]
fn generalized_angle_import_keeps_the_final_k_matrix() -> Result<()> {
    let sequence = ControlSequence::builder().angles(&[0.0], &[0.0])?.build()?;
    let [matrix] = sequence.matrices() else {
        return fail!("one control required");
    };
    expect_that!(matrix[0][0].re, eq(0.0));
    expect_that!(matrix[0][1].re, eq(-1.0));
    expect_that!(matrix[1][0].re, eq(1.0));
    Ok(())
}
