//! Safe cxx bridge bindings for QuEST 4.2.0.
//!
//! QuEST only permits installing a custom input-error handler after the
//! environment has been initialized. As a result, validation failures during
//! `init_quest_env` and `init_custom_quest_env` can still follow QuEST's
//! default behavior; validation failures after successful initialization are
//! converted into [`QuestError`].
//!
//! QuEST-owned resources exposed as opaque handles are destroyed by RAII. Drop
//! those handles before finalizing the QuEST environment.

use std::pin::Pin;

use cxx::UniquePtr;
use thiserror::Error;

mod generated_api;
pub use generated_api::*;

#[allow(dead_code)]
#[cxx::bridge(namespace = "quest_sys")]
mod ffi {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct QuestComplex {
        pub re: f64,
        pub im: f64,
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct QubitMeasurement {
        pub outcome: i32,
        pub probability: f64,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct QuestEnvironment {
        pub is_multithreaded: bool,
        pub is_gpu_accelerated: bool,
        pub is_distributed: bool,
        pub is_mpi_user_owned: bool,
        pub is_cu_quantum_enabled: bool,
        pub is_gpu_sharing_enabled: i32,
        pub is_mpi_gpu_aware: i32,
        pub rank: i32,
        pub num_nodes: i32,
    }

    unsafe extern "C++" {
        include!("quest_bindings.hpp");

        type Qureg;
        type CompMatr1;
        type CompMatr2;
        type CompMatr;
        type DiagMatr1;
        type DiagMatr2;
        type DiagMatr;
        type FullStateDiagMatr;
        type SuperOp;
        type KrausMap;
        type PauliStr;
        type PauliStrSum;

        fn init_quest_env() -> Result<()>;
        fn init_custom_quest_env(
            use_distrib: bool,
            use_gpu_accel: bool,
            use_multithread: bool,
        ) -> Result<()>;
        fn finalize_quest_env() -> Result<()>;
        fn sync_quest_env() -> Result<()>;
        fn is_quest_env_init() -> bool;
        fn get_quest_env() -> Result<QuestEnvironment>;
        fn get_environment_string() -> Result<String>;

        fn create_qureg(num_qubits: i32) -> Result<UniquePtr<Qureg>>;
        fn create_density_qureg(num_qubits: i32) -> Result<UniquePtr<Qureg>>;
        fn unique_ptr_marker_comp_matr1() -> UniquePtr<CompMatr1>;
        fn unique_ptr_marker_comp_matr2() -> UniquePtr<CompMatr2>;
        fn unique_ptr_marker_diag_matr1() -> UniquePtr<DiagMatr1>;
        fn unique_ptr_marker_diag_matr2() -> UniquePtr<DiagMatr2>;
        fn unique_ptr_marker_diag_matr() -> UniquePtr<DiagMatr>;
        fn unique_ptr_marker_full_state_diag_matr() -> UniquePtr<FullStateDiagMatr>;
        fn unique_ptr_marker_pauli_str() -> UniquePtr<PauliStr>;

        fn init_zero_state(qureg: Pin<&mut Qureg>) -> Result<()>;
        fn init_plus_state(qureg: Pin<&mut Qureg>) -> Result<()>;
        fn init_arbitrary_pure_state(qureg: Pin<&mut Qureg>, amps: &[QuestComplex]) -> Result<()>;

        fn get_qureg_amp(qureg: &Qureg, index: i64) -> Result<QuestComplex>;
        fn get_qureg_amps(
            qureg: &Qureg,
            start_index: i64,
            num_amps: i64,
        ) -> Result<Vec<QuestComplex>>;
        fn calc_total_prob(qureg: &Qureg) -> Result<f64>;

        fn apply_qubit_measurement(qureg: Pin<&mut Qureg>, target: i32) -> Result<i32>;
        fn apply_qubit_measurement_and_get_prob(
            qureg: Pin<&mut Qureg>,
            target: i32,
        ) -> Result<QubitMeasurement>;

        fn create_comp_matr(num_qubits: i32) -> Result<UniquePtr<CompMatr>>;
        fn set_comp_matr_flat(
            matrix: Pin<&mut CompMatr>,
            values: &[QuestComplex],
            num_rows: i64,
        ) -> Result<()>;
        fn apply_comp_matr(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &CompMatr,
        ) -> Result<()>;
        fn leftapply_comp_matr(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &CompMatr,
        ) -> Result<()>;
        fn rightapply_comp_matr(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &CompMatr,
        ) -> Result<()>;

        fn apply_hadamard(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn apply_pauli_x(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn apply_pauli_y(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;
        fn apply_pauli_z(qureg: Pin<&mut Qureg>, target: i32) -> Result<()>;

        fn mix_dephasing(qureg: Pin<&mut Qureg>, target: i32, probability: f64) -> Result<()>;

        fn create_super_op(num_qubits: i32) -> Result<UniquePtr<SuperOp>>;
        fn create_kraus_map(num_qubits: i32, num_operators: i32) -> Result<UniquePtr<KrausMap>>;

        fn create_inline_pauli_str_sum(spec: &str) -> Result<UniquePtr<PauliStrSum>>;
        fn apply_trotterized_unitary_time_evolution(
            qureg: Pin<&mut Qureg>,
            hamiltonian: &PauliStrSum,
            time: f64,
            order: i32,
            reps: i32,
            permute_terms: bool,
        ) -> Result<()>;
    }
}

pub use ffi::{
    CompMatr, CompMatr1, CompMatr2, DiagMatr, DiagMatr1, DiagMatr2, FullStateDiagMatr, KrausMap,
    PauliStr, PauliStrSum, QubitMeasurement, QuestComplex, QuestEnvironment, Qureg, SuperOp,
};

pub type QuestResult<T> = Result<T, QuestError>;

#[derive(Debug, Error)]
pub enum QuestError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    InvalidInput(String),
}

impl From<cxx::Exception> for QuestError {
    fn from(error: cxx::Exception) -> Self {
        Self::Validation(error.what().to_owned())
    }
}

fn map_quest_result<T>(result: Result<T, cxx::Exception>) -> QuestResult<T> {
    result.map_err(QuestError::from)
}

pub fn init_quest_env() -> QuestResult<()> {
    map_quest_result(ffi::init_quest_env())
}

pub fn init_custom_quest_env(
    use_distrib: bool,
    use_gpu_accel: bool,
    use_multithread: bool,
) -> QuestResult<()> {
    map_quest_result(ffi::init_custom_quest_env(
        use_distrib,
        use_gpu_accel,
        use_multithread,
    ))
}

pub fn finalize_quest_env() -> QuestResult<()> {
    map_quest_result(ffi::finalize_quest_env())
}

pub fn sync_quest_env() -> QuestResult<()> {
    map_quest_result(ffi::sync_quest_env())
}

pub fn is_quest_env_init() -> bool {
    ffi::is_quest_env_init()
}

pub fn get_quest_env() -> QuestResult<QuestEnvironment> {
    map_quest_result(ffi::get_quest_env())
}

pub fn get_environment_string() -> QuestResult<String> {
    map_quest_result(ffi::get_environment_string())
}

pub fn create_qureg(num_qubits: i32) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_qureg(num_qubits))
}

pub fn create_density_qureg(num_qubits: i32) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_density_qureg(num_qubits))
}

