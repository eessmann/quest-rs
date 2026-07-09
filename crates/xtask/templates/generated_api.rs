#![allow(clippy::too_many_arguments)]

use crate::{
    CompMatr, CompMatr1, CompMatr2, DiagMatr, DiagMatr1, DiagMatr2, FullStateDiagMatr, KrausMap,
    PauliStr, PauliStrSum, QuestComplex, QuestResult, Qureg, SuperOp, map_quest_result,
};
use cxx::UniquePtr;
use std::pin::Pin;

#[cxx::bridge(namespace = "quest_sys")]
mod ffi {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct GeneratedComplex {
        pub re: f64,
        pub im: f64,
    }

    unsafe extern "C++" {
        include!("quest_generated_bindings.hpp");

        type Qureg = crate::ffi::Qureg;
        type CompMatr1 = crate::ffi::CompMatr1;
        type CompMatr2 = crate::ffi::CompMatr2;
        type CompMatr = crate::ffi::CompMatr;
        type DiagMatr1 = crate::ffi::DiagMatr1;
        type DiagMatr2 = crate::ffi::DiagMatr2;
        type DiagMatr = crate::ffi::DiagMatr;
        type FullStateDiagMatr = crate::ffi::FullStateDiagMatr;
        type KrausMap = crate::ffi::KrausMap;
        type SuperOp = crate::ffi::SuperOp;
        type PauliStr = crate::ffi::PauliStr;
        type PauliStrSum = crate::ffi::PauliStrSum;

        fn apply_comp_matr1(qureg: Pin<&mut Qureg>, target: i32, matrix: &CompMatr1) -> Result<()>;
        fn apply_comp_matr2(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            matrix: &CompMatr2,
        ) -> Result<()>;
        fn apply_controlled_comp_matr(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            matr: &CompMatr,
        ) -> Result<()>;
        fn apply_controlled_comp_matr1(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
            matrix: &CompMatr1,
        ) -> Result<()>;
        fn apply_controlled_comp_matr2(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target1: i32,
            target2: i32,
            matr: &CompMatr2,
        ) -> Result<()>;
        fn apply_controlled_diag_matr(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            matrix: &DiagMatr,
        ) -> Result<()>;
        fn apply_controlled_diag_matr1(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
            matr: &DiagMatr1,
        ) -> Result<()>;
        fn apply_controlled_diag_matr2(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        ) -> Result<()>;
        fn apply_controlled_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn apply_controlled_hadamard(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
        ) -> Result<()>;
        fn apply_controlled_multi_qubit_not(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
        ) -> Result<()>;
        fn apply_controlled_pauli_gadget(
            qureg: Pin<&mut Qureg>,
            control: i32,
            str_arg: &PauliStr,
            angle: f64,
        ) -> Result<()>;
        fn apply_controlled_pauli_str(
            qureg: Pin<&mut Qureg>,
            control: i32,
            str_arg: &PauliStr,
        ) -> Result<()>;
        fn apply_controlled_pauli_x(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
        ) -> Result<()>;
        fn apply_controlled_pauli_y(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
        ) -> Result<()>;
        fn apply_controlled_pauli_z(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
        ) -> Result<()>;
        fn apply_controlled_phase_gadget(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            angle: f64,
        ) -> Result<()>;
        fn apply_controlled_rotate_around_axis(
            qureg: Pin<&mut Qureg>,
            ctrl: i32,
            targ: i32,
            angle: f64,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
        ) -> Result<()>;
        fn apply_controlled_rotate_x(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_controlled_rotate_y(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_controlled_rotate_z(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_controlled_s(qureg: Pin<&mut Qureg>, control: i32, target: i32) -> Result<()>;
        fn apply_controlled_sqrt_swap(
            qureg: Pin<&mut Qureg>,
            control: i32,
            qubit1: i32,
            qubit2: i32,
        ) -> Result<()>;
        fn apply_controlled_swap(
            qureg: Pin<&mut Qureg>,
            control: i32,
            qubit1: i32,
            qubit2: i32,
        ) -> Result<()>;
        fn apply_controlled_t(qureg: Pin<&mut Qureg>, control: i32, target: i32) -> Result<()>;
        fn apply_diag_matr(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
        ) -> Result<()>;
        fn apply_diag_matr1(qureg: Pin<&mut Qureg>, target: i32, matr: &DiagMatr1) -> Result<()>;
        fn apply_diag_matr2(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        ) -> Result<()>;
        fn apply_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn apply_forced_multi_qubit_measurement(
            qureg: Pin<&mut Qureg>,
            qubits: &[i32],
            outcomes: &[i32],
        ) -> Result<f64>;
        fn apply_forced_qubit_measurement(
            qureg: Pin<&mut Qureg>,
            target: i32,
            outcome: i32,
        ) -> Result<f64>;
        fn apply_full_quantum_fourier_transform(
            qureg: Pin<&mut Qureg>,
            inverse: bool,
        ) -> Result<()>;
        fn apply_full_state_diag_matr(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
        ) -> Result<()>;
        fn apply_full_state_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn apply_multi_controlled_comp_matr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            matr: &CompMatr,
        ) -> Result<()>;
        fn apply_multi_controlled_comp_matr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            matrix: &CompMatr1,
        ) -> Result<()>;
        fn apply_multi_controlled_comp_matr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target1: i32,
            target2: i32,
            matr: &CompMatr2,
        ) -> Result<()>;
        fn apply_multi_controlled_diag_matr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
        ) -> Result<()>;
        fn apply_multi_controlled_diag_matr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            matr: &DiagMatr1,
        ) -> Result<()>;
        fn apply_multi_controlled_diag_matr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        ) -> Result<()>;
        fn apply_multi_controlled_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn apply_multi_controlled_hadamard(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_controlled_multi_qubit_not(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
        ) -> Result<()>;
        fn apply_multi_controlled_pauli_gadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            str_arg: &PauliStr,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_controlled_pauli_str(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            str_arg: &PauliStr,
        ) -> Result<()>;
        fn apply_multi_controlled_pauli_x(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_controlled_pauli_y(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_controlled_pauli_z(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_controlled_phase_gadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_controlled_rotate_around_axis(
            qureg: Pin<&mut Qureg>,
            ctrls: &[i32],
            targ: i32,
            angle: f64,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
        ) -> Result<()>;
        fn apply_multi_controlled_rotate_x(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_controlled_rotate_y(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_controlled_rotate_z(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_controlled_s(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_controlled_sqrt_swap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            qubit1: i32,
            qubit2: i32,
        ) -> Result<()>;
        fn apply_multi_controlled_swap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            qubit1: i32,
            qubit2: i32,
        ) -> Result<()>;
        fn apply_multi_controlled_t(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_qubit_measurement(qureg: Pin<&mut Qureg>, qubits: &[i32]) -> Result<i64>;
        fn apply_multi_qubit_not(qureg: Pin<&mut Qureg>, targets: &[i32]) -> Result<()>;
        fn apply_multi_qubit_phase_flip(qureg: Pin<&mut Qureg>, targets: &[i32]) -> Result<()>;
        fn apply_multi_qubit_phase_shift(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_qubit_projector(
            qureg: Pin<&mut Qureg>,
            qubits: &[i32],
            outcomes: &[i32],
        ) -> Result<()>;
        fn apply_multi_state_controlled_comp_matr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            matr: &CompMatr,
        ) -> Result<()>;
        fn apply_multi_state_controlled_comp_matr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            matrix: &CompMatr1,
        ) -> Result<()>;
        fn apply_multi_state_controlled_comp_matr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target1: i32,
            target2: i32,
            matr: &CompMatr2,
        ) -> Result<()>;
        fn apply_multi_state_controlled_diag_matr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
        ) -> Result<()>;
        fn apply_multi_state_controlled_diag_matr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            matr: &DiagMatr1,
        ) -> Result<()>;
        fn apply_multi_state_controlled_diag_matr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        ) -> Result<()>;
        fn apply_multi_state_controlled_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn apply_multi_state_controlled_hadamard(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_state_controlled_multi_qubit_not(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
        ) -> Result<()>;
        fn apply_multi_state_controlled_pauli_gadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            str_arg: &PauliStr,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_state_controlled_pauli_str(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            str_arg: &PauliStr,
        ) -> Result<()>;
        fn apply_multi_state_controlled_pauli_x(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_state_controlled_pauli_y(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_state_controlled_pauli_z(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_state_controlled_phase_gadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_state_controlled_rotate_around_axis(
            qureg: Pin<&mut Qureg>,
            ctrls: &[i32],
            states: &[i32],
            targ: i32,
            angle: f64,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
        ) -> Result<()>;
        fn apply_multi_state_controlled_rotate_x(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_state_controlled_rotate_y(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_state_controlled_rotate_z(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            angle: f64,
        ) -> Result<()>;
        fn apply_multi_state_controlled_s(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_multi_state_controlled_sqrt_swap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            qubit1: i32,
            qubit2: i32,
        ) -> Result<()>;
        fn apply_multi_state_controlled_swap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            qubit1: i32,
            qubit2: i32,
        ) -> Result<()>;
        fn apply_multi_state_controlled_t(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        ) -> Result<()>;
        fn apply_non_unitary_pauli_gadget(
            qureg: Pin<&mut Qureg>,
            str_arg: &PauliStr,
            angle: GeneratedComplex,
        ) -> Result<()>;
        fn apply_pauli_gadget(qureg: Pin<&mut Qureg>, str_arg: &PauliStr, angle: f64)
        -> Result<()>;
        fn apply_pauli_str(qureg: Pin<&mut Qureg>, str_arg: &PauliStr) -> Result<()>;
        fn apply_phase_flip(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn apply_phase_gadget(qureg: Pin<&mut Qureg>, targets: &[i32], angle: f64) -> Result<()>;
        fn apply_phase_shift(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> Result<()>;
        fn apply_quantum_fourier_transform(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            inverse: bool,
        ) -> Result<()>;
        fn apply_qubit_projector(qureg: Pin<&mut Qureg>, target: i32, outcome: i32) -> Result<()>;
        fn apply_rotate_around_axis(
            qureg: Pin<&mut Qureg>,
            target: i32,
            angle: f64,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
        ) -> Result<()>;
        fn apply_rotate_x(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> Result<()>;
        fn apply_rotate_y(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> Result<()>;
        fn apply_rotate_z(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> Result<()>;
        fn apply_s(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn apply_sqrt_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> Result<()>;
        fn apply_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> Result<()>;
        fn apply_t(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn apply_trotterized_controlled_pauli_str_sum_gadget(
            qureg: Pin<&mut Qureg>,
            control: i32,
            sum: &PauliStrSum,
            angle: f64,
            order: i32,
            reps: i32,
            permute_terms: bool,
        ) -> Result<()>;
        fn apply_trotterized_imaginary_time_evolution(
            qureg: Pin<&mut Qureg>,
            hamil: &PauliStrSum,
            tau: f64,
            order: i32,
            reps: i32,
            permute_terms: bool,
        ) -> Result<()>;
        fn apply_trotterized_multi_controlled_pauli_str_sum_gadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            sum: &PauliStrSum,
            angle: f64,
            order: i32,
            reps: i32,
            permute_terms: bool,
        ) -> Result<()>;
        fn apply_trotterized_multi_state_controlled_pauli_str_sum_gadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            sum: &PauliStrSum,
            angle: f64,
            order: i32,
            reps: i32,
            permute_terms: bool,
        ) -> Result<()>;
        fn apply_trotterized_non_unitary_pauli_str_sum_gadget(
            qureg: Pin<&mut Qureg>,
            sum: &PauliStrSum,
            angle: GeneratedComplex,
            order: i32,
            reps: i32,
            permute_terms: bool,
        ) -> Result<()>;
        fn apply_trotterized_pauli_str_sum_gadget(
            qureg: Pin<&mut Qureg>,
            sum: &PauliStrSum,
            angle: f64,
            order: i32,
            reps: i32,
            permute_terms: bool,
        ) -> Result<()>;
        fn apply_two_qubit_phase_flip(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
        ) -> Result<()>;
        fn apply_two_qubit_phase_shift(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            angle: f64,
        ) -> Result<()>;
        fn calc_expec_full_state_diag_matr(qureg: &Qureg, matr: &FullStateDiagMatr) -> Result<f64>;
        fn calc_expec_full_state_diag_matr_power(
            qureg: &Qureg,
            matrix: &FullStateDiagMatr,
            exponent: f64,
        ) -> Result<f64>;
        fn calc_expec_non_hermitian_full_state_diag_matr(
            qureg: &Qureg,
            matr: &FullStateDiagMatr,
        ) -> Result<GeneratedComplex>;
        fn calc_expec_non_hermitian_full_state_diag_matr_power(
            qureg: &Qureg,
            matrix: &FullStateDiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<GeneratedComplex>;
        fn calc_expec_non_hermitian_pauli_str_sum(
            qureg: &Qureg,
            sum: &PauliStrSum,
        ) -> Result<GeneratedComplex>;
        fn calc_expec_pauli_str(qureg: &Qureg, str_arg: &PauliStr) -> Result<f64>;
        fn calc_expec_pauli_str_sum(qureg: &Qureg, sum: &PauliStrSum) -> Result<f64>;
        fn calc_fidelity(qureg: &Qureg, other: &Qureg) -> Result<f64>;
        fn calc_inner_product(qureg: &Qureg, other: &Qureg) -> Result<GeneratedComplex>;
        fn calc_partial_trace(qureg: &Qureg, trace_out_qubits: &[i32]) -> Result<UniquePtr<Qureg>>;
        fn calc_prob_of_basis_state(qureg: &Qureg, index: i64) -> Result<f64>;
        fn calc_prob_of_multi_qubit_outcome(
            qureg: &Qureg,
            qubits: &[i32],
            outcomes: &[i32],
        ) -> Result<f64>;
        fn calc_prob_of_qubit_outcome(qureg: &Qureg, qubit: i32, outcome: i32) -> Result<f64>;
        fn calc_probs_of_all_multi_qubit_outcomes(
            qureg: &Qureg,
            qubits: &[i32],
        ) -> Result<Vec<f64>>;
        fn calc_purity(qureg: &Qureg) -> Result<f64>;
        fn calc_reduced_density_matrix(
            qureg: &Qureg,
            retain_qubits: &[i32],
        ) -> Result<UniquePtr<Qureg>>;
        fn clear_qu_est_gpu_cache() -> Result<()>;
        fn create_clone_qureg(qureg: &Qureg) -> Result<UniquePtr<Qureg>>;
        fn create_custom_full_state_diag_matr(
            num_qubits: i32,
            use_distrib: i32,
            use_gpu_accel: i32,
            use_multithread: i32,
        ) -> Result<UniquePtr<FullStateDiagMatr>>;
        fn create_custom_qureg(
            num_qubits: i32,
            is_dens_matr: i32,
            use_distrib: i32,
            use_gpu_accel: i32,
            use_multithread: i32,
        ) -> Result<UniquePtr<Qureg>>;
        fn create_diag_matr(num_qubits: i32) -> Result<UniquePtr<DiagMatr>>;
        fn create_forced_density_qureg(num_qubits: i32) -> Result<UniquePtr<Qureg>>;
        fn create_forced_qureg(num_qubits: i32) -> Result<UniquePtr<Qureg>>;
        fn create_full_state_diag_matr(num_qubits: i32) -> Result<UniquePtr<FullStateDiagMatr>>;
        fn create_full_state_diag_matr_from_pauli_str_sum(
            in_arg: &PauliStrSum,
        ) -> Result<UniquePtr<FullStateDiagMatr>>;
        fn create_inline_diag_matr(
            num_qb: i32,
            elems: &[GeneratedComplex],
        ) -> Result<UniquePtr<DiagMatr>>;
        fn create_pauli_str_sum_from_file(file_name: &str) -> Result<UniquePtr<PauliStrSum>>;
        fn create_pauli_str_sum_from_reversed_file(
            file_name: &str,
        ) -> Result<UniquePtr<PauliStrSum>>;
        fn create_qureg_from_file(file_name: &str) -> Result<UniquePtr<Qureg>>;
        fn get_density_qureg_amp(qureg: &Qureg, row: i64, column: i64) -> Result<GeneratedComplex>;
        fn get_diag_matr1(in_arg: &[GeneratedComplex]) -> Result<UniquePtr<DiagMatr1>>;
        fn get_diag_matr2(in_arg: &[GeneratedComplex]) -> Result<UniquePtr<DiagMatr2>>;
        fn get_pauli_str_from_string(paulis: &str) -> Result<UniquePtr<PauliStr>>;
        fn get_pauli_str(paulis: &str, indices: &[i32]) -> Result<UniquePtr<PauliStr>>;
        fn get_qu_est_gpu_cache_size() -> Result<i64>;
        fn get_qu_est_num_gpu_threads_per_block() -> Result<i32>;
        fn get_qu_est_num_seeds() -> Result<i32>;
        fn get_qu_est_validation_epsilon() -> Result<f64>;
        fn init_blank_state(qureg: Pin<&mut Qureg>) -> Result<()>;
        fn init_classical_state(qureg: Pin<&mut Qureg>, state_ind: i64) -> Result<()>;
        fn init_debug_state(qureg: Pin<&mut Qureg>) -> Result<()>;
        fn init_pure_state(qureg: Pin<&mut Qureg>, pure: &Qureg) -> Result<()>;
        fn init_random_mixed_state(qureg: Pin<&mut Qureg>, num_pure_states: i64) -> Result<()>;
        fn init_random_pure_state(qureg: Pin<&mut Qureg>) -> Result<()>;
        fn leftapply_comp_matr1(
            qureg: Pin<&mut Qureg>,
            target: i32,
            matrix: &CompMatr1,
        ) -> Result<()>;
        fn leftapply_comp_matr2(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            matr: &CompMatr2,
        ) -> Result<()>;
        fn leftapply_diag_matr(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
        ) -> Result<()>;
        fn leftapply_diag_matr1(
            qureg: Pin<&mut Qureg>,
            target: i32,
            matr: &DiagMatr1,
        ) -> Result<()>;
        fn leftapply_diag_matr2(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        ) -> Result<()>;
        fn leftapply_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn leftapply_full_state_diag_matr(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
        ) -> Result<()>;
        fn leftapply_full_state_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn leftapply_multi_qubit_not(qureg: Pin<&mut Qureg>, targets: &[i32]) -> Result<()>;
        fn leftapply_multi_qubit_projector(
            qureg: Pin<&mut Qureg>,
            qubits: &[i32],
            outcomes: &[i32],
        ) -> Result<()>;
        fn leftapply_pauli_gadget(
            qureg: Pin<&mut Qureg>,
            str_arg: &PauliStr,
            angle: f64,
        ) -> Result<()>;
        fn leftapply_pauli_str(qureg: Pin<&mut Qureg>, str_arg: &PauliStr) -> Result<()>;
        fn leftapply_pauli_str_sum(
            qureg: Pin<&mut Qureg>,
            sum: &PauliStrSum,
            workspace: &Qureg,
        ) -> Result<()>;
        fn leftapply_pauli_x(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn leftapply_pauli_y(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn leftapply_pauli_z(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn leftapply_phase_gadget(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            angle: f64,
        ) -> Result<()>;
        fn leftapply_qubit_projector(
            qureg: Pin<&mut Qureg>,
            qubit: i32,
            outcome: i32,
        ) -> Result<()>;
        fn leftapply_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> Result<()>;
        fn mix_damping(qureg: Pin<&mut Qureg>, target: i32, prob: f64) -> Result<()>;
        fn mix_depolarising(qureg: Pin<&mut Qureg>, target: i32, prob: f64) -> Result<()>;
        fn mix_kraus_map(qureg: Pin<&mut Qureg>, targets: &[i32], map: &KrausMap) -> Result<()>;
        fn mix_paulis(
            qureg: Pin<&mut Qureg>,
            target: i32,
            prob_x: f64,
            prob_y: f64,
            prob_z: f64,
        ) -> Result<()>;
        fn mix_qureg(qureg: Pin<&mut Qureg>, other: &Qureg, prob: f64) -> Result<()>;
        fn mix_super_op(qureg: Pin<&mut Qureg>, targets: &[i32], superop: &SuperOp) -> Result<()>;
        fn mix_two_qubit_dephasing(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            prob: f64,
        ) -> Result<()>;
        fn mix_two_qubit_depolarising(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            prob: f64,
        ) -> Result<()>;
        fn report_comp_matr(matrix: &CompMatr) -> Result<()>;
        fn report_comp_matr1(matrix: &CompMatr1) -> Result<()>;
        fn report_comp_matr2(matrix: &CompMatr2) -> Result<()>;
        fn report_diag_matr(matrix: &DiagMatr) -> Result<()>;
        fn report_diag_matr1(matrix: &DiagMatr1) -> Result<()>;
        fn report_diag_matr2(matrix: &DiagMatr2) -> Result<()>;
        fn report_full_state_diag_matr(matr: &FullStateDiagMatr) -> Result<()>;
        fn report_kraus_map(map: &KrausMap) -> Result<()>;
        fn report_pauli_str(str_arg: &PauliStr) -> Result<()>;
        fn report_pauli_str_sum(str_arg: &PauliStrSum) -> Result<()>;
        fn report_qureg(qureg: &Qureg) -> Result<()>;
        fn report_qureg_params(qureg: &Qureg) -> Result<()>;
        fn report_scalar_real(label: &str, num: f64) -> Result<()>;
        fn report_scalar(label: &str, num: GeneratedComplex) -> Result<()>;
        fn report_str(str_arg: &str) -> Result<()>;
        fn report_super_op(op: &SuperOp) -> Result<()>;
        fn rightapply_comp_matr1(
            qureg: Pin<&mut Qureg>,
            target: i32,
            matrix: &CompMatr1,
        ) -> Result<()>;
        fn rightapply_comp_matr2(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            matrix: &CompMatr2,
        ) -> Result<()>;
        fn rightapply_diag_matr(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
        ) -> Result<()>;
        fn rightapply_diag_matr1(
            qureg: Pin<&mut Qureg>,
            target: i32,
            matrix: &DiagMatr1,
        ) -> Result<()>;
        fn rightapply_diag_matr2(
            qureg: Pin<&mut Qureg>,
            target1: i32,
            target2: i32,
            matrix: &DiagMatr2,
        ) -> Result<()>;
        fn rightapply_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn rightapply_full_state_diag_matr(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
        ) -> Result<()>;
        fn rightapply_full_state_diag_matr_power(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
            exponent: GeneratedComplex,
        ) -> Result<()>;
        fn rightapply_multi_qubit_not(qureg: Pin<&mut Qureg>, targets: &[i32]) -> Result<()>;
        fn rightapply_multi_qubit_projector(
            qureg: Pin<&mut Qureg>,
            qubits: &[i32],
            outcomes: &[i32],
        ) -> Result<()>;
        fn rightapply_pauli_gadget(
            qureg: Pin<&mut Qureg>,
            str_arg: &PauliStr,
            angle: f64,
        ) -> Result<()>;
        fn rightapply_pauli_str(qureg: Pin<&mut Qureg>, str_arg: &PauliStr) -> Result<()>;
        fn rightapply_pauli_str_sum(
            qureg: Pin<&mut Qureg>,
            sum: &PauliStrSum,
            workspace: &Qureg,
        ) -> Result<()>;
        fn rightapply_pauli_x(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn rightapply_pauli_y(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn rightapply_pauli_z(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn rightapply_phase_gadget(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            angle: f64,
        ) -> Result<()>;
        fn rightapply_qubit_projector(
            qureg: Pin<&mut Qureg>,
            qubit: i32,
            outcome: i32,
        ) -> Result<()>;
        fn rightapply_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> Result<()>;
        fn save_qureg_to_file(qureg: Pin<&mut Qureg>, arg1: &str) -> Result<()>;
        fn set_density_qureg_flat_amps(
            qureg: Pin<&mut Qureg>,
            start_ind: i64,
            amps: &[GeneratedComplex],
        ) -> Result<()>;
        fn set_diag_matr(out_arg: Pin<&mut DiagMatr>, in_arg: &[GeneratedComplex]) -> Result<()>;
        fn set_full_state_diag_matr(
            out_arg: Pin<&mut FullStateDiagMatr>,
            start_ind: i64,
            in_arg: &[GeneratedComplex],
        ) -> Result<()>;
        fn set_full_state_diag_matr_from_pauli_str_sum(
            out_arg: Pin<&mut FullStateDiagMatr>,
            in_arg: &PauliStrSum,
        ) -> Result<()>;
        fn set_inline_diag_matr(
            matr: Pin<&mut DiagMatr>,
            num_qb: i32,
            in_arg: &[GeneratedComplex],
        ) -> Result<()>;
        fn set_inline_full_state_diag_matr(
            matr: Pin<&mut FullStateDiagMatr>,
            start_ind: i64,
            num_elems: i64,
            in_arg: &[GeneratedComplex],
        ) -> Result<()>;
        fn set_qu_est_max_num_reported_items(num_rows: i64, num_cols: i64) -> Result<()>;
        fn set_qu_est_max_num_reported_sig_figs(num_sig_figs: i32) -> Result<()>;
        fn set_qu_est_num_gpu_threads_per_block(num_threads_per_block: i32) -> Result<()>;
        fn set_qu_est_num_reported_newlines(num_newlines: i32) -> Result<()>;
        fn set_qu_est_reported_pauli_chars(paulis: &str) -> Result<()>;
        fn set_qu_est_reported_pauli_str_style(style: i32) -> Result<()>;
        fn set_qu_est_seeds_to_default() -> Result<()>;
        fn set_qu_est_validation_epsilon(eps: f64) -> Result<()>;
        fn set_qu_est_validation_epsilon_to_default() -> Result<()>;
        fn set_qu_est_validation_off() -> Result<()>;
        fn set_qu_est_validation_on() -> Result<()>;
        fn set_qureg_amps(
            qureg: Pin<&mut Qureg>,
            start_ind: i64,
            amps: &[GeneratedComplex],
        ) -> Result<()>;
        fn set_qureg_to_clone(out_qureg: Pin<&mut Qureg>, in_qureg: &Qureg) -> Result<()>;
        fn set_qureg_to_partial_trace(
            out_arg: Pin<&mut Qureg>,
            in_arg: &Qureg,
            trace_out_qubits: &[i32],
        ) -> Result<()>;
        fn set_qureg_to_pauli_str_sum(qureg: Pin<&mut Qureg>, sum: &PauliStrSum) -> Result<()>;
        fn set_qureg_to_reduced_density_matrix(
            out_arg: Pin<&mut Qureg>,
            in_arg: &Qureg,
            retain_qubits: &[i32],
        ) -> Result<()>;
        fn set_qureg_to_renormalized(qureg: Pin<&mut Qureg>) -> Result<f64>;
        fn sort_pauli_str_sum_lexicographic(sum: Pin<&mut PauliStrSum>) -> Result<()>;
        fn sort_pauli_str_sum_magnitude(sum: Pin<&mut PauliStrSum>) -> Result<()>;
        fn sync_comp_matr(matr: Pin<&mut CompMatr>) -> Result<()>;
        fn sync_diag_matr(matr: Pin<&mut DiagMatr>) -> Result<()>;
        fn sync_full_state_diag_matr(matr: Pin<&mut FullStateDiagMatr>) -> Result<()>;
        fn sync_kraus_map(map: Pin<&mut KrausMap>) -> Result<()>;
        fn sync_qureg_from_gpu(qureg: Pin<&mut Qureg>) -> Result<()>;
        fn sync_qureg_to_gpu(qureg: Pin<&mut Qureg>) -> Result<()>;
        fn sync_sub_qureg_from_gpu(
            qureg: Pin<&mut Qureg>,
            local_start_ind: i64,
            num_local_amps: i64,
        ) -> Result<()>;
        fn sync_sub_qureg_to_gpu(
            qureg: Pin<&mut Qureg>,
            local_start_ind: i64,
            num_local_amps: i64,
        ) -> Result<()>;
        fn sync_super_op(op: Pin<&mut SuperOp>) -> Result<()>;
    }
}

use ffi::GeneratedComplex;

impl From<QuestComplex> for GeneratedComplex {
    fn from(value: QuestComplex) -> Self {
        Self {
            re: value.re,
            im: value.im,
        }
    }
}

impl From<GeneratedComplex> for QuestComplex {
    fn from(value: GeneratedComplex) -> Self {
        Self {
            re: value.re,
            im: value.im,
        }
    }
}

pub fn apply_comp_matr1(
    qureg: Pin<&mut Qureg>,
    target: i32,
    matrix: &CompMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_comp_matr1(qureg, target, matrix))
}

pub fn apply_comp_matr2(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    matrix: &CompMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_comp_matr2(qureg, target1, target2, matrix))
}

pub fn apply_controlled_comp_matr(
    qureg: Pin<&mut Qureg>,
    control: i32,
    targets: &[i32],
    matr: &CompMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_comp_matr(
        qureg, control, targets, matr,
    ))
}

pub fn apply_controlled_comp_matr1(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
    matrix: &CompMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_comp_matr1(
        qureg, control, target, matrix,
    ))
}

pub fn apply_controlled_comp_matr2(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target1: i32,
    target2: i32,
    matr: &CompMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_comp_matr2(
        qureg, control, target1, target2, matr,
    ))
}

pub fn apply_controlled_diag_matr(
    qureg: Pin<&mut Qureg>,
    control: i32,
    targets: &[i32],
    matrix: &DiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_diag_matr(
        qureg, control, targets, matrix,
    ))
}

pub fn apply_controlled_diag_matr1(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
    matr: &DiagMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_diag_matr1(
        qureg, control, target, matr,
    ))
}

pub fn apply_controlled_diag_matr2(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target1: i32,
    target2: i32,
    matr: &DiagMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_diag_matr2(
        qureg, control, target1, target2, matr,
    ))
}

pub fn apply_controlled_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    control: i32,
    targets: &[i32],
    matrix: &DiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::apply_controlled_diag_matr_power(
        qureg,
        control,
        targets,
        matrix,
        exponent_ffi,
    ))
}

pub fn apply_controlled_hadamard(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_hadamard(qureg, control, target))
}

pub fn apply_controlled_multi_qubit_not(
    qureg: Pin<&mut Qureg>,
    control: i32,
    targets: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_multi_qubit_not(
        qureg, control, targets,
    ))
}

pub fn apply_controlled_pauli_gadget(
    qureg: Pin<&mut Qureg>,
    control: i32,
    str_arg: &PauliStr,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_pauli_gadget(
        qureg, control, str_arg, angle,
    ))
}

pub fn apply_controlled_pauli_str(
    qureg: Pin<&mut Qureg>,
    control: i32,
    str_arg: &PauliStr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_pauli_str(qureg, control, str_arg))
}

pub fn apply_controlled_pauli_x(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_pauli_x(qureg, control, target))
}

pub fn apply_controlled_pauli_y(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_pauli_y(qureg, control, target))
}

pub fn apply_controlled_pauli_z(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_pauli_z(qureg, control, target))
}

