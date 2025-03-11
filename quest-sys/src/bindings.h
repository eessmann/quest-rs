#pragma once

#include "quest/include/quest.h"
#include <cstdint>
#include <memory>
#include <vector>
#include <complex>
#include "rust/cxx.h"

struct Quest_Complex {
    double re;
    double im;

    Quest_Complex() : re(0), im(0) {}
    Quest_Complex(const qcomp& c) : re(c.real()), im(c.imag()) {}
    operator qcomp() const { return qcomp(re, im); }
};


// Define some wrapper functions for Rust to interact with QuEST
namespace quest_sys {
    // Calculations
    qreal calcExpecPauliStr(Qureg const& qureg, PauliStr const& str);
    qreal calcExpecPauliStrSum(Qureg const& qureg, PauliStrSum const& sum);
    qreal calcExpecFullStateDiagMatr(Qureg const& qureg, FullStateDiagMatr const& matr);
    qreal calcExpecFullStateDiagMatrPower(Qureg const& qureg, FullStateDiagMatr const& matr, Quest_Complex exponent);

    qreal calcTotalProb(Qureg const& qureg);
    qreal calcProbOfBasisState(Qureg const& qureg , qindex index);
    qreal calcProbOfQubitOutcome(Qureg const& qureg, int qubit, int outcome);
    qreal calcProbOfMultiQubitOutcome(Qureg const& qureg, rust::Slice<int> qubits, rust::Slice<int> outcomes, int numQubits);
    void  calcProbsOfAllMultiQubitOutcomes(rust::Slice<qreal> outcomeProbs, Qureg const& qureg, rust::Slice<int> qubits, int numQubits);

    qreal calcPurity(Qureg const& qureg);
    qreal calcFidelity(Qureg const& qureg, Qureg const& other);
    qreal calcDistance(Qureg const& qureg1, Qureg const& qureg2);

    std::unique_ptr<Qureg> calcPartialTrace(Qureg const& qureg, rust::Slice<int> traceOutQubits, int numTraceQubits);
    std::unique_ptr<Qureg> calcReducedDensityMatrix(Qureg const& qureg, rust::Slice<int> retainQubits, int numRetainQubits);
    void setQuregToPartialTrace(Qureg& out, Qureg const& in, rust::Slice<int> traceOutQubits, int numTraceQubits);
    void setQuregToReducedDensityMatrix(Qureg& out, Qureg const& in, rust::Slice<int> retainQubits, int numRetainQubits);

    Quest_Complex calcInnerProduct(Qureg const& qureg1, Qureg const& qureg2);
    Quest_Complex calcExpecNonHermitianPauliStrSum(Qureg const& qureg, PauliStrSum const& sum);
    Quest_Complex calcExpecNonHermitianFullStateDiagMatr(Qureg const& qureg, FullStateDiagMatr const& matr);
    Quest_Complex calcExpecNonHermitianFullStateDiagMatrPower(Qureg const& qureg, FullStateDiagMatr const& matrix, Quest_Complex exponent);

    // Channel
    KrausMap createKrausMap(int numQubits, int numOperators);
    void syncKrausMap(KrausMap map);
    void destroyKrausMap(KrausMap map);
    void reportKrausMap(KrausMap map);

    SuperOp createSuperOp(int numQubits);
    void syncSuperOp(SuperOp op);
    void destroySuperOp(SuperOp op);
    void reportSuperOp(SuperOp op);

    void setKrausMap(KrausMap map, std::vector<std::vector<std::vector<qcomp>>> matrices);
    void setSuperOp(SuperOp op, std::vector<std::vector<qcomp>> matrix);

    void setInlineKrausMap(KrausMap map, int numQb, int numOps, std::vector<std::vector<std::vector<qcomp>>> matrices);
    void setInlineSuperOp(SuperOp op, int numQb, std::vector<std::vector<qcomp>> matrix);

    KrausMap createInlineKrausMap(int numQubits, int numOperators, std::vector<std::vector<std::vector<qcomp>>> matrices);
    SuperOp createInlineSuperOp(int numQubits, std::vector<std::vector<qcomp>> matrix);

    // Debug
    void setSeeds(unsigned* seeds, int numSeeds);
    void setSeedsToDefault();

    void getSeeds(unsigned* seeds);
    int getNumSeeds();

    void invalidQuESTInputError(const char* msg, const char* func);

    void setValidationOn();
    void setValidationOff();

