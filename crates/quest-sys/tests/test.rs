use std::sync::Once;

use approx::{assert_abs_diff_eq, assert_relative_eq};
use quest_sys::{self, QuestComplex, QuestError, QuestResult};

static INIT: Once = Once::new();

fn ensure_quest_env() {
    INIT.call_once(|| {
        quest_sys::init_quest_env().expect("QuEST environment should initialise");
        unsafe {
            libc::atexit(finalize_quest_env_at_exit);
        }
    });

    assert!(quest_sys::is_quest_env_init());
}

extern "C" fn finalize_quest_env_at_exit() {
    let _ = quest_sys::finalize_quest_env();
}

fn complex(re: f64, im: f64) -> QuestComplex {
    QuestComplex { re, im }
}

#[test]
fn environment_api_uses_results_and_snake_case() -> QuestResult<()> {
    ensure_quest_env();

    let report = quest_sys::get_environment_string()?;

    assert!(!report.is_empty());
    Ok(())
}

#[test]
fn qureg_lifecycle_is_raii() -> QuestResult<()> {
    ensure_quest_env();

    let mut qureg = quest_sys::create_qureg(2)?;
    quest_sys::init_zero_state(qureg.pin_mut())?;

    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;

    assert_relative_eq!(amp0.re, 1.0);
    assert_relative_eq!(amp0.im, 0.0);
    assert_abs_diff_eq!(amp1.re, 0.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp1.im, 0.0, epsilon = 1e-12);

    drop(qureg);
    Ok(())
}

#[test]
fn finalize_fails_while_raii_handles_are_live() -> QuestResult<()> {
    ensure_quest_env();

    let qureg = quest_sys::create_qureg(1)?;
    let err = quest_sys::finalize_quest_env().expect_err("live handles should block finalize");

    assert!(quest_sys::is_quest_env_init());
    assert!(matches!(err, QuestError::Lifecycle(_)));
    assert!(err.to_string().contains("Qureg="));

    drop(qureg);
    Ok(())
}

#[test]
fn invalid_inputs_return_quest_errors() {
    ensure_quest_env();

    let err = match quest_sys::create_qureg(0) {
        Ok(_) => panic!("zero-qubit registers are invalid"),
        Err(err) => err,
    };

    assert!(!err.to_string().is_empty());
}

#[test]
fn arbitrary_pure_state_accepts_complex_slices() -> QuestResult<()> {
    ensure_quest_env();

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

    assert_relative_eq!(amp0.re, inv_sqrt_2, epsilon = 1e-12);
    assert_abs_diff_eq!(amp0.im, 0.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp3.re, 0.0, epsilon = 1e-12);
    assert_relative_eq!(amp3.im, inv_sqrt_2, epsilon = 1e-12);
    assert_relative_eq!(quest_sys::calc_total_prob(&qureg)?, 1.0, epsilon = 1e-12);

    Ok(())
}

#[test]
fn measurement_with_probability_returns_a_struct() -> QuestResult<()> {
    ensure_quest_env();

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_plus_state(qureg.pin_mut())?;

    let measurement = quest_sys::apply_qubit_measurement_and_get_prob(qureg.pin_mut(), 0)?;

    assert!(measurement.outcome == 0 || measurement.outcome == 1);
    assert_relative_eq!(measurement.probability, 0.5, epsilon = 1e-12);

    Ok(())
}

#[test]
fn nested_complex_matrix_inputs_are_flattened_safely() -> QuestResult<()> {
    ensure_quest_env();

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

    assert_abs_diff_eq!(amp0.re, 0.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp0.im, 0.0, epsilon = 1e-12);
    assert_relative_eq!(amp1.re, 1.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp1.im, 0.0, epsilon = 1e-12);

    Ok(())
}

#[test]
fn primitive_operations_are_result_wrapped() -> QuestResult<()> {
    ensure_quest_env();

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_zero_state(qureg.pin_mut())?;
    quest_sys::apply_pauli_x(qureg.pin_mut(), 0)?;

    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;

    assert_abs_diff_eq!(amp0.re, 0.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp0.im, 0.0, epsilon = 1e-12);
    assert_relative_eq!(amp1.re, 1.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp1.im, 0.0, epsilon = 1e-12);

    Ok(())
}