pub fn apply_controlled_phase_gadget(
    qureg: Pin<&mut Qureg>,
    control: i32,
    targets: &[i32],
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_phase_gadget(
        qureg, control, targets, angle,
    ))
}

pub fn apply_controlled_rotate_around_axis(
    qureg: Pin<&mut Qureg>,
    ctrl: i32,
    targ: i32,
    angle: f64,
    axis_x: f64,
    axis_y: f64,
    axis_z: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_rotate_around_axis(
        qureg, ctrl, targ, angle, axis_x, axis_y, axis_z,
    ))
}

pub fn apply_controlled_rotate_x(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_rotate_x(
        qureg, control, target, angle,
    ))
}

pub fn apply_controlled_rotate_y(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_rotate_y(
        qureg, control, target, angle,
    ))
}

pub fn apply_controlled_rotate_z(
    qureg: Pin<&mut Qureg>,
    control: i32,
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_rotate_z(
        qureg, control, target, angle,
    ))
}

pub fn apply_controlled_s(qureg: Pin<&mut Qureg>, control: i32, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_s(qureg, control, target))
}

pub fn apply_controlled_sqrt_swap(
    qureg: Pin<&mut Qureg>,
    control: i32,
    qubit1: i32,
    qubit2: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_sqrt_swap(
        qureg, control, qubit1, qubit2,
    ))
}

