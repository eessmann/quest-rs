use crate::types::ffi::*;

#[cxx::bridge]
pub mod ffi {

    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type Qureg = crate::types::ffi::Qureg;
        type FullStateDiagMatr = crate::types::ffi::FullStateDiagMatr;
        type PauliStr = crate::types::ffi::PauliStr;
        type PauliStrSum = crate::types::ffi::PauliStrSum;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Calculations
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("calculations.hpp");

        // Expectation value calculations
        fn calcExpecPauliStr(qureg: &Qureg, str: &PauliStr) -> f64;
        fn calcExpecPauliStrSum(qureg: &Qureg, sum: &PauliStrSum) -> f64;
        fn calcExpecFullStateDiagMatr(qureg: &Qureg, matr: &FullStateDiagMatr) -> f64;
        fn calcExpecFullStateDiagMatrPower(
            qureg: &Qureg,
            matr: &FullStateDiagMatr,
            exponent: Quest_Complex,
        ) -> f64;

        // Probability calculations
        fn calcTotalProb(qureg: &Qureg) -> f64;
        fn calcProbOfBasisState(qureg: &Qureg, index: i64) -> f64;
        fn calcProbOfQubitOutcome(qureg: &Qureg, qubit: i32, outcome: i32) -> f64;
        fn calcProbOfMultiQubitOutcome(qureg: &Qureg, qubits: &[i32], outcomes: &[i32]) -> f64;
        fn calcProbsOfAllMultiQubitOutcomes(
            outcomeProbs: &mut [f64],
            qureg: &Qureg,
            qubits: &[i32],
        );

        // Purity and fidelity
        fn calcPurity(qureg: &Qureg) -> f64;
        fn calcFidelity(qureg: &Qureg, other: &Qureg) -> f64;
        fn calcDistance(qureg1: &Qureg, qureg2: &Qureg) -> f64;

        // Partial trace operations
        fn calcPartialTrace(qureg: &Qureg, traceOutQubits: &[i32]) -> UniquePtr<Qureg>;
        fn calcReducedDensityMatrix(qureg: &Qureg, retainQubits: &[i32]) -> UniquePtr<Qureg>;
        fn setQuregToPartialTrace(out: Pin<&mut Qureg>, in_: &Qureg, traceOutQubits: &[i32]);
        fn setQuregToReducedDensityMatrix(out: Pin<&mut Qureg>, in_: &Qureg, retainQubits: &[i32]);

        // Non-Hermitian expectation values
        fn calcInnerProduct(qureg1: &Qureg, qureg2: &Qureg) -> Quest_Complex;
        fn calcExpecNonHermitianPauliStrSum(qureg: &Qureg, sum: &PauliStrSum) -> Quest_Complex;
        fn calcExpecNonHermitianFullStateDiagMatr(
            qureg: &Qureg,
            matr: &FullStateDiagMatr,
        ) -> Quest_Complex;
        fn calcExpecNonHermitianFullStateDiagMatrPower(
            qureg: &Qureg,
            matrix: &FullStateDiagMatr,
            exponent: Quest_Complex,
        ) -> Quest_Complex;
    }
}