#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type Qureg;
        type KrausMap;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Decoherence
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("decoherence.hpp");
        // Decoherence functions
        fn mixDephasing(qureg: Pin<&mut Qureg>, qubit: i32, prob: f64);
        fn mixTwoQubitDephasing(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32, prob: f64);
        fn mixDepolarising(qureg: Pin<&mut Qureg>, qubit: i32, prob: f64);
        fn mixTwoQubitDepolarising(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32, prob: f64);
        fn mixDamping(qureg: Pin<&mut Qureg>, qubit: i32, prob: f64);
        fn mixPaulis(qureg: Pin<&mut Qureg>, qubit: i32, probX: f64, probY: f64, probZ: f64);
        fn mixQureg(qureg: Pin<&mut Qureg>, other: Pin<&mut Qureg>, prob: f64);
        fn mixKrausMap(qureg: Pin<&mut Qureg>, qubits: &[i32], map: &KrausMap);
    }
}