pub fn apply_controlled_swap(
    qureg: Pin<&mut Qureg>,
    control: i32,
    qubit1: i32,
    qubit2: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_swap(qureg, control, qubit1, qubit2))
}

pub fn apply_controlled_t(qureg: Pin<&mut Qureg>, control: i32, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_controlled_t(qureg, control, target))
}

pub fn apply_diag_matr(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &DiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_diag_matr(qureg, targets, matrix))
}

pub fn apply_diag_matr1(qureg: Pin<&mut Qureg>, target: i32, matr: &DiagMatr1) -> QuestResult<()> {
    map_quest_result(ffi::apply_diag_matr1(qureg, target, matr))
}

pub fn apply_diag_matr2(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    matr: &DiagMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_diag_matr2(qureg, target1, target2, matr))
}

pub fn apply_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &DiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::apply_diag_matr_power(
        qureg,
        targets,
        matrix,
        exponent_ffi,
    ))
}

pub fn apply_forced_multi_qubit_measurement(
    qureg: Pin<&mut Qureg>,
    qubits: &[i32],
    outcomes: &[i32],
) -> QuestResult<f64> {
    map_quest_result(ffi::apply_forced_multi_qubit_measurement(
        qureg, qubits, outcomes,
    ))
}

pub fn apply_forced_qubit_measurement(
    qureg: Pin<&mut Qureg>,
    target: i32,
    outcome: i32,
) -> QuestResult<f64> {
    map_quest_result(ffi::apply_forced_qubit_measurement(qureg, target, outcome))
}