#[test]
fn generated_initialisation_and_probability_apis_work() -> QuestResult<()> {
    ensure_quest_env();

    let mut qureg = quest_sys::create_qureg(2)?;
    quest_sys::init_classical_state(qureg.pin_mut(), 2)?;

    assert_relative_eq!(
        quest_sys::calc_prob_of_basis_state(&qureg, 2)?,
        1.0,
        epsilon = 1e-12
    );
    assert_relative_eq!(
        quest_sys::calc_prob_of_qubit_outcome(&qureg, 1, 1)?,
        1.0,
        epsilon = 1e-12
    );

    let probs = quest_sys::calc_probs_of_all_multi_qubit_outcomes(&qureg, &[1])?;
    assert_eq!(probs.len(), 2);
    assert_relative_eq!(probs[0], 0.0, epsilon = 1e-12);
    assert_relative_eq!(probs[1], 1.0, epsilon = 1e-12);

    Ok(())
}

#[test]
fn generated_overloads_have_distinct_safe_adapters() -> QuestResult<()> {
    ensure_quest_env();

    let pauli_without_indices = quest_sys::get_pauli_str_from_string("Z")?;
    let pauli_with_indices = quest_sys::get_pauli_str("Z", &[0])?;

    quest_sys::report_pauli_str(&pauli_without_indices)?;
    quest_sys::report_pauli_str(&pauli_with_indices)?;
    quest_sys::report_scalar_real("real scalar", 1.0)?;
    quest_sys::report_scalar("complex scalar", complex(1.0, -0.5))?;

    Ok(())
}

#[test]
fn generated_diag_matrix_handles_are_raii_owned() -> QuestResult<()> {
    ensure_quest_env();

    let mut qureg = quest_sys::create_qureg(1)?;
    quest_sys::init_plus_state(qureg.pin_mut())?;

    let mut diag = quest_sys::create_diag_matr(1)?;
    quest_sys::set_diag_matr(diag.pin_mut(), &[complex(1.0, 0.0), complex(-1.0, 0.0)])?;
    quest_sys::apply_diag_matr(qureg.pin_mut(), &[0], &diag)?;

    let inv_sqrt_2 = 1.0 / 2.0_f64.sqrt();
    let amp0 = quest_sys::get_qureg_amp(&qureg, 0)?;
    let amp1 = quest_sys::get_qureg_amp(&qureg, 1)?;

    assert_relative_eq!(amp0.re, inv_sqrt_2, epsilon = 1e-12);
    assert_abs_diff_eq!(amp0.im, 0.0, epsilon = 1e-12);
    assert_relative_eq!(amp1.re, -inv_sqrt_2, epsilon = 1e-12);
    assert_abs_diff_eq!(amp1.im, 0.0, epsilon = 1e-12);

    drop(diag);
    Ok(())
}

#[test]
fn multiplication_api_uses_safe_matrix_wrappers() -> QuestResult<()> {
    ensure_quest_env();

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
    assert_relative_eq!(amp1.re, 1.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp1.im, 0.0, epsilon = 1e-12);

    Ok(())
}

#[test]
fn channel_handles_are_raii_owned() -> QuestResult<()> {
    ensure_quest_env();

    let super_op = quest_sys::create_super_op(1)?;
    let kraus_map = quest_sys::create_kraus_map(1, 1)?;

    drop(super_op);
    drop(kraus_map);
    Ok(())
}

#[test]
fn decoherence_api_returns_results() -> QuestResult<()> {
    ensure_quest_env();

    let mut density = quest_sys::create_density_qureg(1)?;
    quest_sys::init_plus_state(density.pin_mut())?;
    quest_sys::mix_dephasing(density.pin_mut(), 0, 0.1)?;

    assert_relative_eq!(quest_sys::calc_total_prob(&density)?, 1.0, epsilon = 1e-12);
    Ok(())
}

#[test]
fn trotterisation_accepts_raii_pauli_sums() -> QuestResult<()> {
    ensure_quest_env();

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
    assert_relative_eq!(amp0.re, 1.0, epsilon = 1e-12);
    assert_abs_diff_eq!(amp0.im, 0.0, epsilon = 1e-12);

    Ok(())
}
