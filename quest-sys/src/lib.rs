mod calculations;
mod channel;
mod debug;
mod decoherence;
mod environment;
mod initialisation;
mod matrices;
mod operations;
mod qureg;
mod types;

/// Convenience function to create a quest_complex
pub fn complex(re: f64, im: f64) -> types::ffi::Quest_Complex {
    types::ffi::Quest_Complex { re, im }
}

// Create a safe module with re-exports of commonly used functions
pub mod safe {
    //pub use crate::environment::{init_quest_env, finalize_quest_env};
    //pub use crate::qureg::{create_qureg, destroy_qureg};
    //pub use crate::initialisation::{init_zero_state, init_plus_state};
    //pub use crate::operations::{
    //    apply_hadamard, apply_pauli_x, apply_pauli_y, apply_pauli_z,
    //    apply_controlled_pauli_x, apply_qubit_measurement
    //};
}