pub fn apply_full_quantum_fourier_transform(
    qureg: Pin<&mut Qureg>,
    inverse: bool,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_full_quantum_fourier_transform(qureg, inverse))
}

pub fn apply_full_state_diag_matr(
    qureg: Pin<&mut Qureg>,
    matrix: &FullStateDiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_full_state_diag_matr(qureg, matrix))
}

pub fn apply_full_state_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    matrix: &FullStateDiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::apply_full_state_diag_matr_power(
        qureg,
        matrix,
        exponent_ffi,
    ))
}

pub fn apply_multi_controlled_comp_matr(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    targets: &[i32],
    matr: &CompMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_comp_matr(
        qureg, controls, targets, matr,
    ))
}

pub fn apply_multi_controlled_comp_matr1(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
    matrix: &CompMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_comp_matr1(
        qureg, controls, target, matrix,
    ))
}

pub fn apply_multi_controlled_comp_matr2(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target1: i32,
    target2: i32,
    matr: &CompMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_comp_matr2(
        qureg, controls, target1, target2, matr,
    ))
}

pub fn apply_multi_controlled_diag_matr(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    targets: &[i32],
    matrix: &DiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_diag_matr(
        qureg, controls, targets, matrix,
    ))
}

pub fn apply_multi_controlled_diag_matr1(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
    matr: &DiagMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_diag_matr1(
        qureg, controls, target, matr,
    ))
}

pub fn apply_multi_controlled_diag_matr2(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target1: i32,
    target2: i32,
    matr: &DiagMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_diag_matr2(
        qureg, controls, target1, target2, matr,
    ))
}

pub fn apply_multi_controlled_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    targets: &[i32],
    matrix: &DiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::apply_multi_controlled_diag_matr_power(
        qureg,
        controls,
        targets,
        matrix,
        exponent_ffi,
    ))
}

pub fn apply_multi_controlled_hadamard(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_hadamard(
        qureg, controls, target,
    ))
}

pub fn apply_multi_controlled_multi_qubit_not(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    targets: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_multi_qubit_not(
        qureg, controls, targets,
    ))
}

pub fn apply_multi_controlled_pauli_gadget(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    str_arg: &PauliStr,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_pauli_gadget(
        qureg, controls, str_arg, angle,
    ))
}

pub fn apply_multi_controlled_pauli_str(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    str_arg: &PauliStr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_pauli_str(
        qureg, controls, str_arg,
    ))
}

pub fn apply_multi_controlled_pauli_x(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_pauli_x(qureg, controls, target))
}

pub fn apply_multi_controlled_pauli_y(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_pauli_y(qureg, controls, target))
}

pub fn apply_multi_controlled_pauli_z(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_pauli_z(qureg, controls, target))
}

pub fn apply_multi_controlled_phase_gadget(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    targets: &[i32],
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_phase_gadget(
        qureg, controls, targets, angle,
    ))
}

pub fn apply_multi_controlled_rotate_around_axis(
    qureg: Pin<&mut Qureg>,
    ctrls: &[i32],
    targ: i32,
    angle: f64,
    axis_x: f64,
    axis_y: f64,
    axis_z: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_rotate_around_axis(
        qureg, ctrls, targ, angle, axis_x, axis_y, axis_z,
    ))
}

pub fn apply_multi_controlled_rotate_x(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_rotate_x(
        qureg, controls, target, angle,
    ))
}

pub fn apply_multi_controlled_rotate_y(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_rotate_y(
        qureg, controls, target, angle,
    ))
}

pub fn apply_multi_controlled_rotate_z(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_rotate_z(
        qureg, controls, target, angle,
    ))
}

pub fn apply_multi_controlled_s(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_s(qureg, controls, target))
}

