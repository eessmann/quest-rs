use std::sync::Once;

use googletest::prelude::*;
use quest_sys::{self, QuestComplex, QuestError};

static INIT: Once = Once::new();
const EPSILON: f64 = 1e-12;

fn ensure_quest_env() -> googletest::Result<()> {
    INIT.call_once(|| {
        quest_sys::init_quest_env().expect("QuEST environment should initialise");
        unsafe {
            libc::atexit(finalize_quest_env_at_exit);
        }
    });

    verify_that!(quest_sys::is_quest_env_init(), eq(true))
}

extern "C" fn finalize_quest_env_at_exit() {
    let _ = quest_sys::finalize_quest_env();
}

fn complex(re: f64, im: f64) -> QuestComplex {
    QuestComplex { re, im }
}

fn expect_complex_near(actual: QuestComplex, re: f64, im: f64) {
    expect_that!(actual.re, near(re, EPSILON));
    expect_that!(actual.im, near(im, EPSILON));
}

#[gtest]
fn environment_api_uses_results_and_snake_case() -> googletest::Result<()> {
    ensure_quest_env()?;

    let report = quest_sys::get_environment_string()?;

    verify_that!(report.as_str(), not(eq("")))
}

#[gtest]
fn qureg_lifecycle_is_raii() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(2)?;
    quest_sys::init_zero_state(qureg.pin_mut())?;

    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;

    expect_complex_near(amp0, 1.0, 0.0);
    expect_complex_near(amp1, 0.0, 0.0);

    drop(qureg);
    Ok(())
}

#[gtest]
fn finalize_fails_while_raii_handles_are_live() -> googletest::Result<()> {
    ensure_quest_env()?;

    let qureg = quest_sys::create_qureg(1)?;
    let Err(err) = quest_sys::finalize_quest_env() else {
        return fail!("live handles should block finalize");
    };

    expect_that!(quest_sys::is_quest_env_init(), eq(true));
    verify_that!(&err, pat!(QuestError::Lifecycle(_)))?;
    verify_that!(err.to_string(), contains_substring("Qureg="))?;

    drop(qureg);
    Ok(())
}

#[gtest]
fn invalid_inputs_return_quest_errors() -> googletest::Result<()> {
    ensure_quest_env()?;

    let Err(err) = quest_sys::create_qureg(0) else {
        return fail!("zero-qubit registers are invalid");
    };

    verify_that!(err.to_string().as_str(), not(eq("")))
}

#[gtest]
fn arbitrary_pure_state_accepts_complex_slices() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(2)?;
    let inv_sqrt_2 = 1.0 / 2.0_f64.sqrt();
    let amps = [
        complex(inv_sqrt_2, 0.0),
        complex(0.0, 0.0),
        complex(0.0, 0.0),
        complex(0.0, inv_sqrt_2),
    ];

    quest_sys::init_arbitrary_pure_state(qureg.pin_mut(), &amps)?;

    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp3 = quest_sys::get_qureg_amp(&qureg, 3)?;

    expect_complex_near(amp0, inv_sqrt_2, 0.0);
    expect_complex_near(amp3, 0.0, inv_sqrt_2);
    expect_that!(quest_sys::calc_total_prob(&qureg)?, near(1.0, EPSILON));

    Ok(())
}

#[gtest]
fn measurement_with_probability_returns_a_struct() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_plus_state(qureg.pin_mut())?;

    let measurement = quest_sys::apply_qubit_measurement_and_get_prob(qureg.pin_mut(), 0)?;

    expect_that!(measurement.outcome, any!(eq(0), eq(1)));
    expect_that!(measurement.probability, near(0.5, EPSILON));

    Ok(())
}

#[gtest]
fn nested_complex_matrix_inputs_are_flattened_safely() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_zero_state(qureg.pin_mut())?;

    let matrix = [
        [complex(0.0, 0.0), complex(1.0, 0.0)],
        [complex(1.0, 0.0), complex(0.0, 0.0)],
    ];
    let rows = [&matrix[0][..], &matrix[1][..]];
    let mut comp_matr = quest_sys::create_comp_matr(1)?;

    quest_sys::set_comp_matr(comp_matr.pin_mut(), &rows)?;
    quest_sys::apply_comp_matr(qureg.pin_mut(), &[0], &comp_matr)?;

    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;

    expect_complex_near(amp0, 0.0, 0.0);
    expect_complex_near(amp1, 1.0, 0.0);

    Ok(())
}

