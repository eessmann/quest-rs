use std::pin::Pin;

#[cxx::bridge]
mod ffi {
    // Shared struct for complex numbers
    #[derive(Debug, Clone)]
    struct Complex {
        re: f64,
        im: f64,
    }

    // Opaque C++ types with tag types for ExternType
    unsafe extern "C++" {
        include!("quest-sys/src/bindings.h");

        #[namespace = "quest_sys"]
        type QuregTag;

        #[namespace = "quest_sys"]
        type QuESTEnvTag;

        #[namespace = "quest_sys"]
        type PauliStrTag;

        #[namespace = "quest_sys"]
        type PauliStrSumTag;

        // Actual opaque types - no namespace means global namespace
        type Qureg;
        type QuESTEnv;
        type PauliStr;
        type PauliStrSum;

        // Type aliases for precision-dependent types - now only used in C++
        #[namespace = "quest_sys"]
        type Complex;
    }

    // FFI interface for environment management
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn initQuESTEnv();

        #[namespace = "quest_sys"]
        fn finalizeQuESTEnv();

        #[namespace = "quest_sys"]
        fn getQuESTEnv() -> UniquePtr<QuESTEnv>;

        #[namespace = "quest_sys"]
        fn isMultithreaded(env: &QuESTEnv) -> bool;

        #[namespace = "quest_sys"]
        fn isDistributed(env: &QuESTEnv) -> bool;