pub fn apply_multi_controlled_sqrt_swap(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    qubit1: i32,
    qubit2: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_sqrt_swap(
        qureg, controls, qubit1, qubit2,
    ))
}

pub fn apply_multi_controlled_swap(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    qubit1: i32,
    qubit2: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_swap(
        qureg, controls, qubit1, qubit2,
    ))
}

pub fn apply_multi_controlled_t(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_controlled_t(qureg, controls, target))
}

pub fn apply_multi_qubit_measurement(qureg: Pin<&mut Qureg>, qubits: &[i32]) -> QuestResult<i64> {
    map_quest_result(ffi::apply_multi_qubit_measurement(qureg, qubits))
}

pub fn apply_multi_qubit_not(qureg: Pin<&mut Qureg>, targets: &[i32]) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_qubit_not(qureg, targets))
}

pub fn apply_multi_qubit_phase_flip(qureg: Pin<&mut Qureg>, targets: &[i32]) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_qubit_phase_flip(qureg, targets))
}

pub fn apply_multi_qubit_phase_shift(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_qubit_phase_shift(qureg, targets, angle))
}

pub fn apply_multi_qubit_projector(
    qureg: Pin<&mut Qureg>,
    qubits: &[i32],
    outcomes: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_qubit_projector(qureg, qubits, outcomes))
}

pub fn apply_multi_state_controlled_comp_matr(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    targets: &[i32],
    matr: &CompMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_comp_matr(
        qureg, controls, states, targets, matr,
    ))
}

pub fn apply_multi_state_controlled_comp_matr1(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
    matrix: &CompMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_comp_matr1(
        qureg, controls, states, target, matrix,
    ))
}

pub fn apply_multi_state_controlled_comp_matr2(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target1: i32,
    target2: i32,
    matr: &CompMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_comp_matr2(
        qureg, controls, states, target1, target2, matr,
    ))
}

pub fn apply_multi_state_controlled_diag_matr(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    targets: &[i32],
    matrix: &DiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_diag_matr(
        qureg, controls, states, targets, matrix,
    ))
}

pub fn apply_multi_state_controlled_diag_matr1(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
    matr: &DiagMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_diag_matr1(
        qureg, controls, states, target, matr,
    ))
}

pub fn apply_multi_state_controlled_diag_matr2(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target1: i32,
    target2: i32,
    matr: &DiagMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_diag_matr2(
        qureg, controls, states, target1, target2, matr,
    ))
}

pub fn apply_multi_state_controlled_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    targets: &[i32],
    matrix: &DiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::apply_multi_state_controlled_diag_matr_power(
        qureg,
        controls,
        states,
        targets,
        matrix,
        exponent_ffi,
    ))
}

pub fn apply_multi_state_controlled_hadamard(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_hadamard(
        qureg, controls, states, target,
    ))
}

pub fn apply_multi_state_controlled_multi_qubit_not(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    targets: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_multi_qubit_not(
        qureg, controls, states, targets,
    ))
}

pub fn apply_multi_state_controlled_pauli_gadget(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    str_arg: &PauliStr,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_pauli_gadget(
        qureg, controls, states, str_arg, angle,
    ))
}

pub fn apply_multi_state_controlled_pauli_str(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    str_arg: &PauliStr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_pauli_str(
        qureg, controls, states, str_arg,
    ))
}

pub fn apply_multi_state_controlled_pauli_x(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_pauli_x(
        qureg, controls, states, target,
    ))
}

pub fn apply_multi_state_controlled_pauli_y(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_pauli_y(
        qureg, controls, states, target,
    ))
}

pub fn apply_multi_state_controlled_pauli_z(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_pauli_z(
        qureg, controls, states, target,
    ))
}

pub fn apply_multi_state_controlled_phase_gadget(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    targets: &[i32],
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_phase_gadget(
        qureg, controls, states, targets, angle,
    ))
}

pub fn apply_multi_state_controlled_rotate_around_axis(
    qureg: Pin<&mut Qureg>,
    ctrls: &[i32],
    states: &[i32],
    targ: i32,
    angle: f64,
    axis_x: f64,
    axis_y: f64,
    axis_z: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_rotate_around_axis(
        qureg, ctrls, states, targ, angle, axis_x, axis_y, axis_z,
    ))
}

pub fn apply_multi_state_controlled_rotate_x(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_rotate_x(
        qureg, controls, states, target, angle,
    ))
}

pub fn apply_multi_state_controlled_rotate_y(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_rotate_y(
        qureg, controls, states, target, angle,
    ))
}

pub fn apply_multi_state_controlled_rotate_z(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_rotate_z(
        qureg, controls, states, target, angle,
    ))
}

pub fn apply_multi_state_controlled_s(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_s(
        qureg, controls, states, target,
    ))
}

pub fn apply_multi_state_controlled_sqrt_swap(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    qubit1: i32,
    qubit2: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_sqrt_swap(
        qureg, controls, states, qubit1, qubit2,
    ))
}

pub fn apply_multi_state_controlled_swap(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    qubit1: i32,
    qubit2: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_swap(
        qureg, controls, states, qubit1, qubit2,
    ))
}

pub fn apply_multi_state_controlled_t(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    target: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_multi_state_controlled_t(
        qureg, controls, states, target,
    ))
}

pub fn apply_non_unitary_pauli_gadget(
    qureg: Pin<&mut Qureg>,
    str_arg: &PauliStr,
    angle: QuestComplex,
) -> QuestResult<()> {
    let angle_ffi = GeneratedComplex::from(angle);
    map_quest_result(ffi::apply_non_unitary_pauli_gadget(
        qureg, str_arg, angle_ffi,
    ))
}

pub fn apply_pauli_gadget(
    qureg: Pin<&mut Qureg>,
    str_arg: &PauliStr,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_pauli_gadget(qureg, str_arg, angle))
}

pub fn apply_pauli_str(qureg: Pin<&mut Qureg>, str_arg: &PauliStr) -> QuestResult<()> {
    map_quest_result(ffi::apply_pauli_str(qureg, str_arg))
}

pub fn apply_phase_flip(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_phase_flip(qureg, target))
}

pub fn apply_phase_gadget(qureg: Pin<&mut Qureg>, targets: &[i32], angle: f64) -> QuestResult<()> {
    map_quest_result(ffi::apply_phase_gadget(qureg, targets, angle))
}

pub fn apply_phase_shift(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> QuestResult<()> {
    map_quest_result(ffi::apply_phase_shift(qureg, target, angle))
}

pub fn apply_quantum_fourier_transform(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    inverse: bool,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_quantum_fourier_transform(
        qureg, targets, inverse,
    ))
}

pub fn apply_qubit_projector(qureg: Pin<&mut Qureg>, target: i32, outcome: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_qubit_projector(qureg, target, outcome))
}

pub fn apply_rotate_around_axis(
    qureg: Pin<&mut Qureg>,
    target: i32,
    angle: f64,
    axis_x: f64,
    axis_y: f64,
    axis_z: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_rotate_around_axis(
        qureg, target, angle, axis_x, axis_y, axis_z,
    ))
}

pub fn apply_rotate_x(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> QuestResult<()> {
    map_quest_result(ffi::apply_rotate_x(qureg, target, angle))
}

pub fn apply_rotate_y(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> QuestResult<()> {
    map_quest_result(ffi::apply_rotate_y(qureg, target, angle))
}

pub fn apply_rotate_z(qureg: Pin<&mut Qureg>, target: i32, angle: f64) -> QuestResult<()> {
    map_quest_result(ffi::apply_rotate_z(qureg, target, angle))
}

pub fn apply_s(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_s(qureg, target))
}

pub fn apply_sqrt_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_sqrt_swap(qureg, qubit1, qubit2))
}

pub fn apply_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_swap(qureg, qubit1, qubit2))
}

pub fn apply_t(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_t(qureg, target))
}

pub fn apply_trotterized_controlled_pauli_str_sum_gadget(
    qureg: Pin<&mut Qureg>,
    control: i32,
    sum: &PauliStrSum,
    angle: f64,
    order: i32,
    reps: i32,
    permute_terms: bool,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_trotterized_controlled_pauli_str_sum_gadget(
        qureg,
        control,
        sum,
        angle,
        order,
        reps,
        permute_terms,
    ))
}

pub fn apply_trotterized_imaginary_time_evolution(
    qureg: Pin<&mut Qureg>,
    hamil: &PauliStrSum,
    tau: f64,
    order: i32,
    reps: i32,
    permute_terms: bool,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_trotterized_imaginary_time_evolution(
        qureg,
        hamil,
        tau,
        order,
        reps,
        permute_terms,
    ))
}

pub fn apply_trotterized_multi_controlled_pauli_str_sum_gadget(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    sum: &PauliStrSum,
    angle: f64,
    order: i32,
    reps: i32,
    permute_terms: bool,
) -> QuestResult<()> {
    map_quest_result(
        ffi::apply_trotterized_multi_controlled_pauli_str_sum_gadget(
            qureg,
            controls,
            sum,
            angle,
            order,
            reps,
            permute_terms,
        ),
    )
}

pub fn apply_trotterized_multi_state_controlled_pauli_str_sum_gadget(
    qureg: Pin<&mut Qureg>,
    controls: &[i32],
    states: &[i32],
    sum: &PauliStrSum,
    angle: f64,
    order: i32,
    reps: i32,
    permute_terms: bool,
) -> QuestResult<()> {
    map_quest_result(
        ffi::apply_trotterized_multi_state_controlled_pauli_str_sum_gadget(
            qureg,
            controls,
            states,
            sum,
            angle,
            order,
            reps,
            permute_terms,
        ),
    )
}