pub fn init_zero_state(qureg: Pin<&mut Qureg>) -> QuestResult<()> {
    map_quest_result(ffi::init_zero_state(qureg))
}

pub fn init_plus_state(qureg: Pin<&mut Qureg>) -> QuestResult<()> {
    map_quest_result(ffi::init_plus_state(qureg))
}

pub fn init_arbitrary_pure_state(qureg: Pin<&mut Qureg>, amps: &[QuestComplex]) -> QuestResult<()> {
    map_quest_result(ffi::init_arbitrary_pure_state(qureg, amps))
}

pub fn get_qureg_amp(qureg: &Qureg, index: i64) -> QuestResult<QuestComplex> {
    map_quest_result(ffi::get_qureg_amp(qureg, index))
}

pub fn get_qureg_amps(
    qureg: &Qureg,
    start_index: i64,
    num_amps: i64,
) -> QuestResult<Vec<QuestComplex>> {
    map_quest_result(ffi::get_qureg_amps(qureg, start_index, num_amps))
}

pub fn calc_total_prob(qureg: &Qureg) -> QuestResult<f64> {
    map_quest_result(ffi::calc_total_prob(qureg))
}

pub fn apply_qubit_measurement(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<i32> {
    map_quest_result(ffi::apply_qubit_measurement(qureg, target))
}

pub fn apply_qubit_measurement_and_get_prob(
    qureg: Pin<&mut Qureg>,
    target: i32,
) -> QuestResult<QubitMeasurement> {
    map_quest_result(ffi::apply_qubit_measurement_and_get_prob(qureg, target))
}

pub fn create_comp_matr(num_qubits: i32) -> QuestResult<UniquePtr<CompMatr>> {
    map_quest_result(ffi::create_comp_matr(num_qubits))
}

pub fn set_comp_matr(matrix: Pin<&mut CompMatr>, rows: &[&[QuestComplex]]) -> QuestResult<()> {
    let Some(first_row) = rows.first() else {
        return Err(QuestError::InvalidInput(
            "set_comp_matr requires at least one row".to_owned(),
        ));
    };

    let width = first_row.len();
    if width != rows.len() {
        return Err(QuestError::InvalidInput(
            "set_comp_matr requires a square matrix".to_owned(),
        ));
    }
    if rows.iter().any(|row| row.len() != width) {
        return Err(QuestError::InvalidInput(
            "set_comp_matr requires rows with identical lengths".to_owned(),
        ));
    }

    let values = rows
        .iter()
        .flat_map(|row| row.iter().copied())
        .collect::<Vec<_>>();

    map_quest_result(ffi::set_comp_matr_flat(matrix, &values, rows.len() as i64))
}

pub fn apply_comp_matr(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &CompMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_comp_matr(qureg, targets, matrix))
}

