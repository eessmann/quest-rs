#pragma once

#include <complex>
#include <cstdint>
#include <memory>
#include <vector>
#include "quest/include/quest.h"
#include "rust/cxx.h"

struct Quest_Complex {
  double re;
  double im;

  Quest_Complex() : re(0), im(0) {}

  Quest_Complex(const qcomp& c) : re(c.real()), im(c.imag()) {}

  operator qcomp() const { return qcomp(re, im); }

  static Quest_Complex* from_qcomp_ptr(qcomp* ptr) {
    static_assert(sizeof(Quest_Complex) == sizeof(qcomp),
                  "Incompatible types for casting");
    static_assert(offsetof(Quest_Complex, re) == 0 &&
                      offsetof(Quest_Complex, im) == sizeof(double),
                  "Memory layout not as expected");
    return reinterpret_cast<Quest_Complex*>(ptr);
  }

  static qcomp* to_qcomp_ptr(Quest_Complex* ptr) {
    static_assert(sizeof(Quest_Complex) == sizeof(qcomp),
                  "Incompatible types for casting");
    static_assert(offsetof(Quest_Complex, re) == 0 &&
                      offsetof(Quest_Complex, im) == sizeof(double),
                  "Memory layout not as expected");
    return reinterpret_cast<qcomp*>(ptr);
  }
};