#[gtest]
fn primitive_operations_are_result_wrapped() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_zero_state(qureg.pin_mut())?;
    quest_sys::apply_pauli_x(qureg.pin_mut(), 0)?;

    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;

    expect_complex_near(amp0, 0.0, 0.0);
    expect_complex_near(amp1, 1.0, 0.0);

    Ok(())
}

#[gtest]
fn generated_initialisation_and_probability_apis_work() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(2)?;
    quest_sys::init_classical_state(qureg.pin_mut(), 2)?;

    expect_that!(
        quest_sys::calc_prob_of_basis_state(&qureg, 2)?,
        near(1.0, EPSILON)
    );
    expect_that!(
        quest_sys::calc_prob_of_qubit_outcome(&qureg, 1, 1)?,
        near(1.0, EPSILON)
    );

    let probs = quest_sys::calc_probs_of_all_multi_qubit_outcomes(&qureg, &[1])?;
    verify_that!(probs.len(), eq(2))?;
    expect_that!(probs[0], near(0.0, EPSILON));
    expect_that!(probs[1], near(1.0, EPSILON));

    Ok(())
}

#[gtest]
fn generated_overloads_have_distinct_safe_adapters() -> googletest::Result<()> {
    ensure_quest_env()?;

    let pauli_without_indices = quest_sys::get_pauli_str_from_string("Z")?;
    let pauli_with_indices = quest_sys::get_pauli_str("Z", &[0])?;

    quest_sys::report_pauli_str(&pauli_without_indices)?;
    quest_sys::report_pauli_str(&pauli_with_indices)?;
    quest_sys::report_scalar_real("real scalar", 1.0)?;
    quest_sys::report_scalar("complex scalar", complex(1.0, -0.5))?;

    Ok(())
}

#[gtest]
fn generated_diag_matrix_handles_are_raii_owned() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_plus_state(qureg.pin_mut())?;

    let mut diag = quest_sys::create_diag_matr(1)?;
    quest_sys::set_diag_matr(diag.pin_mut(), &[complex(1.0, 0.0), complex(-1.0, 0.0)])?;
    quest_sys::apply_diag_matr(qureg.pin_mut(), &[0], &diag)?;

    let inv_sqrt_2 = 1.0 / 2.0_f64.sqrt();
    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;

    expect_complex_near(amp0, inv_sqrt_2, 0.0);
    expect_complex_near(amp1, -inv_sqrt_2, 0.0);

    drop(diag);
    Ok(())
}

#[gtest]
fn multiplication_api_uses_safe_matrix_wrappers() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_zero_state(qureg.pin_mut())?;

    let matrix = [
        [complex(0.0, 0.0), complex(1.0, 0.0)],
        [complex(1.0, 0.0), complex(0.0, 0.0)],
    ];
    let rows = [&matrix[0][..], &matrix[1][..]];
    let mut comp_matr = quest_sys::create_comp_matr(1)?;

    quest_sys::set_comp_matr(comp_matr.pin_mut(), &rows)?;
    quest_sys::leftapply_comp_matr(qureg.pin_mut(), &[0], &comp_matr)?;

    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;
    expect_complex_near(amp1, 1.0, 0.0);

    Ok(())
}

#[gtest]
fn channel_handles_are_raii_owned() -> googletest::Result<()> {
    ensure_quest_env()?;

    let super_op = quest_sys::create_super_op(1)?;
    let kraus_map = quest_sys::create_kraus_map(1, 1)?;

    drop(super_op);
    drop(kraus_map);
    Ok(())
}

#[gtest]
fn decoherence_api_returns_results() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut density = quest_sys::create_density_qureg(1)?;
    quest_sys::init_plus_state(density.pin_mut())?;
    quest_sys::mix_dephasing(density.pin_mut(), 0, 0.1)?;

    verify_that!(quest_sys::calc_total_prob(&density)?, near(1.0, EPSILON))?;
    Ok(())
}

#[gtest]
fn trotterisation_accepts_raii_pauli_sums() -> googletest::Result<()> {
    ensure_quest_env()?;

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_zero_state(qureg.pin_mut())?;
    let hamiltonian = quest_sys::create_inline_pauli_str_sum("1 Z")?;

    quest_sys::apply_trotterized_unitary_time_evolution(
        qureg.pin_mut(),
        &hamiltonian,
        0.0,
        1,
        1,
        false,
    )?;

    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    expect_complex_near(amp0, 1.0, 0.0);

    Ok(())
}