pub fn apply_trotterized_non_unitary_pauli_str_sum_gadget(
    qureg: Pin<&mut Qureg>,
    sum: &PauliStrSum,
    angle: QuestComplex,
    order: i32,
    reps: i32,
    permute_terms: bool,
) -> QuestResult<()> {
    let angle_ffi = GeneratedComplex::from(angle);
    map_quest_result(ffi::apply_trotterized_non_unitary_pauli_str_sum_gadget(
        qureg,
        sum,
        angle_ffi,
        order,
        reps,
        permute_terms,
    ))
}

pub fn apply_trotterized_pauli_str_sum_gadget(
    qureg: Pin<&mut Qureg>,
    sum: &PauliStrSum,
    angle: f64,
    order: i32,
    reps: i32,
    permute_terms: bool,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_trotterized_pauli_str_sum_gadget(
        qureg,
        sum,
        angle,
        order,
        reps,
        permute_terms,
    ))
}

pub fn apply_two_qubit_phase_flip(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_two_qubit_phase_flip(qureg, target1, target2))
}

pub fn apply_two_qubit_phase_shift(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_two_qubit_phase_shift(
        qureg, target1, target2, angle,
    ))
}

pub fn calc_expec_full_state_diag_matr(
    qureg: &Qureg,
    matr: &FullStateDiagMatr,
) -> QuestResult<f64> {
    map_quest_result(ffi::calc_expec_full_state_diag_matr(qureg, matr))
}

pub fn calc_expec_full_state_diag_matr_power(
    qureg: &Qureg,
    matrix: &FullStateDiagMatr,
    exponent: f64,
) -> QuestResult<f64> {
    map_quest_result(ffi::calc_expec_full_state_diag_matr_power(
        qureg, matrix, exponent,
    ))
}

pub fn calc_expec_non_hermitian_full_state_diag_matr(
    qureg: &Qureg,
    matr: &FullStateDiagMatr,
) -> QuestResult<QuestComplex> {
    map_quest_result(ffi::calc_expec_non_hermitian_full_state_diag_matr(
        qureg, matr,
    ))
    .map(QuestComplex::from)
}

pub fn calc_expec_non_hermitian_full_state_diag_matr_power(
    qureg: &Qureg,
    matrix: &FullStateDiagMatr,
    exponent: QuestComplex,
) -> QuestResult<QuestComplex> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::calc_expec_non_hermitian_full_state_diag_matr_power(
        qureg,
        matrix,
        exponent_ffi,
    ))
    .map(QuestComplex::from)
}

pub fn calc_expec_non_hermitian_pauli_str_sum(
    qureg: &Qureg,
    sum: &PauliStrSum,
) -> QuestResult<QuestComplex> {
    map_quest_result(ffi::calc_expec_non_hermitian_pauli_str_sum(qureg, sum))
        .map(QuestComplex::from)
}

pub fn calc_expec_pauli_str(qureg: &Qureg, str_arg: &PauliStr) -> QuestResult<f64> {
    map_quest_result(ffi::calc_expec_pauli_str(qureg, str_arg))
}

pub fn calc_expec_pauli_str_sum(qureg: &Qureg, sum: &PauliStrSum) -> QuestResult<f64> {
    map_quest_result(ffi::calc_expec_pauli_str_sum(qureg, sum))
}

pub fn calc_fidelity(qureg: &Qureg, other: &Qureg) -> QuestResult<f64> {
    map_quest_result(ffi::calc_fidelity(qureg, other))
}

pub fn calc_inner_product(qureg: &Qureg, other: &Qureg) -> QuestResult<QuestComplex> {
    map_quest_result(ffi::calc_inner_product(qureg, other)).map(QuestComplex::from)
}

pub fn calc_partial_trace(
    qureg: &Qureg,
    trace_out_qubits: &[i32],
) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::calc_partial_trace(qureg, trace_out_qubits))
}

pub fn calc_prob_of_basis_state(qureg: &Qureg, index: i64) -> QuestResult<f64> {
    map_quest_result(ffi::calc_prob_of_basis_state(qureg, index))
}

pub fn calc_prob_of_multi_qubit_outcome(
    qureg: &Qureg,
    qubits: &[i32],
    outcomes: &[i32],
) -> QuestResult<f64> {
    map_quest_result(ffi::calc_prob_of_multi_qubit_outcome(
        qureg, qubits, outcomes,
    ))
}

pub fn calc_prob_of_qubit_outcome(qureg: &Qureg, qubit: i32, outcome: i32) -> QuestResult<f64> {
    map_quest_result(ffi::calc_prob_of_qubit_outcome(qureg, qubit, outcome))
}

pub fn calc_probs_of_all_multi_qubit_outcomes(
    qureg: &Qureg,
    qubits: &[i32],
) -> QuestResult<Vec<f64>> {
    map_quest_result(ffi::calc_probs_of_all_multi_qubit_outcomes(qureg, qubits))
}

pub fn calc_purity(qureg: &Qureg) -> QuestResult<f64> {
    map_quest_result(ffi::calc_purity(qureg))
}

pub fn calc_reduced_density_matrix(
    qureg: &Qureg,
    retain_qubits: &[i32],
) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::calc_reduced_density_matrix(qureg, retain_qubits))
}

pub fn clear_qu_est_gpu_cache() -> QuestResult<()> {
    map_quest_result(ffi::clear_qu_est_gpu_cache())
}

pub fn create_clone_qureg(qureg: &Qureg) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_clone_qureg(qureg))
}

pub fn create_custom_full_state_diag_matr(
    num_qubits: i32,
    use_distrib: i32,
    use_gpu_accel: i32,
    use_multithread: i32,
) -> QuestResult<UniquePtr<FullStateDiagMatr>> {
    map_quest_result(ffi::create_custom_full_state_diag_matr(
        num_qubits,
        use_distrib,
        use_gpu_accel,
        use_multithread,
    ))
}

pub fn create_custom_qureg(
    num_qubits: i32,
    is_dens_matr: i32,
    use_distrib: i32,
    use_gpu_accel: i32,
    use_multithread: i32,
) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_custom_qureg(
        num_qubits,
        is_dens_matr,
        use_distrib,
        use_gpu_accel,
        use_multithread,
    ))
}

pub fn create_diag_matr(num_qubits: i32) -> QuestResult<UniquePtr<DiagMatr>> {
    map_quest_result(ffi::create_diag_matr(num_qubits))
}

pub fn create_forced_density_qureg(num_qubits: i32) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_forced_density_qureg(num_qubits))
}

pub fn create_forced_qureg(num_qubits: i32) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_forced_qureg(num_qubits))
}

pub fn create_full_state_diag_matr(num_qubits: i32) -> QuestResult<UniquePtr<FullStateDiagMatr>> {
    map_quest_result(ffi::create_full_state_diag_matr(num_qubits))
}

pub fn create_full_state_diag_matr_from_pauli_str_sum(
    in_arg: &PauliStrSum,
) -> QuestResult<UniquePtr<FullStateDiagMatr>> {
    map_quest_result(ffi::create_full_state_diag_matr_from_pauli_str_sum(in_arg))
}

pub fn create_inline_diag_matr(
    num_qb: i32,
    elems: &[QuestComplex],
) -> QuestResult<UniquePtr<DiagMatr>> {
    let elems_ffi = elems
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::create_inline_diag_matr(num_qb, &elems_ffi))
}

pub fn create_pauli_str_sum_from_file(file_name: &str) -> QuestResult<UniquePtr<PauliStrSum>> {
    map_quest_result(ffi::create_pauli_str_sum_from_file(file_name))
}

pub fn create_pauli_str_sum_from_reversed_file(
    file_name: &str,
) -> QuestResult<UniquePtr<PauliStrSum>> {
    map_quest_result(ffi::create_pauli_str_sum_from_reversed_file(file_name))
}

pub fn create_qureg_from_file(file_name: &str) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_qureg_from_file(file_name))
}

pub fn get_density_qureg_amp(qureg: &Qureg, row: i64, column: i64) -> QuestResult<QuestComplex> {
    map_quest_result(ffi::get_density_qureg_amp(qureg, row, column)).map(QuestComplex::from)
}

pub fn get_diag_matr1(in_arg: &[QuestComplex]) -> QuestResult<UniquePtr<DiagMatr1>> {
    let in_arg_ffi = in_arg
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::get_diag_matr1(&in_arg_ffi))
}

pub fn get_diag_matr2(in_arg: &[QuestComplex]) -> QuestResult<UniquePtr<DiagMatr2>> {
    let in_arg_ffi = in_arg
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::get_diag_matr2(&in_arg_ffi))
}

pub fn get_pauli_str_from_string(paulis: &str) -> QuestResult<UniquePtr<PauliStr>> {
    map_quest_result(ffi::get_pauli_str_from_string(paulis))
}

pub fn get_pauli_str(paulis: &str, indices: &[i32]) -> QuestResult<UniquePtr<PauliStr>> {
    map_quest_result(ffi::get_pauli_str(paulis, indices))
}

pub fn get_qu_est_gpu_cache_size() -> QuestResult<i64> {
    map_quest_result(ffi::get_qu_est_gpu_cache_size())
}

pub fn get_qu_est_num_gpu_threads_per_block() -> QuestResult<i32> {
    map_quest_result(ffi::get_qu_est_num_gpu_threads_per_block())
}