    void setValidationEpsilonToDefault();
    void setValidationEpsilon(qreal eps);
    qreal getValidationEpsilon();

    void setMaxNumReportedItems(qindex numRows, qindex numCols);
    void setMaxNumReportedSigFigs(int numSigFigs);

    qindex getGpuCacheSize();
    void clearGpuCache();

    void getEnvironmentString(char str[200]);

    //  Decoherence
    void mixDephasing(Qureg qureg, int qubit, qreal prob);
    void mixTwoQubitDephasing(Qureg qureg, int qubit1, int qubit2, qreal prob);
    void mixDepolarising(Qureg qureg, int qubit, qreal prob);
    void mixTwoQubitDepolarising(Qureg qureg, int qubit1, int qubit2, qreal prob);
    void mixDamping(Qureg qureg, int qubit, qreal prob);
    void mixPaulis(Qureg qureg, int qubit, qreal probX, qreal probY, qreal probZ);
    void mixQureg(Qureg qureg, Qureg other, qreal prob);
    void mixKrausMap(Qureg qureg, int* qubits, int numQubits, KrausMap map);

    // Environment management
    void initQuESTEnv();
    void initCustomQuESTEnv(int useDistrib, int useGpuAccel, int useMultithread);
    void finalizeQuESTEnv();
    void syncQuESTEnv();
    void reportQuESTEnv();
    bool isQuESTEnvInit();
    QuESTEnv getQuESTEnv();

    // Initialisation
    void initBlankState(Qureg qureg);
    void initZeroState(Qureg qureg);
    void initPlusState(Qureg qureg);
    void initPureState(Qureg qureg, Qureg pure);
    void initClassicalState(Qureg qureg, qindex stateInd);
    void initDebugState(Qureg qureg);
    void initArbitraryPureState(Qureg qureg, qcomp* amps);
    void initRandomPureState(Qureg qureg);
    void initRandomMixedState(Qureg qureg, qindex numPureStates);

    void setQuregAmps(Qureg qureg, qindex startInd, qcomp* amps, qindex numAmps);
    void setDensityQuregAmps(Qureg qureg, qindex startRow, qindex startCol, qcomp** amps, qindex numRows, qindex numCols);
    void setDensityQuregFlatAmps(Qureg qureg, qindex startInd, qcomp* amps, qindex numAmps);
    void setQuregToClone(Qureg targetQureg, Qureg copyQureg);
    void setQuregToSuperposition(qcomp facOut, Qureg out, qcomp fac1, Qureg qureg1, qcomp fac2, Qureg qureg2);
    qreal setQuregToRenormalized(Qureg qureg);
    void setQuregToPauliStrSum(Qureg qureg, PauliStrSum sum);

    // Matrices
    CompMatr1 getCompMatr1(std::vector<std::vector<qcomp>> in);
    CompMatr2 getCompMatr2(std::vector<std::vector<qcomp>> in);

    DiagMatr1 getDiagMatr1(std::vector<qcomp> in);
    DiagMatr2 getDiagMatr2(std::vector<qcomp> in);

    CompMatr createCompMatr(int numQubits);
    DiagMatr createDiagMatr(int numQubits);
    FullStateDiagMatr createFullStateDiagMatr(int numQubits);
    FullStateDiagMatr createCustomFullStateDiagMatr(int numQubits, int useDistrib, int useGpuAccel);

    void destroyCompMatr(CompMatr matrix);
    void destroyDiagMatr(DiagMatr matrix);
    void destroyFullStateDiagMatr(FullStateDiagMatr matrix);

    void syncCompMatr(CompMatr matr);
    void syncDiagMatr(DiagMatr matr);
    void syncFullStateDiagMatr(FullStateDiagMatr matr);

    void setCompMatr(CompMatr out, std::vector<std::vector<qcomp>> in);
    void setDiagMatr(DiagMatr out, std::vector<qcomp> in);
    void setFullStateDiagMatr(FullStateDiagMatr out, qindex startInd, std::vector<qcomp> in);

    void setInlineCompMatr(CompMatr matr, int numQb, std::vector<std::vector<qcomp>> in);
    void setInlineDiagMatr(DiagMatr matr, int numQb, std::vector<qcomp> in);
    void setInlineFullStateDiagMatr(FullStateDiagMatr matr, qindex startInd, qindex numElems, std::vector<qcomp> in);