pub fn leftapply_comp_matr(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &CompMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::leftapply_comp_matr(qureg, targets, matrix))
}

pub fn rightapply_comp_matr(
    qureg: Pin<&mut Qureg>,
    targets: &[i32],
    matrix: &CompMatr,
) -> QuestResult<()> {
    map_quest_result(ffi::rightapply_comp_matr(qureg, targets, matrix))
}

pub fn apply_hadamard(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_hadamard(qureg, target))
}

pub fn apply_pauli_x(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_pauli_x(qureg, target))
}

pub fn apply_pauli_y(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_pauli_y(qureg, target))
}

pub fn apply_pauli_z(qureg: Pin<&mut Qureg>, target: i32) -> QuestResult<()> {
    map_quest_result(ffi::apply_pauli_z(qureg, target))
}

pub fn mix_dephasing(qureg: Pin<&mut Qureg>, target: i32, probability: f64) -> QuestResult<()> {
    map_quest_result(ffi::mix_dephasing(qureg, target, probability))
}

pub fn create_super_op(num_qubits: i32) -> QuestResult<UniquePtr<SuperOp>> {
    map_quest_result(ffi::create_super_op(num_qubits))
}

pub fn create_kraus_map(num_qubits: i32, num_operators: i32) -> QuestResult<UniquePtr<KrausMap>> {
    map_quest_result(ffi::create_kraus_map(num_qubits, num_operators))
}

pub fn create_inline_pauli_str_sum(spec: &str) -> QuestResult<UniquePtr<PauliStrSum>> {
    map_quest_result(ffi::create_inline_pauli_str_sum(spec))
}

pub fn apply_trotterized_unitary_time_evolution(
    qureg: Pin<&mut Qureg>,
    hamiltonian: &PauliStrSum,
    time: f64,
    order: i32,
    reps: i32,
    permute_terms: bool,
) -> QuestResult<()> {
    map_quest_result(ffi::apply_trotterized_unitary_time_evolution(
        qureg,
        hamiltonian,
        time,
        order,
        reps,
        permute_terms,
    ))
}