        #[namespace = "quest_sys"]
        fn isGpuAccelerated(env: &QuESTEnv) -> bool;
    }

    // FFI interface for quantum register management
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn createQureg(numQubits: i32) -> UniquePtr<Qureg>;

        #[namespace = "quest_sys"]
        fn createDensityQureg(numQubits: i32) -> UniquePtr<Qureg>;

        #[namespace = "quest_sys"]
        fn cloneQureg(qureg: &Qureg) -> UniquePtr<Qureg>;

        #[namespace = "quest_sys"]
        fn destroyQureg(qureg: Pin<&mut Qureg>);

        // Getters for Qureg properties
        #[namespace = "quest_sys"]
        fn getNumQubits(qureg: &Qureg) -> i32;

        #[namespace = "quest_sys"]
        fn getNumAmps(qureg: &Qureg) -> u64;

        #[namespace = "quest_sys"]
        fn isDensityMatrix(qureg: &Qureg) -> bool;
    }

    // FFI interface for state initialization
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn initZeroState(qureg: Pin<&mut Qureg>);

        #[namespace = "quest_sys"]
        fn initPlusState(qureg: Pin<&mut Qureg>);

        #[namespace = "quest_sys"]
        fn initClassicalState(qureg: Pin<&mut Qureg>, stateInd: u64);

        #[namespace = "quest_sys"]
        fn initPureState(qureg: Pin<&mut Qureg>, pure: &Qureg);

        #[namespace = "quest_sys"]
        fn initRandomPureState(qureg: Pin<&mut Qureg>);
    }

    // FFI interface for basic quantum gates
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn pauliX(qureg: Pin<&mut Qureg>, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn pauliY(qureg: Pin<&mut Qureg>, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn pauliZ(qureg: Pin<&mut Qureg>, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn hadamard(qureg: Pin<&mut Qureg>, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn rotateX(qureg: Pin<&mut Qureg>, targetQubit: i32, angle: f64);

        #[namespace = "quest_sys"]
        fn rotateY(qureg: Pin<&mut Qureg>, targetQubit: i32, angle: f64);

        #[namespace = "quest_sys"]
        fn rotateZ(qureg: Pin<&mut Qureg>, targetQubit: i32, angle: f64);

        #[namespace = "quest_sys"]
        fn sGate(qureg: Pin<&mut Qureg>, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn tGate(qureg: Pin<&mut Qureg>, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn phaseShift(qureg: Pin<&mut Qureg>, targetQubit: i32, angle: f64);
    }

    // FFI interface for multi-qubit gates
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn controlledNot(qureg: Pin<&mut Qureg>, controlQubit: i32, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn controlledPauliY(qureg: Pin<&mut Qureg>, controlQubit: i32, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn controlledPauliZ(qureg: Pin<&mut Qureg>, controlQubit: i32, targetQubit: i32);

        #[namespace = "quest_sys"]
        fn swapGate(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32);
    }

    // FFI interface for measurements
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn measure(qureg: Pin<&mut Qureg>, qubit: i32) -> i32;

        #[namespace = "quest_sys"]
        fn measureWithProb(qureg: Pin<&mut Qureg>, qubit: i32, prob: &mut f64) -> i32;

        #[namespace = "quest_sys"]
        fn calcProbOfOutcome(qureg: &Qureg, qubit: i32, outcome: i32) -> f64;
    }

    // FFI interface for calculations
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn calcFidelity(qureg1: &Qureg, qureg2: &Qureg) -> f64;

        #[namespace = "quest_sys"]
        fn calcInnerProduct(bra: &Qureg, ket: &Qureg) -> Complex;

        #[namespace = "quest_sys"]
        fn calcProbOfAllOutcomes(qureg: &Qureg, qubits: &[i32]) -> f64;
    }

    // FFI interface for reporting
    unsafe extern "C++" {
        #[namespace = "quest_sys"]
        fn reportStateToScreen(qureg: &Qureg, precision: i32);
    }
}

// Implement ExternType for opaque types
unsafe impl cxx::ExternType for ffi::Qureg {
    type Id = ffi::QuregTag;
    type Kind = cxx::kind::Opaque;
}

unsafe impl cxx::ExternType for ffi::QuESTEnv {
    type Id = ffi::QuESTEnvTag;
    type Kind = cxx::kind::Opaque;
}

unsafe impl cxx::ExternType for ffi::PauliStr {
    type Id = ffi::PauliStrTag;
    type Kind = cxx::kind::Opaque;
}

unsafe impl cxx::ExternType for ffi::PauliStrSum {
    type Id = ffi::PauliStrSumTag;
    type Kind = cxx::kind::Opaque;
}

// Re-export types and functions for convenience
pub use ffi::*;

/// Convenience function to create a Complex
pub fn complex(re: f64, im: f64) -> Complex {
    Complex { re, im }
}

/// Pauli operator enum
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pauli {
    I = 0,
    X = 1,
    Y = 2,
    Z = 3,
}

/// Basic safety wrappers around FFI functions
pub mod safe {
    use super::*;
    use std::pin::Pin;

    /// Initialize the QuEST environment
    pub fn init_quest_env() {
        unsafe { ffi::initQuESTEnv() }
    }

    /// Finalize the QuEST environment
    pub fn finalize_quest_env() {
        unsafe { ffi::finalizeQuESTEnv() }
    }

    /// Create a quantum register
    pub fn create_qureg(num_qubits: i32) -> cxx::UniquePtr<Qureg> {
        unsafe { ffi::createQureg(num_qubits) }
    }

    /// Create a density quantum register
    pub fn create_density_qureg(num_qubits: i32) -> cxx::UniquePtr<Qureg> {
        unsafe { ffi::createDensityQureg(num_qubits) }
    }

    /// Clone a quantum register
    pub fn clone_qureg(qureg: &Qureg) -> cxx::UniquePtr<Qureg> {
        unsafe { ffi::cloneQureg(qureg) }
    }

    /// Initialize a quantum register to the |0⟩ state
    pub fn init_zero_state(qureg: Pin<&mut Qureg>) {
        unsafe { ffi::initZeroState(qureg) }
    }

    /// Apply a Pauli X gate to a quantum register
    pub fn apply_pauli_x(qureg: Pin<&mut Qureg>, target_qubit: i32) {
        unsafe { ffi::pauliX(qureg, target_qubit) }
    }

    /// Apply a Hadamard gate to a quantum register
    pub fn apply_hadamard(qureg: Pin<&mut Qureg>, target_qubit: i32) {
        unsafe { ffi::hadamard(qureg, target_qubit) }
    }

    /// Apply a CNOT gate to a quantum register
    pub fn apply_cnot(qureg: Pin<&mut Qureg>, control_qubit: i32, target_qubit: i32) {
        unsafe { ffi::controlledNot(qureg, control_qubit, target_qubit) }
    }

    /// Measure a qubit in a quantum register
    pub fn measure_qubit(qureg: Pin<&mut Qureg>, qubit: i32) -> i32 {
        unsafe { ffi::measure(qureg, qubit) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_complex() {
        let c = complex(1.0, 2.0);
        assert_eq!(c.re, 1.0);
        assert_eq!(c.im, 2.0);
    }
}