    CompMatr createInlineCompMatr(int numQb, std::vector<std::vector<qcomp>> elems);
    DiagMatr createInlineDiagMatr(int numQb, std::vector<qcomp> elems);

    void setDiagMatrFromMultiVarFunc(DiagMatr out, qcomp (*func)(qindex*), int* numQubitsPerVar, int numVars, int areSigned);
    void setDiagMatrFromMultiDimLists(DiagMatr out, void* lists, int* numQubitsPerDim, int numDims);

    FullStateDiagMatr createFullStateDiagMatrFromPauliStrSum(PauliStrSum in);
    void setFullStateDiagMatrFromPauliStrSum(FullStateDiagMatr out, PauliStrSum in);
    void setFullStateDiagMatrFromMultiVarFunc(FullStateDiagMatr out, qcomp (*func)(qindex*), int* numQubitsPerVar, int numVars, int areSigned);
    void setFullStateDiagMatrFromMultiDimLists(FullStateDiagMatr out, void* lists, int* numQubitsPerDim, int numDims);

    void reportCompMatr1(CompMatr1 matrix);
    void reportCompMatr2(CompMatr2 matrix);
    void reportCompMatr(CompMatr matrix);

    void reportDiagMatr1(DiagMatr1 matrix);
    void reportDiagMatr2(DiagMatr2 matrix);
    void reportDiagMatr(DiagMatr matrix);
    void reportFullStateDiagMatr(FullStateDiagMatr matr);

    // Operations
    /// CompMatr1
    void multiplyCompMatr1(Qureg qureg, int target, CompMatr1 matr);
    void applyCompMatr1(Qureg qureg, int target, CompMatr1 matr);
    void applyControlledCompMatr1(Qureg qureg, int control, int target, CompMatr1 matr);
    void applyMultiControlledCompMatr1(Qureg qureg, int* controls, int numControls, int target, CompMatr1 matr);
    void applyMultiStateControlledCompMatr1(Qureg qureg, int* controls, int* states, int numControls, int target, CompMatr1 matr);

    /// CompMatr2
    void multiplyCompMatr2(Qureg qureg, int target1, int target2, CompMatr2 matr);
    void applyCompMatr2(Qureg qureg, int target1, int target2, CompMatr2 matr);
    void applyControlledCompMatr2(Qureg qureg, int control, int target1, int target2, CompMatr2 matr);
    void applyMultiControlledCompMatr2(Qureg qureg, int* controls, int numControls, int target1, int target2, CompMatr2 matr);
    void applyMultiStateControlledCompMatr2(Qureg qureg, int* controls, int* states, int numControls, int target1, int target2, CompMatr2 matr);

    /// CompMatr
    void multiplyCompMatr(Qureg qureg, int* targets, int numTargets, CompMatr matr);
    void applyCompMatr(Qureg qureg, int* targets, int numTargets, CompMatr matr);
    void applyControlledCompMatr(Qureg qureg, int control, int* targets, int numTargets, CompMatr matr);
    void applyMultiControlledCompMatr(Qureg qureg, int* controls, int numControls, int* targets, int numTargets, CompMatr matr);
    void applyMultiStateControlledCompMatr(Qureg qureg, int* controls, int* states, int numControls, int* targets, int numTargets, CompMatr matr);

    /// DiagMatr1
    void multiplyDiagMatr1(Qureg qureg, int target, DiagMatr1 matr);
    void applyDiagMatr1(Qureg qureg, int target, DiagMatr1 matr);
    void applyControlledDiagMatr1(Qureg qureg, int control, int target, DiagMatr1 matr);
    void applyMultiControlledDiagMatr1(Qureg qureg, int* controls, int numControls, int target, DiagMatr1 matr);
    void applyMultiStateControlledDiagMatr1(Qureg qureg, int* controls, int* states, int numControls, int target, DiagMatr1 matr);

     /// DiagMatr2
    void multiplyDiagMatr2(Qureg qureg, int target1, int target2, DiagMatr2 matr);
    void applyDiagMatr2(Qureg qureg, int target1, int target2, DiagMatr2 matr);
    void applyControlledDiagMatr2(Qureg qureg, int control, int target1, int target2, DiagMatr2 matr);
    void applyMultiControlledDiagMatr2(Qureg qureg, int* controls, int numControls, int target1, int target2, DiagMatr2 matr);
    void applyMultiStateControlledDiagMatr2(Qureg qureg, int* controls, int* states, int numControls, int target1, int target2, DiagMatr2 matr);