pub fn get_qu_est_num_seeds() -> QuestResult<i32> {
    map_quest_result(ffi::get_qu_est_num_seeds())
}

pub fn get_qu_est_validation_epsilon() -> QuestResult<f64> {
    map_quest_result(ffi::get_qu_est_validation_epsilon())
}

pub fn init_blank_state(qureg: Pin<&mut Qureg>) -> QuestResult<()> {
    map_quest_result(ffi::init_blank_state(qureg))
}

pub fn init_classical_state(qureg: Pin<&mut Qureg>, state_ind: i64) -> QuestResult<()> {
    map_quest_result(ffi::init_classical_state(qureg, state_ind))
}

pub fn init_debug_state(qureg: Pin<&mut Qureg>) -> QuestResult<()> {
    map_quest_result(ffi::init_debug_state(qureg))
}

pub fn init_pure_state(qureg: Pin<&mut Qureg>, pure: &Qureg) -> QuestResult<()> {
    map_quest_result(ffi::init_pure_state(qureg, pure))
}

pub fn init_random_mixed_state(qureg: Pin<&mut Qureg>, num_pure_states: i64) -> QuestResult<()> {
    map_quest_result(ffi::init_random_mixed_state(qureg, num_pure_states))
}

pub fn init_random_pure_state(qureg: Pin<&mut Qureg>) -> QuestResult<()> {
    map_quest_result(ffi::init_random_pure_state(qureg))
}

pub fn leftapply_comp_matr1(
    qureg: Pin<&mut Qureg>,
    target: i32,
    matrix: &CompMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_comp_matr1(qureg, target, matrix))
}

pub fn leftapply_comp_matr2(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    matr: &CompMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_comp_matr2(qureg, target1, target2, matr))
}

pub fn leftapply_diag_matr(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &DiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_diag_matr(qureg, targets, matrix))
}

pub fn leftapply_diag_matr1(
    qureg: Pin<&mut Qureg>,
    target: i32,
    matr: &DiagMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_diag_matr1(qureg, target, matr))
}

pub fn leftapply_diag_matr2(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    matr: &DiagMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_diag_matr2(qureg, target1, target2, matr))
}

pub fn leftapply_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &DiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::leftapply_diag_matr_power(
        qureg,
        targets,
        matrix,
        exponent_ffi,
    ))
}

pub fn leftapply_full_state_diag_matr(
    qureg: Pin<&mut Qureg>,
    matrix: &FullStateDiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_full_state_diag_matr(qureg, matrix))
}

pub fn leftapply_full_state_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    matrix: &FullStateDiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::leftapply_full_state_diag_matr_power(
        qureg,
        matrix,
        exponent_ffi,
    ))
}

pub fn leftapply_multi_qubit_not(qureg: Pin<&mut Qureg>, targets: &[i32]) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_multi_qubit_not(qureg, targets))
}

pub fn leftapply_multi_qubit_projector(
    qureg: Pin<&mut Qureg>,
    qubits: &[i32],
    outcomes: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_multi_qubit_projector(
        qureg, qubits, outcomes,
    ))
}

pub fn leftapply_pauli_gadget(
    qureg: Pin<&mut Qureg>,
    str_arg: &PauliStr,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_pauli_gadget(qureg, str_arg, angle))
}

pub fn leftapply_pauli_str(qureg: Pin<&mut Qureg>, str_arg: &PauliStr) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_pauli_str(qureg, str_arg))
}

pub fn leftapply_pauli_str_sum(
    qureg: Pin<&mut Qureg>,
    sum: &PauliStrSum,
    workspace: &Qureg,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_pauli_str_sum(qureg, sum, workspace))
}

pub fn leftapply_pauli_x(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_pauli_x(qureg, target))
}

pub fn leftapply_pauli_y(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_pauli_y(qureg, target))
}

pub fn leftapply_pauli_z(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_pauli_z(qureg, target))
}

pub fn leftapply_phase_gadget(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_phase_gadget(qureg, targets, angle))
}

pub fn leftapply_qubit_projector(
    qureg: Pin<&mut Qureg>,
    qubit: i32,
    outcome: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_qubit_projector(qureg, qubit, outcome))
}

pub fn leftapply_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_swap(qureg, qubit1, qubit2))
}

pub fn mix_damping(qureg: Pin<&mut Qureg>, target: i32, prob: f64) -> QuestResult<()> {
    map_quest_result(ffi::mix_damping(qureg, target, prob))
}

pub fn mix_depolarising(qureg: Pin<&mut Qureg>, target: i32, prob: f64) -> QuestResult<()> {
    map_quest_result(ffi::mix_depolarising(qureg, target, prob))
}

pub fn mix_kraus_map(qureg: Pin<&mut Qureg>, targets: &[i32], map: &KrausMap) -> QuestResult<()> {
    map_quest_result(ffi::mix_kraus_map(qureg, targets, map))
}

pub fn mix_paulis(
    qureg: Pin<&mut Qureg>,
    target: i32,
    prob_x: f64,
    prob_y: f64,
    prob_z: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::mix_paulis(qureg, target, prob_x, prob_y, prob_z))
}

pub fn mix_qureg(qureg: Pin<&mut Qureg>, other: &Qureg, prob: f64) -> QuestResult<()> {
    map_quest_result(ffi::mix_qureg(qureg, other, prob))
}

pub fn mix_super_op(qureg: Pin<&mut Qureg>, targets: &[i32], superop: &SuperOp) -> QuestResult<()> {
    map_quest_result(ffi::mix_super_op(qureg, targets, superop))
}

pub fn mix_two_qubit_dephasing(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    prob: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::mix_two_qubit_dephasing(qureg, target1, target2, prob))
}

pub fn mix_two_qubit_depolarising(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    prob: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::mix_two_qubit_depolarising(
        qureg, target1, target2, prob,
    ))
}

pub fn report_comp_matr(matrix: &CompMatr) -> QuestResult<()> {
    map_quest_result(ffi::report_comp_matr(matrix))
}

pub fn report_comp_matr1(matrix: &CompMatr1) -> QuestResult<()> {
    map_quest_result(ffi::report_comp_matr1(matrix))
}

pub fn report_comp_matr2(matrix: &CompMatr2) -> QuestResult<()> {
    map_quest_result(ffi::report_comp_matr2(matrix))
}

pub fn report_diag_matr(matrix: &DiagMatr) -> QuestResult<()> {
    map_quest_result(ffi::report_diag_matr(matrix))
}

pub fn report_diag_matr1(matrix: &DiagMatr1) -> QuestResult<()> {
    map_quest_result(ffi::report_diag_matr1(matrix))
}

pub fn report_diag_matr2(matrix: &DiagMatr2) -> QuestResult<()> {
    map_quest_result(ffi::report_diag_matr2(matrix))
}

pub fn report_full_state_diag_matr(matr: &FullStateDiagMatr) -> QuestResult<()> {
    map_quest_result(ffi::report_full_state_diag_matr(matr))
}

pub fn report_kraus_map(map: &KrausMap) -> QuestResult<()> {
    map_quest_result(ffi::report_kraus_map(map))
}

pub fn report_pauli_str(str_arg: &PauliStr) -> QuestResult<()> {
    map_quest_result(ffi::report_pauli_str(str_arg))
}

pub fn report_pauli_str_sum(str_arg: &PauliStrSum) -> QuestResult<()> {
    map_quest_result(ffi::report_pauli_str_sum(str_arg))
}

pub fn report_qureg(qureg: &Qureg) -> QuestResult<()> {
    map_quest_result(ffi::report_qureg(qureg))
}

pub fn report_qureg_params(qureg: &Qureg) -> QuestResult<()> {
    map_quest_result(ffi::report_qureg_params(qureg))
}

pub fn report_scalar_real(label: &str, num: f64) -> QuestResult<()> {
    map_quest_result(ffi::report_scalar_real(label, num))
}

pub fn report_scalar(label: &str, num: QuestComplex) -> QuestResult<()> {
    let num_ffi = GeneratedComplex::from(num);
    map_quest_result(ffi::report_scalar(label, num_ffi))
}

pub fn report_str(str_arg: &str) -> QuestResult<()> {
    map_quest_result(ffi::report_str(str_arg))
}

pub fn report_super_op(op: &SuperOp) -> QuestResult<()> {
    map_quest_result(ffi::report_super_op(op))
}

pub fn rightapply_comp_matr1(
    qureg: Pin<&mut Qureg>,
    target: i32,
    matrix: &CompMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_comp_matr1(qureg, target, matrix))
}

pub fn rightapply_comp_matr2(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    matrix: &CompMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_comp_matr2(qureg, target1, target2, matrix))
}

pub fn rightapply_diag_matr(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &DiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_diag_matr(qureg, targets, matrix))
}

pub fn rightapply_diag_matr1(
    qureg: Pin<&mut Qureg>,
    target: i32,
    matrix: &DiagMatr1,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_diag_matr1(qureg, target, matrix))
}

pub fn rightapply_diag_matr2(
    qureg: Pin<&mut Qureg>,
    target1: i32,
    target2: i32,
    matrix: &DiagMatr2,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_diag_matr2(qureg, target1, target2, matrix))
}

pub fn rightapply_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &DiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::rightapply_diag_matr_power(
        qureg,
        targets,
        matrix,
        exponent_ffi,
    ))
}

pub fn rightapply_full_state_diag_matr(
    qureg: Pin<&mut Qureg>,
    matrix: &FullStateDiagMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_full_state_diag_matr(qureg, matrix))
}

