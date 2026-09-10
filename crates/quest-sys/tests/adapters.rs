mod common;
use common::isolated;

use googletest::prelude::*;
use quest_sys::QuestComplex;

fn c(re: f64, im: f64) -> QuestComplex {
    QuestComplex { re, im }
}

#[gtest]
fn flat_matrix_preserves_complex_row_order_and_rejects_bad_dimensions() -> googletest::Result<()> {
    isolated(
        "flat_matrix_preserves_complex_row_order_and_rejects_bad_dimensions",
        || {
            quest_sys::init_custom_quest_env(false, false, false)?;
            let mut q = quest_sys::create_qureg(1)?;
            let mut matrix = quest_sys::create_comp_matr(1)?;
            quest_sys::set_comp_matr_flat(
                matrix.pin_mut(),
                &[c(0.0, 0.0), c(1.0, 0.0), c(0.0, 1.0), c(0.0, 0.0)],
                2,
            )?;
            quest_sys::init_zero_state(q.pin_mut())?;
            quest_sys::apply_comp_matr(q.pin_mut(), &[0], &matrix)?;
            let amp = quest_sys::get_qureg_amp(&q, 1)?;
            expect_that!(amp.re, near(0.0, 1e-12));
            expect_that!(amp.im, near(1.0, 1e-12));
            for rows in [-1, 0, 3, i64::MAX] {
                expect_that!(
                    quest_sys::set_comp_matr_flat(matrix.pin_mut(), &[], rows),
                    err(anything())
                );
            }
            expect_that!(
                quest_sys::set_comp_matr_flat(matrix.pin_mut(), &[c(1.0, 0.0); 3], 2),
                err(anything())
            );
            drop(matrix);
            drop(q);
            quest_sys::finalize_quest_env()?;
            Ok(())
        },
    )
}

#[gtest]
fn rectangular_density_transfers_preserve_rows_and_columns() -> googletest::Result<()> {
    isolated(
        "rectangular_density_transfers_preserve_rows_and_columns",
        || {
            quest_sys::init_custom_quest_env(false, false, false)?;
            let mut q = quest_sys::create_density_qureg(2)?;
            let values = [
                c(1.0, 2.0),
                c(3.0, 4.0),
                c(5.0, 6.0),
                c(7.0, 8.0),
                c(9.0, 10.0),
                c(11.0, 12.0),
            ];
            for (rows, cols) in [(2, 3), (3, 2)] {
                quest_sys::set_density_qureg_amps(q.pin_mut(), 1, 1, &values, rows, cols)?;
                expect_that!(
                    quest_sys::get_density_qureg_amps(&q, 1, 1, rows, cols)?,
                    eq(&values.to_vec())
                );
                expect_that!(quest_sys::get_density_qureg_amp(&q, 1, 1)?, eq(values[0]));
                expect_that!(
                    quest_sys::get_density_qureg_amp(&q, rows, cols)?,
                    eq(values[5])
                );
            }
            expect_that!(
                quest_sys::set_density_qureg_amps(q.pin_mut(), 0, 0, &values, 2, 2),
                err(anything())
            );
            for (row, col, rows, cols) in [
                (-1, 0, 1, 1),
                (0, 4, 1, 1),
                (0, 0, i64::MAX, i64::MAX),
                (0, 0, 0, 1),
            ] {
                expect_that!(
                    quest_sys::get_density_qureg_amps(&q, row, col, rows, cols),
                    err(anything())
                );
            }
            let mut state = quest_sys::create_qureg(2)?;
            expect_that!(
                quest_sys::set_density_qureg_amps(state.pin_mut(), 0, 0, &values, 2, 3),
                err(anything())
            );
            drop(state);
            drop(q);
            quest_sys::finalize_quest_env()?;
            Ok(())
        },
    )
}