    /// DiagMatr
    void multiplyDiagMatr(Qureg qureg, int* targets, int numTargets, DiagMatr matrix);
    void applyDiagMatr(Qureg qureg, int* targets, int numTargets, DiagMatr matrix);
    void applyControlledDiagMatr(Qureg, int control, int* targets, int numTargets, DiagMatr matrix);
    void applyMultiControlledDiagMatr(Qureg, int* controls, int numControls, int* targets, int numTargets, DiagMatr matrix);
    void applyMultiStateControlledDiagMatr(Qureg, int* controls, int* states, int numControls, int* targets, int numTargets, DiagMatr matrix);

    /// DiagMatrPower
    void multiplyDiagMatrPower(Qureg qureg, int* targets, int numTargets, DiagMatr matrix, qcomp exponent);
    void applyDiagMatrPower(Qureg qureg, int* targets, int numTargets, DiagMatr matrix, qcomp exponent);
    void applyControlledDiagMatrPower(Qureg qureg, int control, int* targets, int numTargets, DiagMatr matrix, qcomp exponent);
    void applyMultiControlledDiagMatrPower(Qureg qureg, int* controls, int numControls, int* targets, int numTargets, DiagMatr matrix, qcomp exponent);
    void applyMultiStateControlledDiagMatrPower(Qureg qureg, int* controls, int* states, int numControls, int* targets, int numTargets, DiagMatr matrix, qcomp exponent);

    /// FullStateDiagMatr
    void multiplyFullStateDiagMatr(Qureg qureg, FullStateDiagMatr matrix);
    void multiplyFullStateDiagMatrPower(Qureg qureg, FullStateDiagMatr matrix, qcomp exponent);
    void applyFullStateDiagMatr(Qureg qureg, FullStateDiagMatr matrix);
    void applyFullStateDiagMatrPower(Qureg qureg, FullStateDiagMatr matrix, qcomp exponent);

    /// S gate
    void applyS(Qureg qureg, int target);
    void applyControlledS(Qureg qureg, int control, int target);
    void applyMultiControlledS(Qureg qureg, int* controls, int numControls, int target);
    void applyMultiStateControlledS(Qureg qureg, int* controls, int* states, int numControls, int target);

    /// T gate
    void applyT(Qureg qureg, int target);
    void applyControlledT(Qureg qureg, int control, int target);
    void applyMultiControlledT(Qureg qureg, int* controls, int numControls, int target);
    void applyMultiStateControlledT(Qureg qureg, int* controls, int* states, int numControls, int target);
    /// Hadamard
    void applyHadamard(Qureg qureg, int target);
    void applyControlledHadamard(Qureg qureg, int control, int target);
    void applyMultiControlledHadamard(Qureg qureg, int* controls, int numControls, int target);
    void applyMultiStateControlledHadamard(Qureg qureg, int* controls, int* states, int numControls, int target);

    /// swaps
    void multiplySwap(Qureg qureg, int qubit1, int qubit2);
    void applySwap(Qureg qureg, int qubit1, int qubit2);
    void applyControlledSwap(Qureg qureg, int control, int qubit1, int qubit2);
    void applyMultiControlledSwap(Qureg qureg, int* controls, int numControls, int qubit1, int qubit2);
    void applyMultiStateControlledSwap(Qureg qureg, int* controls, int* states, int numControls, int qubit1, int qubit2);

     /// sqrt-swap
    void applySqrtSwap(Qureg qureg, int qubit1, int qubit2);
    void applyControlledSqrtSwap(Qureg qureg, int control, int qubit1, int qubit2);
    void applyMultiControlledSqrtSwap(Qureg qureg, int* controls, int numControls, int qubit1, int qubit2);
    void applyMultiStateControlledSqrtSwap(Qureg qureg, int* controls, int* states, int numControls, int qubit1, int qubit2);

    /// individual Paulis
    void multiplyPauliX(Qureg qureg, int target);
    void multiplyPauliY(Qureg qureg, int target);
    void multiplyPauliZ(Qureg qureg, int target);

    void applyPauliX(Qureg qureg, int target);
    void applyPauliY(Qureg qureg, int target);
    void applyPauliZ(Qureg qureg, int target);

    void applyControlledPauliX(Qureg qureg, int control, int target);
    void applyControlledPauliY(Qureg qureg, int control, int target);
    void applyControlledPauliZ(Qureg qureg, int control, int target);

