#![allow(
    clippy::missing_errors_doc,
    reason = "one-to-one safe CXX wrappers uniformly forward lifecycle and native validation errors"
)]

//! Safe `cxx` bridge bindings for `QuEST` 4.3.x, binary64, deprecated APIs disabled.
//!
//! `QuEST` only permits installing a custom input-error handler after the
//! environment has been initialized. As a result, validation failures during
//! `init_quest_env` and `init_custom_quest_env` can still follow `QuEST`'s
//! default behavior; validation failures after successful initialization are
//! converted into [`QuestError`].
//!
//! `QuEST`-owned resources exposed as opaque handles are destroyed by RAII. Drop
//! those handles before finalizing the `QuEST` environment.
//! Initialization may be attempted once per process. Every native operation
//! must run on that initializing thread. Calls before initialization, after
//! finalization, or from another thread return [`QuestError::Lifecycle`].
//! [`is_quest_env_init`] reads synchronized bridge state and is thread-independent.
//!
//! Safe bridge users cannot disable native validation:
//! ```compile_fail
//! quest_sys::set_qu_est_validation_off().unwrap();
//! ```

#![cfg_attr(
    not(all(feature = "mpi", quest_native_mpi)),
    doc = "The MPI API is hidden without both the optional feature and native MPI/subcommunicator support.
```compile_fail
use quest_sys::mpi::MpiRuntime;
```"
)]

use std::pin::Pin;

use cxx::UniquePtr;
use thiserror::Error;

#[cfg(all(feature = "mpi", quest_native_mpi))]
pub mod mpi;

mod generated_api;
pub use generated_api::*;

#[allow(
    dead_code,
    reason = "CXX requires marker declarations for opaque native handle ownership"
)]
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

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct NumericalFingerprint {
        pub rounding_mode: i32,
        pub simd_control: u64,
        pub round_to_nearest: bool,
        pub flush_to_zero: bool,
        pub denormals_are_zero: bool,
        pub underflow_control_supported: bool,
        pub validation_epsilon: f64,
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

    /// Raw values sampled from one allocated native Qureg.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct QuestRegisterDeployment {
        pub is_density_matrix: i32,
        pub is_gpu_accelerated: i32,
        pub is_distributed: i32,
        pub is_multithreaded: i32,
        pub num_qubits: i32,
        pub rank: i32,
        pub num_nodes: i32,
        pub num_amps_per_node: i64,
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
        fn init_custom_quest_env_modes(
            use_distrib: i32,
            use_gpu_accel: i32,
            use_multithread: i32,
        ) -> Result<()>;
        fn finalize_quest_env() -> Result<()>;
        fn finalize_quest_env_on_drop();
        fn sync_quest_env() -> Result<()>;
        fn is_quest_env_init() -> bool;
        fn get_quest_env() -> Result<QuestEnvironment>;
        fn get_environment_string() -> Result<String>;
        fn get_numerical_fingerprint() -> Result<NumericalFingerprint>;
        fn set_qu_est_seeds(seeds: &[u32]) -> Result<()>;
        fn get_qu_est_seeds() -> Result<Vec<u32>>;

        fn create_qureg(num_qubits: i32) -> Result<UniquePtr<Qureg>>;
        fn create_density_qureg(num_qubits: i32) -> Result<UniquePtr<Qureg>>;
        fn get_qureg_deployment(qureg: &Qureg) -> Result<QuestRegisterDeployment>;
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
        fn set_density_qureg_amps(
            qureg: Pin<&mut Qureg>,
            start_row: i64,
            start_col: i64,
            values: &[QuestComplex],
            num_rows: i64,
            num_cols: i64,
        ) -> Result<()>;
        fn add_qureg(out: Pin<&mut Qureg>, source: &Qureg) -> Result<()>;
        fn get_density_qureg_amps(
            qureg: &Qureg,
            start_row: i64,
            start_col: i64,
            num_rows: i64,
            num_cols: i64,
        ) -> Result<Vec<QuestComplex>>;
        fn apply_global_phase(qureg: Pin<&mut Qureg>, angle: f64) -> Result<()>;

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
        fn set_kraus_map_flat(
            map: Pin<&mut KrausMap>,
            values: &[QuestComplex],
            num_operators: i32,
            num_rows: i64,
        ) -> Result<()>;

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
    NumericalFingerprint, PauliStr, PauliStrSum, QubitMeasurement, QuestComplex, QuestEnvironment,
    QuestRegisterDeployment, Qureg, SuperOp,
};

pub type QuestResult<T> = Result<T, QuestError>;

#[derive(Debug, Error)]
pub enum QuestError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Lifecycle(String),
}