// Define some wrapper functions for Rust to interact with QuEST
namespace quest_sys {
// Calculations
qreal calcExpecPauliStr(const Qureg& qureg, const PauliStr& str);

qreal calcExpecPauliStrSum(const Qureg& qureg, const PauliStrSum& sum);

qreal calcExpecFullStateDiagMatr(const Qureg& qureg,
                                 const FullStateDiagMatr& matr);

qreal calcExpecFullStateDiagMatrPower(const Qureg& qureg,
                                      const FullStateDiagMatr& matr,
                                      Quest_Complex exponent);

qreal calcTotalProb(const Qureg& qureg);

qreal calcProbOfBasisState(const Qureg& qureg, qindex index);

qreal calcProbOfQubitOutcome(const Qureg& qureg, int qubit, int outcome);

qreal calcProbOfMultiQubitOutcome(const Qureg& qureg,
                                  rust::Slice<int> qubits,
                                  rust::Slice<int> outcomes,
                                  int numQubits);

void calcProbsOfAllMultiQubitOutcomes(rust::Slice<qreal> outcomeProbs,
                                      const Qureg& qureg,
                                      rust::Slice<int> qubits,
                                      int numQubits);

qreal calcPurity(const Qureg& qureg);

qreal calcFidelity(const Qureg& qureg, const Qureg& other);

qreal calcDistance(const Qureg& qureg1, const Qureg& qureg2);

std::unique_ptr<Qureg> calcPartialTrace(const Qureg& qureg,
                                        rust::Slice<int> traceOutQubits,
                                        int numTraceQubits);

std::unique_ptr<Qureg> calcReducedDensityMatrix(const Qureg& qureg,
                                                rust::Slice<int> retainQubits,
                                                int numRetainQubits);

void setQuregToPartialTrace(Qureg& out,
                            const Qureg& in,
                            rust::Slice<int> traceOutQubits,
                            int numTraceQubits);

void setQuregToReducedDensityMatrix(Qureg& out,
                                    const Qureg& in,
                                    rust::Slice<int> retainQubits,
                                    int numRetainQubits);

Quest_Complex calcInnerProduct(const Qureg& qureg1, const Qureg& qureg2);

Quest_Complex calcExpecNonHermitianPauliStrSum(const Qureg& qureg,
                                               const PauliStrSum& sum);

Quest_Complex calcExpecNonHermitianFullStateDiagMatr(
    const Qureg& qureg,
    const FullStateDiagMatr& matr);

Quest_Complex calcExpecNonHermitianFullStateDiagMatrPower(
    const Qureg& qureg,
    const FullStateDiagMatr& matrix,
    Quest_Complex exponent);

// Channel
std::unique_ptr<KrausMap> createKrausMap(int numQubits, int numOperators);

void syncKrausMap(KrausMap& map);

void destroyKrausMap(KrausMap& map);

void reportKrausMap(const KrausMap& map);

std::unique_ptr<SuperOp> createSuperOp(int numQubits);

void syncSuperOp(SuperOp& op);

void destroySuperOp(SuperOp& op);

void reportSuperOp(const SuperOp& op);

void setKrausMap(
    KrausMap& map,
    std::vector<std::vector<std::vector<Quest_Complex>>>& matrices);

void setSuperOp(SuperOp& op, std::vector<std::vector<Quest_Complex>>& matrix);

std::unique_ptr<KrausMap> createInlineKrausMap(
    int numQubits,
    int numOperators,
    std::vector<std::vector<std::vector<Quest_Complex>>>& matrices);

std::unique_ptr<SuperOp> createInlineSuperOp(
    int numQubits,
    std::vector<std::vector<Quest_Complex>>& matrix);

// Debug
void setSeeds(rust::Slice<const unsigned> seeds, int numSeeds);

void setSeedsToDefault();

std::vector<unsigned> getSeeds();

void invalidQuESTInputError(rust::String msg, rust::String func);

void setValidationOn();

void setValidationOff();

void setValidationEpsilonToDefault();

void setValidationEpsilon(qreal eps);

qreal getValidationEpsilon();

void setMaxNumReportedItems(qindex numRows, qindex numCols);

void setMaxNumReportedSigFigs(int numSigFigs);

qindex getGpuCacheSize();

void clearGpuCache();

rust::String getEnvironmentString();

//  Decoherence
void mixDephasing(Qureg& qureg, int qubit, qreal prob);

void mixTwoQubitDephasing(Qureg& qureg, int qubit1, int qubit2, qreal prob);

void mixDepolarising(Qureg& qureg, int qubit, qreal prob);

void mixTwoQubitDepolarising(Qureg& qureg, int qubit1, int qubit2, qreal prob);

void mixDamping(Qureg& qureg, int qubit, qreal prob);

void mixPaulis(Qureg& qureg, int qubit, qreal probX, qreal probY, qreal probZ);

void mixQureg(Qureg& qureg, Qureg& other, qreal prob);

void mixKrausMap(Qureg& qureg,
                 rust::Slice<const int> qubits,
                 const KrausMap& map);

// Environment management
void initQuESTEnv();

void initCustomQuESTEnv(int useDistrib, int useGpuAccel, int useMultithread);

void finalizeQuESTEnv();

void syncQuESTEnv();

void reportQuESTEnv();

bool isQuESTEnvInit();

std::unique_ptr<QuESTEnv> getQuESTEnv();

// Initialisation
void initBlankState(Qureg& qureg);

void initZeroState(Qureg& qureg);

void initPlusState(Qureg& qureg);

void initPureState(Qureg& qureg, Qureg& pure);

void initClassicalState(Qureg& qureg, qindex stateInd);

void initDebugState(Qureg& qureg);

void initArbitraryPureState(Qureg& qureg,
                            rust::Slice<const Quest_Complex> amps);

void initRandomPureState(Qureg& qureg);

void initRandomMixedState(Qureg& qureg, qindex numPureStates);

void setQuregAmps(Qureg& qureg,
                  qindex startInd,
                  rust::Slice<const Quest_Complex> amps);

void setDensityQuregAmps(
    Qureg& qureg,
    qindex startRow,
    qindex startCol,
    rust::Slice<const rust::Slice<const Quest_Complex>> amps);

void setDensityQuregFlatAmps(Qureg& qureg,
                             qindex startInd,
                             rust::Slice<const Quest_Complex> amps);

void setQuregToClone(Qureg& targetQureg, const Qureg& copyQureg);

void setQuregToSuperposition(Quest_Complex facOut,
                             Qureg& out,
                             Quest_Complex fac1,
                             const Qureg& qureg1,
                             qcomp fac2,
                             const Qureg& qureg2);

qreal setQuregToRenormalized(Qureg& qureg);

void setQuregToPauliStrSum(Qureg& qureg, const PauliStrSum& sum);

// Matrices
std::unique_ptr<CompMatr1> getCompMatr1(
    rust::Slice<const rust::Slice<const Quest_Complex>> in);

std::unique_ptr<CompMatr2> getCompMatr2(
    rust::Slice<const rust::Slice<const Quest_Complex>> in);

std::unique_ptr<DiagMatr1> getDiagMatr1(rust::Slice<const Quest_Complex> in);

std::unique_ptr<DiagMatr2> getDiagMatr2(rust::Slice<const Quest_Complex> in);

std::unique_ptr<CompMatr> createCompMatr(int numQubits);

std::unique_ptr<DiagMatr> createDiagMatr(int numQubits);

std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatr(int numQubits);

std::unique_ptr<FullStateDiagMatr>
createCustomFullStateDiagMatr(int numQubits, int useDistrib, int useGpuAccel);

void destroyCompMatr(CompMatr& matrix);

void destroyDiagMatr(DiagMatr& matrix);

void destroyFullStateDiagMatr(FullStateDiagMatr& matrix);

void syncCompMatr(CompMatr& matr);

void syncDiagMatr(DiagMatr& matr);

void syncFullStateDiagMatr(FullStateDiagMatr& matr);

void setCompMatr(CompMatr& out,
                 rust::Slice<const rust::Slice<const Quest_Complex>> in);

void setDiagMatr(DiagMatr& out, rust::Slice<const Quest_Complex> in);

void setFullStateDiagMatr(FullStateDiagMatr& out,
                          qindex startInd,
                          rust::Slice<const Quest_Complex> in);

std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatrFromPauliStrSum(
    const PauliStrSum& in);

void setFullStateDiagMatrFromPauliStrSum(FullStateDiagMatr& out,
                                         const PauliStrSum& in);

void setFullStateDiagMatrFromMultiVarFunc(
    FullStateDiagMatr& out,
    rust::Fn<Quest_Complex(rust::Slice<const qindex>)> func,
    rust::Slice<const int> numQubitsPerVar,
    int numVars,
    int areSigned);

void reportCompMatr1(const CompMatr1& matrix);

void reportCompMatr2(const CompMatr2& matrix);

void reportCompMatr(const CompMatr& matrix);

void reportDiagMatr1(const DiagMatr1& matrix);

void reportDiagMatr2(const DiagMatr2& matrix);

void reportDiagMatr(const DiagMatr& matrix);

void reportFullStateDiagMatr(const FullStateDiagMatr& matr);

// Operations
/// CompMatr1
void multiplyCompMatr1(Qureg& qureg, int target, const CompMatr1& matr);

void applyCompMatr1(Qureg& qureg, int target, const CompMatr1& matr);

void applyControlledCompMatr1(Qureg& qureg,
                              int control,
                              int target,
                              const CompMatr1& matr);

void applyMultiControlledCompMatr1(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int numControls,
                                   int target,
                                   const CompMatr1& matr);

void applyMultiStateControlledCompMatr1(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int numControls,
                                        int target,
                                        const CompMatr1& matr);

/// CompMatr2
void multiplyCompMatr2(Qureg& qureg,
                       int target1,
                       int target2,
                       const CompMatr2& matr);

void applyCompMatr2(Qureg& qureg,
                    int target1,
                    int target2,
                    const CompMatr2& matr);

void applyControlledCompMatr2(Qureg& qureg,
                              int control,
                              int target1,
                              int target2,
                              const CompMatr2& matr);

void applyMultiControlledCompMatr2(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int numControls,
                                   int target1,
                                   int target2,
                                   const CompMatr2& matr);

void applyMultiStateControlledCompMatr2(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int numControls,
                                        int target1,
                                        int target2,
                                        const CompMatr2& matr);

/// CompMatr
void multiplyCompMatr(Qureg& qureg, rust::Slice<const int> targets, int numTargets, const CompMatr& matr);

void applyCompMatr(Qureg& qureg, int* targets, int numTargets, const CompMatr& matr);

void applyControlledCompMatr(Qureg& qureg,
                             int control,
                             rust::Slice<const int> targets,
                             int numTargets,
                             const CompMatr& matr);

void applyMultiControlledCompMatr(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  int numControls,
                                  rust::Slice<const int> targets,
                                  int numTargets,
                                  const CompMatr& matr);

void applyMultiStateControlledCompMatr(Qureg qureg,
                                       int* controls,
                                       int* states,
                                       int numControls,
                                       int* targets,
                                       int numTargets,
                                       CompMatr matr);

/// DiagMatr1
void multiplyDiagMatr1(Qureg qureg, int target, DiagMatr1 matr);

void applyDiagMatr1(Qureg qureg, int target, DiagMatr1 matr);

void applyControlledDiagMatr1(Qureg qureg,
                              int control,
                              int target,
                              DiagMatr1 matr);

void applyMultiControlledDiagMatr1(Qureg qureg,
                                   int* controls,
                                   int numControls,
                                   int target,
                                   DiagMatr1 matr);

void applyMultiStateControlledDiagMatr1(Qureg qureg,
                                        int* controls,
                                        int* states,
                                        int numControls,
                                        int target,
                                        DiagMatr1 matr);

/// DiagMatr2
void multiplyDiagMatr2(Qureg qureg, int target1, int target2, DiagMatr2 matr);

void applyDiagMatr2(Qureg qureg, int target1, int target2, DiagMatr2 matr);

void applyControlledDiagMatr2(Qureg qureg,
                              int control,
                              int target1,
                              int target2,
                              DiagMatr2 matr);

void applyMultiControlledDiagMatr2(Qureg qureg,
                                   int* controls,
                                   int numControls,
                                   int target1,
                                   int target2,
                                   DiagMatr2 matr);

void applyMultiStateControlledDiagMatr2(Qureg qureg,
                                        int* controls,
                                        int* states,
                                        int numControls,
                                        int target1,
                                        int target2,
                                        DiagMatr2 matr);

/// DiagMatr
void multiplyDiagMatr(Qureg qureg,
                      int* targets,
                      int numTargets,
                      DiagMatr matrix);

void applyDiagMatr(Qureg qureg, int* targets, int numTargets, DiagMatr matrix);

void applyControlledDiagMatr(Qureg,
                             int control,
                             int* targets,
                             int numTargets,
                             DiagMatr matrix);

void applyMultiControlledDiagMatr(Qureg,
                                  int* controls,
                                  int numControls,
                                  int* targets,
                                  int numTargets,
                                  DiagMatr matrix);

void applyMultiStateControlledDiagMatr(Qureg,
                                       int* controls,
                                       int* states,
                                       int numControls,
                                       int* targets,
                                       int numTargets,
                                       DiagMatr matrix);

/// DiagMatrPower
void multiplyDiagMatrPower(Qureg qureg,
                           int* targets,
                           int numTargets,
                           DiagMatr matrix,
                           Quest_Complex exponent);

void applyDiagMatrPower(Qureg qureg,
                        int* targets,
                        int numTargets,
                        DiagMatr matrix,
                        Quest_Complex exponent);

void applyControlledDiagMatrPower(Qureg qureg,
                                  int control,
                                  int* targets,
                                  int numTargets,
                                  DiagMatr matrix,
                                  Quest_Complex exponent);

void applyMultiControlledDiagMatrPower(Qureg qureg,
                                       int* controls,
                                       int numControls,
                                       int* targets,
                                       int numTargets,
                                       DiagMatr matrix,
                                       Quest_Complex exponent);

void applyMultiStateControlledDiagMatrPower(Qureg qureg,
                                            int* controls,
                                            int* states,
                                            int numControls,
                                            int* targets,
                                            int numTargets,
                                            DiagMatr matrix,
                                            Quest_Complex exponent);

/// FullStateDiagMatr
void multiplyFullStateDiagMatr(Qureg qureg, FullStateDiagMatr matrix);

void multiplyFullStateDiagMatrPower(Qureg qureg,
                                    FullStateDiagMatr matrix,
                                    Quest_Complex exponent);

void applyFullStateDiagMatr(Qureg qureg, FullStateDiagMatr matrix);

void applyFullStateDiagMatrPower(Qureg qureg,
                                 FullStateDiagMatr matrix,
                                 Quest_Complex exponent);

/// S gate
void applyS(Qureg qureg, int target);

void applyControlledS(Qureg qureg, int control, int target);

void applyMultiControlledS(Qureg qureg,
                           int* controls,
                           int numControls,
                           int target);

void applyMultiStateControlledS(Qureg qureg,
                                int* controls,
                                int* states,
                                int numControls,
                                int target);

/// T gate
void applyT(Qureg qureg, int target);

void applyControlledT(Qureg qureg, int control, int target);

void applyMultiControlledT(Qureg qureg,
                           int* controls,
                           int numControls,
                           int target);

void applyMultiStateControlledT(Qureg qureg,
                                int* controls,
                                int* states,
                                int numControls,
                                int target);

/// Hadamard
void applyHadamard(Qureg qureg, int target);

void applyControlledHadamard(Qureg qureg, int control, int target);

void applyMultiControlledHadamard(Qureg qureg,
                                  int* controls,
                                  int numControls,
                                  int target);

void applyMultiStateControlledHadamard(Qureg qureg,
                                       int* controls,
                                       int* states,
                                       int numControls,
                                       int target);

/// swaps
void multiplySwap(Qureg qureg, int qubit1, int qubit2);

void applySwap(Qureg qureg, int qubit1, int qubit2);

void applyControlledSwap(Qureg qureg, int control, int qubit1, int qubit2);

void applyMultiControlledSwap(Qureg qureg,
                              int* controls,
                              int numControls,
                              int qubit1,
                              int qubit2);

void applyMultiStateControlledSwap(Qureg qureg,
                                   int* controls,
                                   int* states,
                                   int numControls,
                                   int qubit1,
                                   int qubit2);

/// sqrt-swap
void applySqrtSwap(Qureg qureg, int qubit1, int qubit2);

void applyControlledSqrtSwap(Qureg qureg, int control, int qubit1, int qubit2);

void applyMultiControlledSqrtSwap(Qureg qureg,
                                  int* controls,
                                  int numControls,
                                  int qubit1,
                                  int qubit2);

void applyMultiStateControlledSqrtSwap(Qureg qureg,
                                       int* controls,
                                       int* states,
                                       int numControls,
                                       int qubit1,
                                       int qubit2);

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

void applyMultiControlledPauliX(Qureg qureg,
                                int* controls,
                                int numControls,
                                int target);

void applyMultiControlledPauliY(Qureg qureg,
                                int* controls,
                                int numControls,
                                int target);

void applyMultiControlledPauliZ(Qureg qureg,
                                int* controls,
                                int numControls,
                                int target);

void applyMultiStateControlledPauliX(Qureg qureg,
                                     int* controls,
                                     int* states,
                                     int numControls,
                                     int target);

void applyMultiStateControlledPauliY(Qureg qureg,
                                     int* controls,
                                     int* states,
                                     int numControls,
                                     int target);

void applyMultiStateControlledPauliZ(Qureg qureg,
                                     int* controls,
                                     int* states,
                                     int numControls,
                                     int target);

/// Pauli strings
void multiplyPauliStr(Qureg qureg, PauliStr str);

void applyPauliStr(Qureg qureg, PauliStr str);

void applyControlledPauliStr(Qureg qureg, int control, PauliStr str);

void applyMultiControlledPauliStr(Qureg qureg,
                                  int* controls,
                                  int numControls,
                                  PauliStr str);

void applyMultiStateControlledPauliStr(Qureg qureg,
                                       int* controls,
                                       int* states,
                                       int numControls,
                                       PauliStr str);

/// Pauli string sums
void multiplyPauliStrSum(Qureg qureg, PauliStrSum sum, Qureg workspace);

void applyTrotterizedPauliStrSumGadget(Qureg qureg,
                                       PauliStrSum sum,
                                       qreal angle,
                                       int order,
                                       int reps);

/// individual axis rotations
void applyRotateX(Qureg qureg, int target, qreal angle);

void applyRotateY(Qureg qureg, int target, qreal angle);

void applyRotateZ(Qureg qureg, int target, qreal angle);

void applyControlledRotateX(Qureg qureg, int control, int target, qreal angle);

void applyControlledRotateY(Qureg qureg, int control, int target, qreal angle);

void applyControlledRotateZ(Qureg qureg, int control, int target, qreal angle);

void applyMultiControlledRotateX(Qureg qureg,
                                 int* controls,
                                 int numControls,
                                 int target,
                                 qreal angle);

void applyMultiControlledRotateY(Qureg qureg,
                                 int* controls,
                                 int numControls,
                                 int target,
                                 qreal angle);

void applyMultiControlledRotateZ(Qureg qureg,
                                 int* controls,
                                 int numControls,
                                 int target,
                                 qreal angle);

void applyMultiStateControlledRotateX(Qureg qureg,
                                      int* controls,
                                      int* states,
                                      int numControls,
                                      int target,
                                      qreal angle);

void applyMultiStateControlledRotateY(Qureg qureg,
                                      int* controls,
                                      int* states,
                                      int numControls,
                                      int target,
                                      qreal angle);

void applyMultiStateControlledRotateZ(Qureg qureg,
                                      int* controls,
                                      int* states,
                                      int numControls,
                                      int target,
                                      qreal angle);

/// arbitrary axis rotation
void applyRotateAroundAxis(Qureg qureg,
                           int targ,
                           qreal angle,
                           qreal axisX,
                           qreal axisY,
                           qreal axisZ);

void applyControlledRotateAroundAxis(Qureg qureg,
                                     int ctrl,
                                     int targ,
                                     qreal angle,
                                     qreal axisX,
                                     qreal axisY,
                                     qreal axisZ);

void applyMultiControlledRotateAroundAxis(Qureg qureg,
                                          int* ctrls,
                                          int numCtrls,
                                          int targ,
                                          qreal angle,
                                          qreal axisX,
                                          qreal axisY,
                                          qreal axisZ);

void applyMultiStateControlledRotateAroundAxis(Qureg qureg,
                                               int* ctrls,
                                               int* states,
                                               int numCtrls,
                                               int targ,
                                               qreal angle,
                                               qreal axisX,
                                               qreal axisY,
                                               qreal axisZ);

/// Pauli gadgets
void multiplyPauliGadget(Qureg qureg, PauliStr str, qreal angle);

void applyPauliGadget(Qureg qureg, PauliStr str, qreal angle);

void applyControlledPauliGadget(Qureg qureg,
                                int control,
                                PauliStr str,
                                qreal angle);

void applyMultiControlledPauliGadget(Qureg qureg,
                                     int* controls,
                                     int numControls,
                                     PauliStr str,
                                     qreal angle);

void applyMultiStateControlledPauliGadget(Qureg qureg,
                                          int* controls,
                                          int* states,
                                          int numControls,
                                          PauliStr str,
                                          qreal angle);

/// phase gadgets
void multiplyPhaseGadget(Qureg qureg,
                         int* targets,
                         int numTargets,
                         qreal angle);

void applyPhaseGadget(Qureg qureg, int* targets, int numTargets, qreal angle);

void applyControlledPhaseGadget(Qureg qureg,
                                int control,
                                int* targets,
                                int numTargets,
                                qreal angle);

void applyMultiControlledPhaseGadget(Qureg qureg,
                                     int* controls,
                                     int numControls,
                                     int* targets,
                                     int numTargets,
                                     qreal angle);

void applyMultiStateControlledPhaseGadget(Qureg qureg,
                                          int* controls,
                                          int* states,
                                          int numControls,
                                          int* targets,
                                          int numTargets,
                                          qreal angle);

/// phase shifts and flips
void applyPhaseFlip(Qureg qureg, int target);

void applyPhaseShift(Qureg qureg, int target, qreal angle);

void applyTwoQubitPhaseFlip(Qureg qureg, int target1, int target2);

void applyTwoQubitPhaseShift(Qureg qureg,
                             int target1,
                             int target2,
                             qreal angle);

void applyMultiQubitPhaseFlip(Qureg qureg, int* targets, int numTargets);

void applyMultiQubitPhaseShift(Qureg qureg,
                               int* targets,
                               int numTargets,
                               qreal angle);

/// many-qubit CNOTs (aliases for X)
void multiplyMultiQubitNot(Qureg qureg, int* targets, int numTargets);

void applyMultiQubitNot(Qureg, int* targets, int numTargets);

void applyControlledMultiQubitNot(Qureg,
                                  int control,
                                  int* targets,
                                  int numTargets);

void applyMultiControlledMultiQubitNot(Qureg,
                                       int* controls,
                                       int numControls,
                                       int* targets,
                                       int numTargets);

void applyMultiStateControlledMultiQubitNot(Qureg,
                                            int* controls,
                                            int* states,
                                            int numControls,
                                            int* targets,
                                            int numTargets);

/// superoperator
void applySuperOp(Qureg qureg, int* targets, int numTargets, SuperOp superop);

/// measurement
int applyQubitMeasurement(Qureg qureg, int target);

int applyQubitMeasurementAndGetProb(Qureg qureg,
                                    int target,
                                    qreal* probability);

qreal applyForcedQubitMeasurement(Qureg qureg, int target, int outcome);

void applyQubitProjector(Qureg qureg, int target, int outcome);

qindex applyMultiQubitMeasurement(Qureg qureg, int* qubits, int numQubits);

qindex applyMultiQubitMeasurementAndGetProb(Qureg qureg,
                                            int* qubits,
                                            int numQubits,
                                            qreal* probability);

qreal applyForcedMultiQubitMeasurement(Qureg qureg,
                                       int* qubits,
                                       int* outcomes,
                                       int numQubits);

void applyMultiQubitProjector(Qureg qureg,
                              int* qubits,
                              int* outcomes,
                              int numQubits);

/// QFT
void applyQuantumFourierTransform(Qureg qureg, int* targets, int numTargets);

void applyFullQuantumFourierTransform(Qureg qureg);

// Pauli
PauliStr getPauliStr(std::string paulis, std::vector<int> indices);

PauliStrSum createPauliStrSum(std::vector<PauliStr> strings,
                              std::vector<Quest_Complex> coeffs);

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

Qureg createCustomQureg(int numQubits,
                        int isDensMatr,
                        int useDistrib,
                        int useGpuAccel,
                        int useMultithread);

Qureg createCloneQureg(Qureg qureg);

void destroyQureg(Qureg qureg);

void reportQuregParams(Qureg qureg);

void reportQureg(Qureg qureg);

void reportQuregToFile(Qureg qureg, char* fn);

void syncQuregToGpu(Qureg qureg);

void syncQuregFromGpu(Qureg qureg);

void syncSubQuregToGpu(Qureg qureg, qindex localStartInd, qindex numLocalAmps);

void syncSubQuregFromGpu(Qureg qureg,
                         qindex localStartInd,
                         qindex numLocalAmps);

void getQuregAmps(Quest_Complex* outAmps,
                  Qureg qureg,
                  qindex startInd,
                  qindex numAmps);

void getDensityQuregAmps(Quest_Complex** outAmps,
                         Qureg qureg,
                         qindex startRow,
                         qindex startCol,
                         qindex numRows,
                         qindex numCols);

Quest_Complex getQuregAmp(Qureg qureg, qindex index);

Quest_Complex getDensityQuregAmp(Qureg qureg, qindex row, qindex column);

// Types
void reportQcomp(Quest_Complex num);
}  // namespace quest_sys