    void applyMultiControlledPauliX(Qureg qureg, int* controls, int numControls, int target);
    void applyMultiControlledPauliY(Qureg qureg, int* controls, int numControls, int target);
    void applyMultiControlledPauliZ(Qureg qureg, int* controls, int numControls, int target);

    void applyMultiStateControlledPauliX(Qureg qureg, int* controls, int* states, int numControls, int target);
    void applyMultiStateControlledPauliY(Qureg qureg, int* controls, int* states, int numControls, int target);
    void applyMultiStateControlledPauliZ(Qureg qureg, int* controls, int* states, int numControls, int target);

    /// Pauli strings
    void multiplyPauliStr(Qureg qureg, PauliStr str);
    void applyPauliStr(Qureg qureg, PauliStr str);
    void applyControlledPauliStr(Qureg qureg, int control, PauliStr str);
    void applyMultiControlledPauliStr(Qureg qureg, int* controls, int numControls, PauliStr str);
    void applyMultiStateControlledPauliStr(Qureg qureg, int* controls, int* states, int numControls, PauliStr str);

    /// Pauli string sums
    void multiplyPauliStrSum(Qureg qureg, PauliStrSum sum, Qureg workspace);
    void applyTrotterizedPauliStrSumGadget(Qureg qureg, PauliStrSum sum, qreal angle, int order, int reps);

    /// individual axis rotations
    void applyRotateX(Qureg qureg, int target, qreal angle);
    void applyRotateY(Qureg qureg, int target, qreal angle);
    void applyRotateZ(Qureg qureg, int target, qreal angle);

    void applyControlledRotateX(Qureg qureg, int control, int target, qreal angle);
    void applyControlledRotateY(Qureg qureg, int control, int target, qreal angle);
    void applyControlledRotateZ(Qureg qureg, int control, int target, qreal angle);

    void applyMultiControlledRotateX(Qureg qureg, int* controls, int numControls, int target, qreal angle);
    void applyMultiControlledRotateY(Qureg qureg, int* controls, int numControls, int target, qreal angle);
    void applyMultiControlledRotateZ(Qureg qureg, int* controls, int numControls, int target, qreal angle);

    void applyMultiStateControlledRotateX(Qureg qureg, int* controls, int* states, int numControls, int target, qreal angle);
    void applyMultiStateControlledRotateY(Qureg qureg, int* controls, int* states, int numControls, int target, qreal angle);
    void applyMultiStateControlledRotateZ(Qureg qureg, int* controls, int* states, int numControls, int target, qreal angle);

    /// arbitrary axis rotation
    void applyRotateAroundAxis(Qureg qureg, int targ, qreal angle, qreal axisX, qreal axisY, qreal axisZ);
    void applyControlledRotateAroundAxis(Qureg qureg, int ctrl, int targ, qreal angle, qreal axisX, qreal axisY, qreal axisZ);
    void applyMultiControlledRotateAroundAxis(Qureg qureg, int* ctrls, int numCtrls, int targ, qreal angle, qreal axisX, qreal axisY, qreal axisZ);
    void applyMultiStateControlledRotateAroundAxis(Qureg qureg, int* ctrls, int* states, int numCtrls, int targ, qreal angle, qreal axisX, qreal axisY, qreal axisZ);

    /// Pauli gadgets
    void multiplyPauliGadget(Qureg qureg, PauliStr str, qreal angle);
    void applyPauliGadget(Qureg qureg, PauliStr str, qreal angle);
    void applyControlledPauliGadget(Qureg qureg, int control, PauliStr str, qreal angle);
    void applyMultiControlledPauliGadget(Qureg qureg, int* controls, int numControls, PauliStr str, qreal angle);
    void applyMultiStateControlledPauliGadget(Qureg qureg, int* controls, int* states, int numControls, PauliStr str, qreal angle);

    /// phase gadgets
    void multiplyPhaseGadget(Qureg qureg, int* targets, int numTargets, qreal angle);
    void applyPhaseGadget(Qureg qureg, int* targets, int numTargets, qreal angle);
    void applyControlledPhaseGadget(Qureg qureg, int control, int* targets, int numTargets, qreal angle);
    void applyMultiControlledPhaseGadget(Qureg qureg, int* controls, int numControls, int* targets, int numTargets, qreal angle);
    void applyMultiStateControlledPhaseGadget(Qureg qureg, int* controls, int* states, int numControls, int* targets, int numTargets, qreal angle);

