#[cxx::bridge]
pub mod ffi {
    // Share these types across all bridges
    #[derive(Debug, Clone, Copy)]
    pub struct Quest_Complex {
        pub re: f64,
        pub im: f64,
    }

    unsafe extern "C++" {
        include!("types.hpp");

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
        type QuESTEnv;

        // Common type
        type Quest_Complex;
    }

    // Calculations
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("calculations.hpp");
        // Expectation value calculations
        fn calcExpecPauliStr(qureg: &Qureg, str: &PauliStr) -> f64;
        fn calcExpecPauliStrSum(qureg: &Qureg, sum: &PauliStrSum) -> f64;
        fn calcExpecFullStateDiagMatr(qureg: &Qureg, matr: &FullStateDiagMatr) -> f64;
        fn calcExpecFullStateDiagMatrPower(qureg: &Qureg, matr: &FullStateDiagMatr, exponent: Quest_Complex) -> f64;

        // Probability calculations
        fn calcTotalProb(qureg: &Qureg) -> f64;
        fn calcProbOfBasisState(qureg: &Qureg, index: i64) -> f64;
        fn calcProbOfQubitOutcome(qureg: &Qureg, qubit: i32, outcome: i32) -> f64;
        fn calcProbOfMultiQubitOutcome(qureg: &Qureg, qubits: &[i32], outcomes: &[i32]) -> f64;
        fn calcProbsOfAllMultiQubitOutcomes(outcomeProbs: &mut [f64], qureg: &Qureg, qubits: &[i32]);

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
        fn calcExpecNonHermitianFullStateDiagMatr(qureg: &Qureg, matr: &FullStateDiagMatr) -> Quest_Complex;
        fn calcExpecNonHermitianFullStateDiagMatrPower(qureg: &Qureg, matrix: &FullStateDiagMatr, exponent: Quest_Complex) -> Quest_Complex;
    }

    // Channels
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("channels.hpp");
        fn createKrausMap(numQubits: i32, numOperators: i32) -> UniquePtr<KrausMap>;
        fn syncKrausMap(map: Pin<&mut KrausMap>);
        fn destroyKrausMap(map: Pin<&mut KrausMap>);
        fn reportKrausMap(map: &KrausMap);

        fn createSuperOp(numQubuts: i32) -> UniquePtr<SuperOp>;
        fn syncSuperOp(map: Pin<&mut SuperOp>);
        fn destroySuperOp(map: Pin<&mut SuperOp>);
        fn reportSuperOp(map: &SuperOp);

        fn setKrausMap(map: Pin<&mut KrausMap>, matrices: &[&[&[Quest_Complex]]]);
        fn setSuperOp(map: Pin<&mut SuperOp>, matrix: &[&[Quest_Complex]]);
        fn createInlineKrausMap(numQubits: i32, numOperators: i32, matrices: &[&[&[Quest_Complex]]]) -> UniquePtr<KrausMap>;
        fn createInlineSuperOp(numQubits: i32, matrix: &[&[Quest_Complex]]) -> UniquePtr<SuperOp>;
    }

    // Debug
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("debug.hpp");
        // Random number generators
        fn setSeeds(seeds: &[u32]);
        fn setSeedsToDefault();
        fn getSeeds() -> Vec<u32>;

        // Validation
        fn invalidQuESTInputError(msg: String, func: String);
        fn setValidationOn();
        fn setValidationOff();
        fn setValidationEpsilonToDefault();
        fn setValidationEpsilon(eps: f64);
        fn getValidationEpsilon() -> f64;

        // Reporting
        fn setMaxNumReportedItems(numRows: i64, numCols: i64);
        fn setMaxNumReportedSigFigs(numSigFigs: i32);

        // GPU cache
        fn getGpuCacheSize() -> i64;
        fn clearGpuCache();

        // Environment 
        fn getEnvironmentString() -> String;
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

    // Environment management
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("environment.hpp");
        fn initQuESTEnv();
        fn initCustomQuESTEnv(useDistrib: bool, useGpuAccel: bool, useMultithread: bool);
        fn finalizeQuESTEnv();
        fn syncQuESTEnv();
        fn reportQuESTEnv();
        fn isQuESTEnvInit() -> bool;
        fn getQuESTEnv() -> UniquePtr<QuESTEnv>;
    }

    // Initialisation
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("initialisation.hpp");
        // State initialization
        fn initBlankState(qureg: Pin<&mut Qureg>);
        fn initZeroState(qureg: Pin<&mut Qureg>);
        fn initPlusState(qureg: Pin<&mut Qureg>);
        fn initPureState(qureg: Pin<&mut Qureg>, pure: Pin<&mut Qureg>);
        fn initClassicalState(qureg: Pin<&mut Qureg>, stateInd: i64);
        fn initDebugState(qureg: Pin<&mut Qureg>);
        fn initArbitraryPureState(qureg: Pin<&mut Qureg>, amps: &[Quest_Complex]);
        fn initRandomPureState(qureg: Pin<&mut Qureg>);
        fn initRandomMixedState(qureg: Pin<&mut Qureg>, numPureStates: i64);

        // Setting amplitudes
        fn setQuregAmps(qureg: Pin<&mut Qureg>, startInd: i64, amps: &[Quest_Complex]);
        fn setDensityQuregAmps(qureg: Pin<&mut Qureg>, startRow: i64, startCol: i64, amps: &[&[Quest_Complex]]);
        fn setDensityQuregFlatAmps(qureg: Pin<&mut Qureg>, startInd: i64, amps: &[Quest_Complex]);

        // Qureg manipulation
        fn setQuregToClone(targetQureg: Pin<&mut Qureg>, copyQureg: &Qureg);
        fn setQuregToSuperposition(facOut: Quest_Complex, out: Pin<&mut Qureg>, fac1: Quest_Complex, qureg1: &Qureg, fac2: Quest_Complex, qureg2: &Qureg);
        fn setQuregToRenormalized(qureg: Pin<&mut Qureg>) -> f64;
        fn setQuregToPauliStrSum(qureg: Pin<&mut Qureg>, sum: &PauliStrSum);
    }

    // Matrices
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("matrices.hpp");
        // Matrix creation and destruction
        fn getCompMatr1(in_: &[&[Quest_Complex]]) -> UniquePtr<CompMatr1>;
        fn getCompMatr2(in_: &[&[Quest_Complex]]) -> UniquePtr<CompMatr2>;
        fn getDiagMatr1(in_: &[Quest_Complex]) -> UniquePtr<DiagMatr1>;
        fn getDiagMatr2(in_: &[Quest_Complex]) -> UniquePtr<DiagMatr2>;

        fn createCompMatr(numQubits: i32) -> UniquePtr<CompMatr>;
        fn createDiagMatr(numQubits: i32) -> UniquePtr<DiagMatr>;
        fn createFullStateDiagMatr(numQubits: i32) -> UniquePtr<FullStateDiagMatr>;
        fn createCustomFullStateDiagMatr(numQubits: i32, useDistrib: i32, useGpuAccel: i32) -> UniquePtr<FullStateDiagMatr>;

        fn destroyCompMatr(matrix: Pin<&mut CompMatr>);
        fn destroyDiagMatr(matrix: Pin<&mut DiagMatr>);
        fn destroyFullStateDiagMatr(matrix: Pin<&mut FullStateDiagMatr>);

        // Matrix synchronization
        fn syncCompMatr(matr: Pin<&mut CompMatr>);
        fn syncDiagMatr(matr: Pin<&mut DiagMatr>);
        fn syncFullStateDiagMatr(matr: Pin<&mut FullStateDiagMatr>);

        // Setting matrix values
        fn setCompMatr(out: Pin<&mut CompMatr>, in_: &[&[Quest_Complex]]);
        fn setDiagMatr(out: Pin<&mut DiagMatr>, in_: &[Quest_Complex]);
        fn setFullStateDiagMatr(out: Pin<&mut FullStateDiagMatr>, startInd: i64, in_: &[Quest_Complex]);

        // Special matrix creation
        fn createFullStateDiagMatrFromPauliStrSum(in_: &PauliStrSum) -> UniquePtr<FullStateDiagMatr>;
        fn setFullStateDiagMatrFromPauliStrSum(out: Pin<&mut FullStateDiagMatr>, in_: &PauliStrSum);

        // Matrix reporting
        fn reportCompMatr1(matrix: &CompMatr1);
        fn reportCompMatr2(matrix: &CompMatr2);
        fn reportCompMatr(matrix: &CompMatr);
        fn reportDiagMatr1(matrix: &DiagMatr1);
        fn reportDiagMatr2(matrix: &DiagMatr2);
        fn reportDiagMatr(matrix: &DiagMatr);
        fn reportFullStateDiagMatr(matr: &FullStateDiagMatr);
    }

    // Operations
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("operations.hpp");
        // CompMatr1 operations
        fn multiplyCompMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &CompMatr1);
        fn applyCompMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &CompMatr1);
        fn applyControlledCompMatr1(qureg: Pin<&mut Qureg>, control: i32, target: i32, matr: &CompMatr1);
        fn applyMultiControlledCompMatr1(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32, matr: &CompMatr1);
        fn applyMultiStateControlledCompMatr1(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32, matr: &CompMatr1);

        // CompMatr2 operations
        fn multiplyCompMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &CompMatr2);
        fn applyCompMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &CompMatr2);
        fn applyControlledCompMatr2(qureg: Pin<&mut Qureg>, control: i32, target1: i32, target2: i32, matr: &CompMatr2);
        fn applyMultiControlledCompMatr2(qureg: Pin<&mut Qureg>, controls: &[i32], numControls: i32, target1: i32, target2: i32, matr: &CompMatr2);
        fn applyMultiStateControlledCompMatr2(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target1: i32, target2: i32, matr: &CompMatr2);

        // CompMatr operations
        fn multiplyCompMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matr: &CompMatr);
        fn applyCompMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matr: &CompMatr);
        fn applyControlledCompMatr(qureg: Pin<&mut Qureg>, control: i32, targets: &[i32], matr: &CompMatr);
        fn applyMultiControlledCompMatr(qureg: Pin<&mut Qureg>, controls: &[i32], targets: &[i32], matr: &CompMatr);
        fn applyMultiStateControlledCompMatr(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], targets: &[i32], matr: &CompMatr);

        // DiagMatr1 operations
        fn multiplyDiagMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &DiagMatr1);
        fn applyDiagMatr1(qureg: Pin<&mut Qureg>, target: i32, matr: &DiagMatr1);
        fn applyControlledDiagMatr1(qureg: Pin<&mut Qureg>, control: i32, target: i32, matr: &DiagMatr1);
        fn applyMultiControlledDiagMatr1(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32, matr: &DiagMatr1);
        fn applyMultiStateControlledDiagMatr1(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32, matr: &DiagMatr1);

        // DiagMatr2 operations
        fn multiplyDiagMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &DiagMatr2);
        fn applyDiagMatr2(qureg: Pin<&mut Qureg>, target1: i32, target2: i32, matr: &DiagMatr2);
        fn applyControlledDiagMatr2(qureg: Pin<&mut Qureg>, control: i32, target1: i32, target2: i32, matr: &DiagMatr2);
        fn applyMultiControlledDiagMatr2(qureg: Pin<&mut Qureg>, controls: &[i32], target1: i32, target2: i32, matr: &DiagMatr2);
        fn applyMultiStateControlledDiagMatr2(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target1: i32, target2: i32, matr: &DiagMatr2);

        // DiagMatr operations
        fn multiplyDiagMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matrix: &DiagMatr);
        fn applyDiagMatr(qureg: Pin<&mut Qureg>, targets: &[i32], matrix: &DiagMatr);
        fn applyControlledDiagMatr(qureg: Pin<&mut Qureg>, control: i32, targets: &[i32], matrix: &DiagMatr);
        fn applyMultiControlledDiagMatr(qureg: Pin<&mut Qureg>, controls: &[i32], targets: &[i32], matrix: &DiagMatr);
        fn applyMultiStateControlledDiagMatr(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], targets: &[i32], matrix: &DiagMatr);

        // DiagMatrPower operations
        fn multiplyDiagMatrPower(qureg: Pin<&mut Qureg>, targets: &[i32], matrix: &DiagMatr, exponent: Quest_Complex);
        fn applyDiagMatrPower(qureg: Pin<&mut Qureg>, targets: &[i32], matrix: &DiagMatr, exponent: Quest_Complex);
        fn applyControlledDiagMatrPower(qureg: Pin<&mut Qureg>, control: i32, targets: &[i32], matrix: &DiagMatr, exponent: Quest_Complex);
        fn applyMultiControlledDiagMatrPower(qureg: Pin<&mut Qureg>, controls: &[i32], targets: &[i32], matrix: &DiagMatr, exponent: Quest_Complex);
        fn applyMultiStateControlledDiagMatrPower(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], targets: &[i32], matrix: &DiagMatr, exponent: Quest_Complex);

        // FullStateDiagMatr operations
        fn multiplyFullStateDiagMatr(qureg: Pin<&mut Qureg>, matrix: &FullStateDiagMatr);
        fn multiplyFullStateDiagMatrPower(qureg: Pin<&mut Qureg>, matrix: &FullStateDiagMatr, exponent: Quest_Complex);
        fn applyFullStateDiagMatr(qureg: Pin<&mut Qureg>, matrix: &FullStateDiagMatr);
        fn applyFullStateDiagMatrPower(qureg: Pin<&mut Qureg>, matrix: &FullStateDiagMatr, exponent: Quest_Complex);

        // S gate operations
        fn applyS(qureg: Pin<&mut Qureg>, target: i32);
        fn applyControlledS(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyMultiControlledS(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);
        fn applyMultiStateControlledS(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32);

        // T gate operations
        fn applyT(qureg: Pin<&mut Qureg>, target: i32);
        fn applyControlledT(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyMultiControlledT(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32);
        fn applyMultiStateControlledT(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32);

        // Hadamard operations
        fn applyHadamard(qureg: Pin<&mut Qureg>, target: i32);
        fn applyControlledHadamard(qureg: Pin<&mut Qureg>, control: i32, target: i32);
        fn applyMultiControlledHadamard(qureg: Pin<&mut Qureg>, controls: &[i32],  target: i32);
        fn applyMultiStateControlledHadamard(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32);

        // Swap operations
        fn multiplySwap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32);
        fn applySwap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32);
        fn applyControlledSwap(qureg: Pin<&mut Qureg>, control: i32, qubit1: i32, qubit2: i32);
        fn applyMultiControlledSwap(qureg: Pin<&mut Qureg>, controls: &[i32], qubit1: i32, qubit2: i32);
        fn applyMultiStateControlledSwap(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], qubit1: i32, qubit2: i32);

        // Sqrt-swap operations
        fn applySqrtSwap(qureg: Pin<&mut Qureg>, qubit1: i32, qubit2: i32);
        fn applyControlledSqrtSwap(qureg: Pin<&mut Qureg>, control: i32, qubit1: i32, qubit2: i32);
        fn applyMultiControlledSqrtSwap(qureg: Pin<&mut Qureg>, controls: &[i32], qubit1: i32, qubit2: i32);
        fn applyMultiStateControlledSqrtSwap(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], qubit1: i32, qubit2: i32);

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

        fn applyMultiStateControlledPauliX(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32);
        fn applyMultiStateControlledPauliY(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32);
        fn applyMultiStateControlledPauliZ(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32);

        // Pauli string operations
        fn multiplyPauliStr(qureg: Pin<&mut Qureg>, str: &PauliStr);
        fn applyPauliStr(qureg: Pin<&mut Qureg>, str: &PauliStr);
        fn applyControlledPauliStr(qureg: Pin<&mut Qureg>, control: i32, str: &PauliStr);
        fn applyMultiControlledPauliStr(qureg: Pin<&mut Qureg>, controls: &[i32], numControls: i32, str: &PauliStr);
        fn applyMultiStateControlledPauliStr(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], str: &PauliStr);

        // Pauli string sum operations
        fn multiplyPauliStrSum(qureg: Pin<&mut Qureg>, sum: &PauliStrSum, workspace: Pin<&mut Qureg>);
        fn applyTrotterizedPauliStrSumGadget(qureg: Pin<&mut Qureg>, sum: &PauliStrSum, angle: f64, order: i32, reps: i32);

        // Rotation operations
        fn applyRotateX(qureg: Pin<&mut Qureg>, target: i32, angle: f64);
        fn applyRotateY(qureg: Pin<&mut Qureg>, target: i32, angle: f64);
        fn applyRotateZ(qureg: Pin<&mut Qureg>, target: i32, angle: f64);

        fn applyControlledRotateX(qureg: Pin<&mut Qureg>, control: i32, target: i32, angle: f64);
        fn applyControlledRotateY(qureg: Pin<&mut Qureg>, control: i32, target: i32, angle: f64);
        fn applyControlledRotateZ(qureg: Pin<&mut Qureg>, control: i32, target: i32, angle: f64);

        fn applyMultiControlledRotateX(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32, angle: f64);
        fn applyMultiControlledRotateY(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32, angle: f64);
        fn applyMultiControlledRotateZ(qureg: Pin<&mut Qureg>, controls: &[i32], target: i32, angle: f64);

        fn applyMultiStateControlledRotateX(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32, angle: f64);
        fn applyMultiStateControlledRotateY(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32, angle: f64);
        fn applyMultiStateControlledRotateZ(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], target: i32, angle: f64);

        // Arbitrary axis rotation
        fn applyRotateAroundAxis(qureg: Pin<&mut Qureg>, targ: i32, angle: f64, axisX: f64, axisY: f64, axisZ: f64);
        fn applyControlledRotateAroundAxis(qureg: Pin<&mut Qureg>, ctrl: i32, targ: i32, angle: f64, axisX: f64, axisY: f64, axisZ: f64);
        fn applyMultiControlledRotateAroundAxis(qureg: Pin<&mut Qureg>, ctrls: &[i32], targ: i32, angle: f64, axisX: f64, axisY: f64, axisZ: f64);
        fn applyMultiStateControlledRotateAroundAxis(qureg: Pin<&mut Qureg>, ctrls: &[i32], states: &[i32], targ: i32, angle: f64, axisX: f64, axisY: f64, axisZ: f64);

        // Pauli gadget operations
        fn multiplyPauliGadget(qureg: Pin<&mut Qureg>, str: &PauliStr, angle: f64);
        fn applyPauliGadget(qureg: Pin<&mut Qureg>, str: &PauliStr, angle: f64);
        fn applyControlledPauliGadget(qureg: Pin<&mut Qureg>, control: i32, str: &PauliStr, angle: f64);
        fn applyMultiControlledPauliGadget(qureg: Pin<&mut Qureg>, controls: &[i32], str: &PauliStr, angle: f64);
        fn applyMultiStateControlledPauliGadget(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], str: &PauliStr, angle: f64);

        // Phase gadget operations
        fn multiplyPhaseGadget(qureg: Pin<&mut Qureg>, targets: &[i32], angle: f64);
        fn applyPhaseGadget(qureg: Pin<&mut Qureg>, targets: &[i32], angle: f64);
        fn applyControlledPhaseGadget(qureg: Pin<&mut Qureg>, control: i32, targets: &[i32], angle: f64);
        fn applyMultiControlledPhaseGadget(qureg: Pin<&mut Qureg>, controls: &[i32], targets: &[i32], angle: f64);
        fn applyMultiStateControlledPhaseGadget(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], targets: &[i32], angle: f64);

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
        fn applyMultiControlledMultiQubitNot(qureg: Pin<&mut Qureg>, controls: &[i32], numControls: i32, targets: &[i32]);
        fn applyMultiStateControlledMultiQubitNot(qureg: Pin<&mut Qureg>, controls: &[i32], states: &[i32], targets: &[i32]);

        // Superoperator
        fn applySuperOp(qureg: Pin<&mut Qureg>, targets: &[i32], superop: &SuperOp);

        // Measurement operations
        fn applyQubitMeasurement(qureg: Pin<&mut Qureg>, target: i32) -> i32;
        unsafe fn applyQubitMeasurementAndGetProb(qureg: Pin<&mut Qureg>, target: i32, probability: *mut f64) -> i32;
        fn applyForcedQubitMeasurement(qureg: Pin<&mut Qureg>, target: i32, outcome: i32) -> f64;
        fn applyQubitProjector(qureg: Pin<&mut Qureg>, target: i32, outcome: i32);

        fn applyMultiQubitMeasurement(qureg: Pin<&mut Qureg>, qubits: &[i32]) -> i64;
        unsafe fn applyMultiQubitMeasurementAndGetProb(qureg: Pin<&mut Qureg>, qubits: &[i32], probability: *mut f64) -> i64;
        fn applyForcedMultiQubitMeasurement(qureg: Pin<&mut Qureg>, qubits: &[i32], outcomes: &[i32]) -> f64;
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

    // Qureg
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("qureg.hpp");
        // Qureg creation and destruction
        fn createQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createDensityQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createForcedQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createForcedDensityQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createCustomQureg(numQubits: i32, isDensMatr: i32, useDistrib: i32, useGpuAccel: i32, useMultithread: i32) -> UniquePtr<Qureg>;
        fn createCloneQureg(qureg: &Qureg) -> UniquePtr<Qureg>;
        fn destroyQureg(qureg: Pin<&mut Qureg>);

        // Qureg reporting
        fn reportQuregParams(qureg: &Qureg);
        fn reportQureg(qureg: &Qureg);

        // Qureg synchronization
        fn syncQuregToGpu(qureg: Pin<&mut Qureg>);
        fn syncQuregFromGpu(qureg: Pin<&mut Qureg>);
        fn syncSubQuregToGpu(qureg: Pin<&mut Qureg>, localStartInd: i64, numLocalAmps: i64);
        fn syncSubQuregFromGpu(qureg: Pin<&mut Qureg>, localStartInd: i64, numLocalAmps: i64);

        // Qureg amplitude access
        fn getQuregAmps(qureg: Pin<&mut Qureg>, startInd: i64, numAmps: i64) -> Vec<Quest_Complex>;
        fn getDensityQuregAmps_flatten(qureg: Pin<&mut Qureg>, startRow: i64, startCol: i64, numRows: i64, numCols: i64) -> Vec<Quest_Complex>;
        fn getQuregAmp(qureg: Pin<&mut Qureg>, index: i64) -> Quest_Complex;
        fn getDensityQuregAmp(qureg: Pin<&mut Qureg>, row: i64, column: i64) -> Quest_Complex;
    }
}

pub use ffi::*;


/// Convenience function to create a quest_complex
pub fn complex(re: f64, im: f64) -> Quest_Complex {
    Quest_Complex { re, im }
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
