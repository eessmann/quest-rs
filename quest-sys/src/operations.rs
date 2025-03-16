#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type Qureg;
        type CompMatr;
        type CompMatr1;
        type CompMatr2;
        type DiagMatr;
        type DiagMatr1;
        type DiagMatr2;
        type FullStateDiagMatr;
        type PauliStr;
        type PauliStrSum;
        type KrausMap;
        type SuperOp;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Operations
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("operations.hpp");
        // CompMatr1 operations
        fn multiplyCompMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &CompMatr1);
        fn applyCompMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &CompMatr1);
        fn applyControlledCompMatr1(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
            matr: &CompMatr1,
        );
        fn applyMultiControlledCompMatr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            matr: &CompMatr1,
        );
        fn applyMultiStateControlledCompMatr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            matr: &CompMatr1,
        );

        // CompMatr2 operations
        fn multiplyCompMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &CompMatr2);
        fn applyCompMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &CompMatr2);
        fn applyControlledCompMatr2(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target1: i32,
            target2: i32,
            matr: &CompMatr2,
        );
        fn applyMultiControlledCompMatr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            numControls: i32,
            target1: i32,
            target2: i32,
            matr: &CompMatr2,
        );
        fn applyMultiStateControlledCompMatr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target1: i32,
            target2: i32,
            matr: &CompMatr2,
        );

        // CompMatr operations
        fn multiplyCompMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matr: &CompMatr);
        fn applyCompMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matr: &CompMatr);
        fn applyControlledCompMatr(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            matr: &CompMatr,
        );
        fn applyMultiControlledCompMatr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            matr: &CompMatr,
        );
        fn applyMultiStateControlledCompMatr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            matr: &CompMatr,
        );

        // DiagMatr1 operations
        fn multiplyDiagMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &DiagMatr1);
        fn applyDiagMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &DiagMatr1);
        fn applyControlledDiagMatr1(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target: i32,
            matr: &DiagMatr1,
        );
        fn applyMultiControlledDiagMatr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            matr: &DiagMatr1,
        );
        fn applyMultiStateControlledDiagMatr1(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            matr: &DiagMatr1,
        );

        // DiagMatr2 operations
        fn multiplyDiagMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &DiagMatr2);
        fn applyDiagMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &DiagMatr2);
        fn applyControlledDiagMatr2(
            qureg: Pin<&mut Qureg>,
            control: i32,
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        );
        fn applyMultiControlledDiagMatr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        );
        fn applyMultiStateControlledDiagMatr2(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target1: i32,
            target2: i32,
            matr: &DiagMatr2,
        );

        // DiagMatr operations
        fn multiplyDiagMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matrix: &DiagMatr);
        fn applyDiagMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matrix: &DiagMatr);
        fn applyControlledDiagMatr(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            matrix: &DiagMatr,
        );
        fn applyMultiControlledDiagMatr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
        );
        fn applyMultiStateControlledDiagMatr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
        );

        // DiagMatrPower operations
        fn multiplyDiagMatrPower(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: Quest_Complex,
        );
        fn applyDiagMatrPower(
            qureg: Pin<&mut Qureg>,
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: Quest_Complex,
        );
        fn applyControlledDiagMatrPower(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: Quest_Complex,
        );
        fn applyMultiControlledDiagMatrPower(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: Quest_Complex,
        );
        fn applyMultiStateControlledDiagMatrPower(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            matrix: &DiagMatr,
            exponent: Quest_Complex,
        );

        // FullStateDiagMatr operations
        fn multiplyFullStateDiagMatr(qureg: Pin<&mut Qureg>, matrix: &FullStateDiagMatr);
        fn multiplyFullStateDiagMatrPower(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
            exponent: Quest_Complex,
        );
        fn applyFullStateDiagMatr(qureg: Pin<&mut Qureg>, matrix: &FullStateDiagMatr);
        fn applyFullStateDiagMatrPower(
            qureg: Pin<&mut Qureg>,
            matrix: &FullStateDiagMatr,
            exponent: Quest_Complex,
        );

        // S gate operations
        fn applyS(qureg: Pin<&mut Qureg>, target: i32);
        fn applyControlledS(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyMultiControlledS(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);
        fn applyMultiStateControlledS(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        );

        // T gate operations
        fn applyT(qureg: Pin<&mut Qureg>, target: i32);
        fn applyControlledT(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyMultiControlledT(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);
        fn applyMultiStateControlledT(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        );

        // Hadamard operations
        fn applyHadamard(qureg: Pin<&mut Qureg>, target: i32);
        fn applyControlledHadamard(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyMultiControlledHadamard(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);
        fn applyMultiStateControlledHadamard(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        );

        // Swap operations
        fn multiplySwap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32);
        fn applySwap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32);
        fn applyControlledSwap(qureg: Pin<&mut Qureg>, control: i32, qubit1: i32, qubit2: i32);
        fn applyMultiControlledSwap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            qubit1: i32,
            qubit2: i32,
        );
        fn applyMultiStateControlledSwap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            qubit1: i32,
            qubit2: i32,
        );

        // Sqrt-swap operations
        fn applySqrtSwap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32);
        fn applyControlledSqrtSwap(qureg: Pin<&mut Qureg>, control: i32, qubit1: i32, qubit2: i32);
        fn applyMultiControlledSqrtSwap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            qubit1: i32,
            qubit2: i32,
        );
        fn applyMultiStateControlledSqrtSwap(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            qubit1: i32,
            qubit2: i32,
        );

        // Individual Pauli operations
        fn multiplyPauliX(qureg: Pin<&mut Qureg>, target: i32);
        fn multiplyPauliY(qureg: Pin<&mut Qureg>, target: i32);
        fn multiplyPauliZ(qureg: Pin<&mut Qureg>, target: i32);

        fn applyPauliX(qureg: Pin<&mut Qureg>, target: i32);
        fn applyPauliY(qureg: Pin<&mut Qureg>, target: i32);
        fn applyPauliZ(qureg: Pin<&mut Qureg>, target: i32);

        fn applyControlledPauliX(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyControlledPauliY(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyControlledPauliZ(qureg: Pin<&mut Qureg>, control: i32, target: i32);

        fn applyMultiControlledPauliX(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);
        fn applyMultiControlledPauliY(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);
        fn applyMultiControlledPauliZ(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);

        fn applyMultiStateControlledPauliX(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        );
        fn applyMultiStateControlledPauliY(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        );
        fn applyMultiStateControlledPauliZ(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
        );

        // Pauli string operations
        fn multiplyPauliStr(qureg: Pin<&mut Qureg>, str: &PauliStr);
        fn applyPauliStr(qureg: Pin<&mut Qureg>, str: &PauliStr);
        fn applyControlledPauliStr(qureg: Pin<&mut Qureg>, control: i32, str: &PauliStr);
        fn applyMultiControlledPauliStr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            numControls: i32,
            str: &PauliStr,
        );
        fn applyMultiStateControlledPauliStr(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            str: &PauliStr,
        );

        // Pauli string sum operations
        fn multiplyPauliStrSum(
            qureg: Pin<&mut Qureg>,
            sum: &PauliStrSum,
            workspace: Pin<&mut Qureg>,
        );
        fn applyTrotterizedPauliStrSumGadget(
            qureg: Pin<&mut Qureg>,
            sum: &PauliStrSum,
            angle: f64,
            order: i32,
            reps: i32,
        );

        // Rotation operations
        fn applyRotateX(qureg: Pin<&mut Qureg>, target: i32, angle: f64);
        fn applyRotateY(qureg: Pin<&mut Qureg>, target: i32, angle: f64);
        fn applyRotateZ(qureg: Pin<&mut Qureg>, target: i32, angle: f64);

        fn applyControlledRotateX(qureg: Pin<&mut Qureg>, control: i32, target: i32, angle: f64);
        fn applyControlledRotateY(qureg: Pin<&mut Qureg>, control: i32, target: i32, angle: f64);
        fn applyControlledRotateZ(qureg: Pin<&mut Qureg>, control: i32, target: i32, angle: f64);

        fn applyMultiControlledRotateX(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            angle: f64,
        );
        fn applyMultiControlledRotateY(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            angle: f64,
        );
        fn applyMultiControlledRotateZ(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            target: i32,
            angle: f64,
        );

        fn applyMultiStateControlledRotateX(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            angle: f64,
        );
        fn applyMultiStateControlledRotateY(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            angle: f64,
        );
        fn applyMultiStateControlledRotateZ(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            target: i32,
            angle: f64,
        );

        // Arbitrary axis rotation
        fn applyRotateAroundAxis(
            qureg: Pin<&mut Qureg>,
            targ: i32,
            angle: f64,
            axisX: f64,
            axisY: f64,
            axisZ: f64,
        );
        fn applyControlledRotateAroundAxis(
            qureg: Pin<&mut Qureg>,
            ctrl: i32,
            targ: i32,
            angle: f64,
            axisX: f64,
            axisY: f64,
            axisZ: f64,
        );
        fn applyMultiControlledRotateAroundAxis(
            qureg: Pin<&mut Qureg>,
            ctrls: &[i32],
            targ: i32,
            angle: f64,
            axisX: f64,
            axisY: f64,
            axisZ: f64,
        );
        fn applyMultiStateControlledRotateAroundAxis(
            qureg: Pin<&mut Qureg>,
            ctrls: &[i32],
            states: &[i32],
            targ: i32,
            angle: f64,
            axisX: f64,
            axisY: f64,
            axisZ: f64,
        );

        // Pauli gadget operations
        fn multiplyPauliGadget(qureg: Pin<&mut Qureg>, str: &PauliStr, angle: f64);
        fn applyPauliGadget(qureg: Pin<&mut Qureg>, str: &PauliStr, angle: f64);
        fn applyControlledPauliGadget(
            qureg: Pin<&mut Qureg>,
            control: i32,
            str: &PauliStr,
            angle: f64,
        );
        fn applyMultiControlledPauliGadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            str: &PauliStr,
            angle: f64,
        );
        fn applyMultiStateControlledPauliGadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            str: &PauliStr,
            angle: f64,
        );

        // Phase gadget operations
        fn multiplyPhaseGadget(qureg: Pin<&mut Qureg>, targets: &[i32], angle: f64);
        fn applyPhaseGadget(qureg: Pin<&mut Qureg>, targets: &[i32], angle: f64);
        fn applyControlledPhaseGadget(
            qureg: Pin<&mut Qureg>,
            control: i32,
            targets: &[i32],
            angle: f64,
        );
        fn applyMultiControlledPhaseGadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            targets: &[i32],
            angle: f64,
        );
        fn applyMultiStateControlledPhaseGadget(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
            angle: f64,
        );

        // Phase shifts and flips
        fn applyPhaseFlip(qureg: Pin<&mut Qureg>, target: i32);
        fn applyPhaseShift(qureg: Pin<&mut Qureg>, target: i32, angle: f64);
        fn applyTwoQubitPhaseFlip(qureg: Pin<&mut Qureg>, target1: i32, target2: i32);
        fn applyTwoQubitPhaseShift(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, angle: f64);
        fn applyMultiQubitPhaseFlip(qureg: Pin<&mut Qureg>, targets: &[i32]);
        fn applyMultiQubitPhaseShift(qureg: Pin<&mut Qureg>, targets: &[i32], angle: f64);

        // Multi-qubit NOT operations (aliases for X)
        fn multiplyMultiQubitNot(qureg: Pin<&mut Qureg>, targets: &[i32]);
        fn applyMultiQubitNot(qureg: Pin<&mut Qureg>, targets: &[i32]);
        fn applyControlledMultiQubitNot(qureg: Pin<&mut Qureg>, control: i32, targets: &[i32]);
        fn applyMultiControlledMultiQubitNot(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            numControls: i32,
            targets: &[i32],
        );
        fn applyMultiStateControlledMultiQubitNot(
            qureg: Pin<&mut Qureg>,
            controls: &[i32],
            states: &[i32],
            targets: &[i32],
        );

        // Superoperator
        fn applySuperOp(qureg: Pin<&mut Qureg>, targets: &[i32], superop: &SuperOp);

        // Measurement operations
        fn applyQubitMeasurement(qureg: Pin<&mut Qureg>, target: i32) -> i32;
        unsafe fn applyQubitMeasurementAndGetProb(
            qureg: Pin<&mut Qureg>,
            target: i32,
            probability: *mut f64,
        ) -> i32;
        fn applyForcedQubitMeasurement(qureg: Pin<&mut Qureg>, target: i32, outcome: i32) -> f64;
        fn applyQubitProjector(qureg: Pin<&mut Qureg>, target: i32, outcome: i32);

        fn applyMultiQubitMeasurement(qureg: Pin<&mut Qureg>, qubits: &[i32]) -> i64;
        unsafe fn applyMultiQubitMeasurementAndGetProb(
            qureg: Pin<&mut Qureg>,
            qubits: &[i32],
            probability: *mut f64,
        ) -> i64;
        fn applyForcedMultiQubitMeasurement(
            qureg: Pin<&mut Qureg>,
            qubits: &[i32],
            outcomes: &[i32],
        ) -> f64;
        fn applyMultiQubitProjector(qureg: Pin<&mut Qureg>, qubits: &[i32], outcomes: &[i32]);

        // QFT operations
        fn applyQuantumFourierTransform(qureg: Pin<&mut Qureg>, targets: &[i32], numTargets: i32);
        fn applyFullQuantumFourierTransform(qureg: Pin<&mut Qureg>);

        // Pauli string creation and management
        fn getPauliStr(paulis: String, indices: &[i32]) -> UniquePtr<PauliStr>;
        fn createInlinePauliStrSum(str: String) -> UniquePtr<PauliStrSum>;
        fn createPauliStrSumFromFile(fn_: String) -> UniquePtr<PauliStrSum>;
        fn createPauliStrSumFromReversedFile(fn_: String) -> UniquePtr<PauliStrSum>;
        fn destroyPauliStrSum(sum: Pin<&mut PauliStrSum>);
        fn reportPauliStr(str: Pin<&mut PauliStr>);
        fn reportPauliStrSum(str: Pin<&mut PauliStrSum>);
    }
}