impl From<cxx::Exception> for QuestError {
    fn from(error: cxx::Exception) -> Self {
        const LIFECYCLE_PREFIX: &str = "quest-sys lifecycle: ";
        let message = error.what();
        message.strip_prefix(LIFECYCLE_PREFIX).map_or_else(
            || Self::Validation(message.to_owned()),
            |message| Self::Lifecycle(message.to_owned()),
        )
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

/// Finalize the runtime after all native handles have been destroyed.
///
/// Rejection for live handles or the wrong thread leaves an active runtime
/// available to its owner. Once native finalization begins, any failure is
/// terminal. Successful finalization is idempotent and never permits restarting
/// the runtime in this process.
pub fn finalize_quest_env() -> QuestResult<()> {
    map_quest_result(ffi::finalize_quest_env())
}

/// Terminal cleanup for an environment owner's destructor or a failed
/// publication after successful native initialization.
///
/// This function does not panic. It finalizes only on the owner thread with no
/// live handles; any unsuccessful cleanup permanently rejects further native
/// operations and initialization, preserving storage unsafe to destroy. Calling
/// it before initialization or after successful finalization is a no-op.
///
/// Do not call this after a rejected initialization attempt: another caller may
/// own the active environment.
#[doc(hidden)]
pub fn finalize_quest_env_on_drop() {
    ffi::finalize_quest_env_on_drop();
}

/// Select native deployment modes: -1 for automatic, 0 disabled, 1 enabled.
/// Invalid flags are rejected before consuming the native initialization attempt.
pub fn init_custom_quest_env_modes(
    use_distrib: i32,
    use_gpu_accel: i32,
    use_multithread: i32,
) -> QuestResult<()> {
    map_quest_result(ffi::init_custom_quest_env_modes(
        use_distrib,
        use_gpu_accel,
        use_multithread,
    ))
}

pub fn sync_quest_env() -> QuestResult<()> {
    map_quest_result(ffi::sync_quest_env())
}

#[must_use]
pub fn is_quest_env_init() -> bool {
    ffi::is_quest_env_init()
}

pub fn get_quest_env() -> QuestResult<QuestEnvironment> {
    map_quest_result(ffi::get_quest_env())
}

pub fn get_environment_string() -> QuestResult<String> {
    map_quest_result(ffi::get_environment_string())
}

/// Snapshot safety-relevant numerical configuration on the owner thread.
///
/// `SIMD` exception status flags are excluded, so arithmetic alone does not
/// invalidate a snapshot. Unsupported architectures report unavailable
/// underflow controls instead of assuming a default policy.
pub fn get_numerical_fingerprint() -> QuestResult<NumericalFingerprint> {
    map_quest_result(ffi::get_numerical_fingerprint())
}

/// Replace `QuEST`'s process-wide RNG seed sequence.
pub fn set_qu_est_seeds(seeds: &[u32]) -> QuestResult<()> {
    map_quest_result(ffi::set_qu_est_seeds(seeds))
}

pub fn get_qu_est_seeds() -> QuestResult<Vec<u32>> {
    map_quest_result(ffi::get_qu_est_seeds())
}

pub fn create_qureg(num_qubits: i32) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_qureg(num_qubits))
}

pub fn create_density_qureg(num_qubits: i32) -> QuestResult<UniquePtr<Qureg>> {
    map_quest_result(ffi::create_density_qureg(num_qubits))
}

/// Read actual deployment fields from a live, allocated native register.
pub fn get_qureg_deployment(qureg: &Qureg) -> QuestResult<QuestRegisterDeployment> {
    map_quest_result(ffi::get_qureg_deployment(qureg))
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

/// Copy a rectangular row-major buffer into a density register.
/// This differs from `QuEST`'s column-major *flat density* storage convention.
pub fn set_density_qureg_amps(
    qureg: Pin<&mut Qureg>,
    start_row: i64,
    start_col: i64,
    values: &[QuestComplex],
    num_rows: i64,
    num_cols: i64,
) -> QuestResult<()> {
    map_quest_result(ffi::set_density_qureg_amps(
        qureg, start_row, start_col, values, num_rows, num_cols,
    ))
}

/// Copy a rectangular density block into owned row-major values.
pub fn get_density_qureg_amps(
    qureg: &Qureg,
    start_row: i64,
    start_col: i64,
    num_rows: i64,
    num_cols: i64,
) -> QuestResult<Vec<QuestComplex>> {
    map_quest_result(ffi::get_density_qureg_amps(
        qureg, start_row, start_col, num_rows, num_cols,
    ))
}

/// Multiply a statevector by `exp(i angle)`; a density matrix is unchanged.
pub fn apply_global_phase(qureg: Pin<&mut Qureg>, angle: f64) -> QuestResult<()> {
    map_quest_result(ffi::apply_global_phase(qureg, angle))
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

/// Copy a square row-major complex buffer into an existing native matrix.
pub fn set_comp_matr_flat(
    matrix: Pin<&mut CompMatr>,
    values: &[QuestComplex],
    num_rows: i64,
) -> QuestResult<()> {
    map_quest_result(ffi::set_comp_matr_flat(matrix, values, num_rows))
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

    let num_rows = i64::try_from(rows.len())
        .map_err(|_| QuestError::InvalidInput("matrix row count exceeds i64".to_owned()))?;
    let count = width.checked_mul(rows.len()).ok_or_else(|| {
        QuestError::InvalidInput("matrix element count overflows usize".to_owned())
    })?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|error| {
        QuestError::InvalidInput(format!("cannot allocate matrix staging buffer: {error}"))
    })?;
    values.extend(rows.iter().flat_map(|row| row.iter().copied()));
    set_comp_matr_flat(matrix, &values, num_rows)
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

/// Copy operators in operator-major, then row-major order into a Kraus map.
pub fn set_kraus_map_flat(
    map: Pin<&mut KrausMap>,
    values: &[QuestComplex],
    num_operators: i32,
    num_rows: i64,
) -> QuestResult<()> {
    map_quest_result(ffi::set_kraus_map_flat(
        map,
        values,
        num_operators,
        num_rows,
    ))
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

/// Add a second register into the output without allocating native state storage.
/// Both registers must have identical dimensions, kind and deployment.
pub fn add_qureg(out: Pin<&mut Qureg>, source: &Qureg) -> QuestResult<()> {
    map_quest_result(ffi::add_qureg(out, source))
}