#[gtest]
fn kraus_flat_matrices_apply_amplitude_damping() -> googletest::Result<()> {
    isolated("kraus_flat_matrices_apply_amplitude_damping", || {
        quest_sys::init_custom_quest_env(false, false, false)?;
        let mut q = quest_sys::create_density_qureg(1)?;
        quest_sys::init_classical_state(q.pin_mut(), 1)?;
        let mut map = quest_sys::create_kraus_map(1, 2)?;
        let values = [
            c(1.0, 0.0),
            c(0.0, 0.0),
            c(0.0, 0.0),
            c(0.5, 0.0),
            c(0.0, 0.0),
            c(0.75_f64.sqrt(), 0.0),
            c(0.0, 0.0),
            c(0.0, 0.0),
        ];
        quest_sys::set_kraus_map_flat(map.pin_mut(), &values, 2, 2)?;
        quest_sys::mix_kraus_map(q.pin_mut(), &[0], &map)?;
        expect_that!(
            quest_sys::get_density_qureg_amp(&q, 0, 0)?.re,
            near(0.75, 1e-12)
        );
        expect_that!(
            quest_sys::get_density_qureg_amp(&q, 1, 1)?.re,
            near(0.25, 1e-12)
        );
        expect_that!(
            quest_sys::set_kraus_map_flat(map.pin_mut(), &values[..7], 2, 2),
            err(anything())
        );
        expect_that!(
            quest_sys::set_kraus_map_flat(map.pin_mut(), &values, 3, 2),
            err(anything())
        );
        drop(map);
        drop(q);
        quest_sys::finalize_quest_env()?;
        Ok(())
    })
}

#[gtest]
fn explicit_seeds_replay_measurement_sequences() -> googletest::Result<()> {
    isolated("explicit_seeds_replay_measurement_sequences", || {
        quest_sys::init_custom_quest_env(false, false, false)?;
        let mut q = quest_sys::create_qureg(1)?;
        let mut sequences = [Vec::new(), Vec::new()];
        for sequence in &mut sequences {
            quest_sys::set_qu_est_seeds(&[17, 29, 41])?;
            expect_that!(quest_sys::get_qu_est_seeds()?, eq(&vec![17, 29, 41]));
            for _ in 0..24 {
                quest_sys::init_plus_state(q.pin_mut())?;
                sequence.push(quest_sys::apply_qubit_measurement(q.pin_mut(), 0)?);
            }
        }
        expect_that!(&sequences[0], eq(&sequences[1]));
        expect_that!(quest_sys::set_qu_est_seeds(&[]), err(anything()));
        drop(q);
        quest_sys::finalize_quest_env()?;
        Ok(())
    })
}

#[gtest]
fn global_phase_changes_statevector_and_preserves_density() -> googletest::Result<()> {
    isolated(
        "global_phase_changes_statevector_and_preserves_density",
        || {
            quest_sys::init_custom_quest_env(false, false, false)?;
            let mut q = quest_sys::create_qureg(1)?;
            let mut rho = quest_sys::create_density_qureg(1)?;
            quest_sys::init_zero_state(q.pin_mut())?;
            quest_sys::init_zero_state(rho.pin_mut())?;
            quest_sys::apply_global_phase(q.pin_mut(), std::f64::consts::FRAC_PI_2)?;
            quest_sys::apply_global_phase(rho.pin_mut(), std::f64::consts::FRAC_PI_2)?;
            expect_that!(quest_sys::get_qureg_amp(&q, 0)?.im, near(1.0, 1e-12));
            expect_that!(
                quest_sys::get_density_qureg_amp(&rho, 0, 0)?,
                eq(c(1.0, 0.0))
            );
            expect_that!(
                quest_sys::apply_global_phase(q.pin_mut(), f64::NAN),
                err(anything())
            );
            drop(rho);
            drop(q);
            quest_sys::finalize_quest_env()?;
            Ok(())
        },
    )
}

#[cfg(target_arch = "x86_64")]
#[gtest]
fn numerical_fingerprint_detects_ambient_simd_changes() -> googletest::Result<()> {
    isolated("numerical_fingerprint_detects_ambient_simd_changes", || {
        quest_sys::init_custom_quest_env(false, false, false)?;
        let before = quest_sys::get_numerical_fingerprint()?;
        // Restore the calling thread's control word even if an assertion fails.
        // SAFETY: MXCSR belongs to this test thread; only defined FTZ/DAZ
        // bits change, and Restore reinstates the original word on every exit.
        #[allow(deprecated)]
        unsafe {
            use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
            let original = _mm_getcsr();
            struct Restore(u32);
            impl Drop for Restore {
                fn drop(&mut self) {
                    // SAFETY: this word was read from this same thread before
                    // the temporary change; reserved bits are preserved.
                    unsafe { _mm_setcsr(self.0) };
                }
            }
            let _restore = Restore(original);
            _mm_setcsr(original | (1 << 15) | (1 << 6));
            let changed = quest_sys::get_numerical_fingerprint()?;
            expect_that!(changed.flush_to_zero, eq(true));
            expect_that!(changed.denormals_are_zero, eq(true));
            expect_that!(changed.underflow_control_supported, eq(true));
            expect_that!(changed, not(eq(before)));
        }
        expect_that!(quest_sys::get_numerical_fingerprint()?, eq(before));
        quest_sys::finalize_quest_env()?;
        Ok(())
    })
}