    /// phase shifts and flips
    void applyPhaseFlip (Qureg qureg, int target);
    void applyPhaseShift(Qureg qureg, int target, qreal angle);
    void applyTwoQubitPhaseFlip( Qureg qureg, int target1, int target2);
    void applyTwoQubitPhaseShift(Qureg qureg, int target1, int target2, qreal angle);
    void applyMultiQubitPhaseFlip (Qureg qureg, int* targets, int numTargets);
    void applyMultiQubitPhaseShift(Qureg qureg, int* targets, int numTargets, qreal angle);

    /// many-qubit CNOTs (aliases for X)
    void multiplyMultiQubitNot(Qureg qureg, int* targets, int numTargets);
    void applyMultiQubitNot(Qureg, int* targets, int numTargets);
    void applyControlledMultiQubitNot(Qureg, int control, int* targets, int numTargets);
    void applyMultiControlledMultiQubitNot(Qureg, int* controls, int numControls, int* targets, int numTargets);
    void applyMultiStateControlledMultiQubitNot(Qureg, int* controls, int* states, int numControls, int* targets, int numTargets);

    /// superoperator
    void applySuperOp(Qureg qureg, int* targets, int numTargets, SuperOp superop);

    /// measurement
    int applyQubitMeasurement(Qureg qureg, int target);
    int applyQubitMeasurementAndGetProb(Qureg qureg, int target, qreal* probability);
    qreal applyForcedQubitMeasurement(Qureg qureg, int target, int outcome);
    void applyQubitProjector(Qureg qureg, int target, int outcome);
    qindex applyMultiQubitMeasurement(Qureg qureg, int* qubits, int numQubits);
    qindex applyMultiQubitMeasurementAndGetProb(Qureg qureg, int* qubits, int numQubits, qreal* probability);
    qreal applyForcedMultiQubitMeasurement(Qureg qureg, int* qubits, int* outcomes, int numQubits);
    void applyMultiQubitProjector(Qureg qureg, int* qubits, int* outcomes, int numQubits);

     /// QFT
    void applyQuantumFourierTransform(Qureg qureg, int* targets, int numTargets);
    void applyFullQuantumFourierTransform(Qureg qureg);

    // Pauli
    PauliStr getPauliStr(std::string paulis, std::vector<int> indices);
    PauliStrSum createPauliStrSum(std::vector<PauliStr> strings, std::vector<qcomp> coeffs);
    PauliStrSum createInlinePauliStrSum(std::string str);
    PauliStrSum createPauliStrSumFromFile(std::string fn);
    PauliStrSum createPauliStrSumFromReversedFile(std::string fn);
    void destroyPauliStrSum(PauliStrSum sum);
    void reportPauliStr(PauliStr str);
    void reportPauliStrSum(PauliStrSum str);

    // Qureg
    Qureg createQureg(int numQubits);
    Qureg createDensityQureg(int numQubits);

    Qureg createForcedQureg(int numQubits);
    Qureg createForcedDensityQureg(int numQubits);

    Qureg createCustomQureg(int numQubits, int isDensMatr, int useDistrib, int useGpuAccel, int useMultithread);

    Qureg createCloneQureg(Qureg qureg);

    void destroyQureg(Qureg qureg);

    void reportQuregParams(Qureg qureg);
    void reportQureg(Qureg qureg);
    void reportQuregToFile(Qureg qureg, char* fn);

    void syncQuregToGpu  (Qureg qureg);
    void syncQuregFromGpu(Qureg qureg);

    void syncSubQuregToGpu  (Qureg qureg, qindex localStartInd, qindex numLocalAmps);
    void syncSubQuregFromGpu(Qureg qureg, qindex localStartInd, qindex numLocalAmps);

    void getQuregAmps(qcomp* outAmps, Qureg qureg, qindex startInd, qindex numAmps);
    void getDensityQuregAmps(qcomp** outAmps, Qureg qureg, qindex startRow, qindex startCol, qindex numRows, qindex numCols);

    qcomp getQuregAmp(Qureg qureg, qindex index);
    qcomp getDensityQuregAmp(Qureg qureg, qindex row, qindex column);

    // Types
    qcomp getQcomp(qreal re, qreal im);
    void reportQcomp(qcomp num);
}