pub fn rightapply_full_state_diag_matr_power(
    qureg: Pin<&mut Qureg>,
    matrix: &FullStateDiagMatr,
    exponent: QuestComplex,
) -> QuestResult<()> {
    let exponent_ffi = GeneratedComplex::from(exponent);
    map_quest_result(ffi::rightapply_full_state_diag_matr_power(
        qureg,
        matrix,
        exponent_ffi,
    ))
}

pub fn rightapply_multi_qubit_not(qureg: Pin<&mut Qureg>, targets: &[i32]) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_multi_qubit_not(qureg, targets))
}

pub fn rightapply_multi_qubit_projector(
    qureg: Pin<&mut Qureg>,
    qubits: &[i32],
    outcomes: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_multi_qubit_projector(
        qureg, qubits, outcomes,
    ))
}

pub fn rightapply_pauli_gadget(
    qureg: Pin<&mut Qureg>,
    str_arg: &PauliStr,
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_pauli_gadget(qureg, str_arg, angle))
}

pub fn rightapply_pauli_str(qureg: Pin<&mut Qureg>, str_arg: &PauliStr) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_pauli_str(qureg, str_arg))
}

pub fn rightapply_pauli_str_sum(
    qureg: Pin<&mut Qureg>,
    sum: &PauliStrSum,
    workspace: &Qureg,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_pauli_str_sum(qureg, sum, workspace))
}

pub fn rightapply_pauli_x(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_pauli_x(qureg, target))
}

pub fn rightapply_pauli_y(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_pauli_y(qureg, target))
}

pub fn rightapply_pauli_z(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_pauli_z(qureg, target))
}

pub fn rightapply_phase_gadget(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    angle: f64,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_phase_gadget(qureg, targets, angle))
}

pub fn rightapply_qubit_projector(
    qureg: Pin<&mut Qureg>,
    qubit: i32,
    outcome: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_qubit_projector(qureg, qubit, outcome))
}

pub fn rightapply_swap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_swap(qureg, qubit1, qubit2))
}

pub fn save_qureg_to_file(qureg: Pin<&mut Qureg>, arg1: &str) -> QuestResult<()> {
    map_quest_result(ffi::save_qureg_to_file(qureg, arg1))
}

pub fn set_density_qureg_flat_amps(
    qureg: Pin<&mut Qureg>,
    start_ind: i64,
    amps: &[QuestComplex],
) -> QuestResult<()> {
    let amps_ffi = amps
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::set_density_qureg_flat_amps(
        qureg, start_ind, &amps_ffi,
    ))
}

pub fn set_diag_matr(out_arg: Pin<&mut DiagMatr>, in_arg: &[QuestComplex]) -> QuestResult<()> {
    let in_arg_ffi = in_arg
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::set_diag_matr(out_arg, &in_arg_ffi))
}

pub fn set_full_state_diag_matr(
    out_arg: Pin<&mut FullStateDiagMatr>,
    start_ind: i64,
    in_arg: &[QuestComplex],
) -> QuestResult<()> {
    let in_arg_ffi = in_arg
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::set_full_state_diag_matr(
        out_arg,
        start_ind,
        &in_arg_ffi,
    ))
}

pub fn set_full_state_diag_matr_from_pauli_str_sum(
    out_arg: Pin<&mut FullStateDiagMatr>,
    in_arg: &PauliStrSum,
) -> QuestResult<()> {
    map_quest_result(ffi::set_full_state_diag_matr_from_pauli_str_sum(
        out_arg, in_arg,
    ))
}

pub fn set_inline_diag_matr(
    matr: Pin<&mut DiagMatr>,
    num_qb: i32,
    in_arg: &[QuestComplex],
) -> QuestResult<()> {
    let in_arg_ffi = in_arg
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::set_inline_diag_matr(matr, num_qb, &in_arg_ffi))
}

pub fn set_inline_full_state_diag_matr(
    matr: Pin<&mut FullStateDiagMatr>,
    start_ind: i64,
    num_elems: i64,
    in_arg: &[QuestComplex],
) -> QuestResult<()> {
    let in_arg_ffi = in_arg
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::set_inline_full_state_diag_matr(
        matr,
        start_ind,
        num_elems,
        &in_arg_ffi,
    ))
}

pub fn set_qu_est_max_num_reported_items(num_rows: i64, num_cols: i64) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_max_num_reported_items(num_rows, num_cols))
}

pub fn set_qu_est_max_num_reported_sig_figs(num_sig_figs: i32) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_max_num_reported_sig_figs(num_sig_figs))
}

pub fn set_qu_est_num_gpu_threads_per_block(num_threads_per_block: i32) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_num_gpu_threads_per_block(
        num_threads_per_block,
    ))
}

pub fn set_qu_est_num_reported_newlines(num_newlines: i32) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_num_reported_newlines(num_newlines))
}

pub fn set_qu_est_reported_pauli_chars(paulis: &str) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_reported_pauli_chars(paulis))
}

pub fn set_qu_est_reported_pauli_str_style(style: i32) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_reported_pauli_str_style(style))
}

pub fn set_qu_est_seeds_to_default() -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_seeds_to_default())
}

pub fn set_qu_est_validation_epsilon(eps: f64) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_validation_epsilon(eps))
}

pub fn set_qu_est_validation_epsilon_to_default() -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_validation_epsilon_to_default())
}

pub fn set_qu_est_validation_off() -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_validation_off())
}

pub fn set_qu_est_validation_on() -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_validation_on())
}

pub fn set_qureg_amps(
    qureg: Pin<&mut Qureg>,
    start_ind: i64,
    amps: &[QuestComplex],
) -> QuestResult<()> {
    let amps_ffi = amps
        .iter()
        .copied()
        .map(GeneratedComplex::from)
        .collect::<Vec<_>>();
    map_quest_result(ffi::set_qureg_amps(qureg, start_ind, &amps_ffi))
}

pub fn set_qureg_to_clone(out_qureg: Pin<&mut Qureg>, in_qureg: &Qureg) -> QuestResult<()> {
    map_quest_result(ffi::set_qureg_to_clone(out_qureg, in_qureg))
}

pub fn set_qureg_to_partial_trace(
    out_arg: Pin<&mut Qureg>,
    in_arg: &Qureg,
    trace_out_qubits: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::set_qureg_to_partial_trace(
        out_arg,
        in_arg,
        trace_out_qubits,
    ))
}

pub fn set_qureg_to_pauli_str_sum(qureg: Pin<&mut Qureg>, sum: &PauliStrSum) -> QuestResult<()> {
    map_quest_result(ffi::set_qureg_to_pauli_str_sum(qureg, sum))
}

pub fn set_qureg_to_reduced_density_matrix(
    out_arg: Pin<&mut Qureg>,
    in_arg: &Qureg,
    retain_qubits: &[i32],
) -> QuestResult<()> {
    map_quest_result(ffi::set_qureg_to_reduced_density_matrix(
        out_arg,
        in_arg,
        retain_qubits,
    ))
}

pub fn set_qureg_to_renormalized(qureg: Pin<&mut Qureg>) -> QuestResult<f64> {
    map_quest_result(ffi::set_qureg_to_renormalized(qureg))
}

pub fn sort_pauli_str_sum_lexicographic(sum: Pin<&mut PauliStrSum>) -> QuestResult<()> {
    map_quest_result(ffi::sort_pauli_str_sum_lexicographic(sum))
}

pub fn sort_pauli_str_sum_magnitude(sum: Pin<&mut PauliStrSum>) -> QuestResult<()> {
    map_quest_result(ffi::sort_pauli_str_sum_magnitude(sum))
}

pub fn sync_comp_matr(matr: Pin<&mut CompMatr>) -> QuestResult<()> {
    map_quest_result(ffi::sync_comp_matr(matr))
}

pub fn sync_diag_matr(matr: Pin<&mut DiagMatr>) -> QuestResult<()> {
    map_quest_result(ffi::sync_diag_matr(matr))
}

pub fn sync_full_state_diag_matr(matr: Pin<&mut FullStateDiagMatr>) -> QuestResult<()> {
    map_quest_result(ffi::sync_full_state_diag_matr(matr))
}

pub fn sync_kraus_map(map: Pin<&mut KrausMap>) -> QuestResult<()> {
    map_quest_result(ffi::sync_kraus_map(map))
}

pub fn sync_qureg_from_gpu(qureg: Pin<&mut Qureg>) -> QuestResult<()> {
    map_quest_result(ffi::sync_qureg_from_gpu(qureg))
}

pub fn sync_qureg_to_gpu(qureg: Pin<&mut Qureg>) -> QuestResult<()> {
    map_quest_result(ffi::sync_qureg_to_gpu(qureg))
}

pub fn sync_sub_qureg_from_gpu(
    qureg: Pin<&mut Qureg>,
    local_start_ind: i64,
    num_local_amps: i64,
) -> QuestResult<()> {
    map_quest_result(ffi::sync_sub_qureg_from_gpu(
        qureg,
        local_start_ind,
        num_local_amps,
    ))
}

pub fn sync_sub_qureg_to_gpu(
    qureg: Pin<&mut Qureg>,
    local_start_ind: i64,
    num_local_amps: i64,
) -> QuestResult<()> {
    map_quest_result(ffi::sync_sub_qureg_to_gpu(
        qureg,
        local_start_ind,
        num_local_amps,
    ))
}

pub fn sync_super_op(op: Pin<&mut SuperOp>) -> QuestResult<()> {
    map_quest_result(ffi::sync_super_op(op))
}
