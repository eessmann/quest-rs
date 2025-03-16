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

using Quest_Real = double;
using Quest_Index = std::int64_t;

// Define some wrapper functions for Rust to interact with QuEST
namespace quest_sys {
// Calculations
Quest_Real calcExpecPauliStr(const Qureg& qureg, const PauliStr& str);

Quest_Real calcExpecPauliStrSum(const Qureg& qureg, const PauliStrSum& sum);

Quest_Real calcExpecFullStateDiagMatr(const Qureg& qureg,
                                      const FullStateDiagMatr& matr);

Quest_Real calcExpecFullStateDiagMatrPower(const Qureg& qureg,
                                           const FullStateDiagMatr& matr,
                                           Quest_Complex exponent);

Quest_Real calcTotalProb(const Qureg& qureg);

Quest_Real calcProbOfBasisState(const Qureg& qureg, Quest_Index index);

Quest_Real calcProbOfQubitOutcome(const Qureg& qureg, int qubit, int outcome);

Quest_Real calcProbOfMultiQubitOutcome(const Qureg& qureg,
                                       rust::Slice<const int> qubits,
                                       rust::Slice<const int> outcomes);

void calcProbsOfAllMultiQubitOutcomes(rust::Slice<Quest_Real> outcomeProbs,
                                      const Qureg& qureg,
                                      rust::Slice<const int> qubits);

Quest_Real calcPurity(const Qureg& qureg);

Quest_Real calcFidelity(const Qureg& qureg, const Qureg& other);

Quest_Real calcDistance(const Qureg& qureg1, const Qureg& qureg2);

std::unique_ptr<Qureg> calcPartialTrace(const Qureg& qureg,
                                        rust::Slice<const int> traceOutQubits);

std::unique_ptr<Qureg> calcReducedDensityMatrix(
    const Qureg& qureg,
    rust::Slice<const int> retainQubits);

void setQuregToPartialTrace(Qureg& out,
                            const Qureg& in,
                            rust::Slice<const int> traceOutQubits);

void setQuregToReducedDensityMatrix(Qureg& out,
                                    const Qureg& in,
                                    rust::Slice<const int> retainQubits);

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
    rust::Slice<const rust::Slice<const rust::Slice<const Quest_Complex>>> matrices);

void setSuperOp(SuperOp& op,
                rust::Slice<const rust::Slice<const Quest_Complex>> matrix);

std::unique_ptr<KrausMap> createInlineKrausMap(
    int numQubits,
    int numOperators,
    rust::Slice<const rust::Slice<const rust::Slice<const Quest_Complex>>> matrices);

std::unique_ptr<SuperOp> createInlineSuperOp(
    int numQubits,
    rust::Slice<const rust::Slice<const Quest_Complex>> matrix);

// Debug
void setSeeds(rust::Slice<const unsigned> seeds);

void setSeedsToDefault();

rust::Vec<unsigned> getSeeds();

void invalidQuESTInputError(rust::String msg, rust::String func);

void setValidationOn();

void setValidationOff();

void setValidationEpsilonToDefault();

void setValidationEpsilon(Quest_Real eps);

Quest_Real getValidationEpsilon();

void setMaxNumReportedItems(Quest_Index numRows, Quest_Index numCols);

void setMaxNumReportedSigFigs(int numSigFigs);

Quest_Index getGpuCacheSize();

void clearGpuCache();

rust::String getEnvironmentString();

//  Decoherence
void mixDephasing(Qureg& qureg, int qubit, Quest_Real prob);

void mixTwoQubitDephasing(Qureg& qureg,
                          int qubit1,
                          int qubit2,
                          Quest_Real prob);

void mixDepolarising(Qureg& qureg, int qubit, Quest_Real prob);

void mixTwoQubitDepolarising(Qureg& qureg,
                             int qubit1,
                             int qubit2,
                             Quest_Real prob);

void mixDamping(Qureg& qureg, int qubit, Quest_Real prob);

void mixPaulis(Qureg& qureg,
               int qubit,
               Quest_Real probX,
               Quest_Real probY,
               Quest_Real probZ);

void mixQureg(Qureg& qureg, Qureg& other, Quest_Real prob);

void mixKrausMap(Qureg& qureg,
                 rust::Slice<const int> qubits,
                 const KrausMap& map);

// Environment management
void initQuESTEnv();

void initCustomQuESTEnv(bool useDistrib, bool useGpuAccel, bool useMultithread);

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

void initClassicalState(Qureg& qureg, Quest_Index stateInd);

void initDebugState(Qureg& qureg);

void initArbitraryPureState(Qureg& qureg,
                            rust::Slice<const Quest_Complex> amps);

void initRandomPureState(Qureg& qureg);

void initRandomMixedState(Qureg& qureg, Quest_Index numPureStates);

void setQuregAmps(Qureg& qureg,
                  Quest_Index startInd,
                  rust::Slice<const Quest_Complex> amps);

void setDensityQuregAmps(
    Qureg& qureg,
    Quest_Index startRow,
    Quest_Index startCol,
    rust::Slice<const rust::Slice<const Quest_Complex>> amps);

void setDensityQuregFlatAmps(Qureg& qureg,
                             Quest_Index startInd,
                             rust::Slice<const Quest_Complex> amps);

void setQuregToClone(Qureg& targetQureg, const Qureg& copyQureg);

void setQuregToSuperposition(Quest_Complex facOut,
                             Qureg& out,
                             Quest_Complex fac1,
                             const Qureg& qureg1,
                             Quest_Complex fac2,
                             const Qureg& qureg2);

Quest_Real setQuregToRenormalized(Qureg& qureg);

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
                          Quest_Index startInd,
                          rust::Slice<const Quest_Complex> in);

std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatrFromPauliStrSum(
    const PauliStrSum& in);

void setFullStateDiagMatrFromPauliStrSum(FullStateDiagMatr& out,
                                         const PauliStrSum& in);

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
                                   int target,
                                   const CompMatr1& matr);

void applyMultiStateControlledCompMatr1(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
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
                                        int target1,
                                        int target2,
                                        const CompMatr2& matr);

/// CompMatr
void multiplyCompMatr(Qureg& qureg,
                      rust::Slice<const int> targets,
                      const CompMatr& matr);

void applyCompMatr(Qureg& qureg,
                   rust::Slice<const int> targets,
                   const CompMatr& matr);

void applyControlledCompMatr(Qureg& qureg,
                             int control,
                             rust::Slice<const int> targets,
                             const CompMatr& matr);

void applyMultiControlledCompMatr(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  rust::Slice<const int> targets,
                                  const CompMatr& matr);

void applyMultiStateControlledCompMatr(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       rust::Slice<const int> targets,
                                       const CompMatr& matr);

/// DiagMatr1
void multiplyDiagMatr1(Qureg& qureg, int target, const DiagMatr1& matr);

void applyDiagMatr1(Qureg& qureg, int target, const DiagMatr1& matr);

void applyControlledDiagMatr1(Qureg& qureg,
                              int control,
                              int target,
                              const DiagMatr1& matr);

void applyMultiControlledDiagMatr1(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int target,
                                   const DiagMatr1& matr);

void applyMultiStateControlledDiagMatr1(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int target,
                                        const DiagMatr1& matr);

/// DiagMatr2
void multiplyDiagMatr2(Qureg& qureg,
                       int target1,
                       int target2,
                       const DiagMatr2& matr);

void applyDiagMatr2(Qureg& qureg,
                    int target1,
                    int target2,
                    const DiagMatr2& matr);

void applyControlledDiagMatr2(Qureg& qureg,
                              int control,
                              int target1,
                              int target2,
                              const DiagMatr2& matr);

void applyMultiControlledDiagMatr2(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int target1,
                                   int target2,
                                   const DiagMatr2& matr);

void applyMultiStateControlledDiagMatr2(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int target1,
                                        int target2,
                                        const DiagMatr2& matr);

/// DiagMatr
void multiplyDiagMatr(Qureg& qureg,
                      rust::Slice<const int> targets,
                      const DiagMatr& matrix);

void applyDiagMatr(Qureg& qureg,
                   rust::Slice<const int> targets,
                   const DiagMatr& matrix);

void applyControlledDiagMatr(Qureg& qureg,
                             int control,
                             rust::Slice<const int> targets,
                             const DiagMatr& matrix);

void applyMultiControlledDiagMatr(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  rust::Slice<const int> targets,
                                  const DiagMatr& matrix);

void applyMultiStateControlledDiagMatr(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       rust::Slice<const int> targets,
                                       const DiagMatr& matrix);

/// DiagMatrPower
void multiplyDiagMatrPower(Qureg& qureg,
                           rust::Slice<const int> targets,
                           const DiagMatr& matrix,
                           Quest_Complex exponent);

void applyDiagMatrPower(Qureg& qureg,
                        rust::Slice<const int> targets,
                        const DiagMatr& matrix,
                        Quest_Complex exponent);

void applyControlledDiagMatrPower(Qureg& qureg,
                                  int control,
                                  rust::Slice<const int> targets,
                                  const DiagMatr& matrix,
                                  Quest_Complex exponent);

void applyMultiControlledDiagMatrPower(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> targets,
                                       const DiagMatr& matrix,
                                       Quest_Complex exponent);

void applyMultiStateControlledDiagMatrPower(Qureg& qureg,
                                            rust::Slice<const int> controls,
                                            rust::Slice<const int> states,
                                            rust::Slice<const int> targets,
                                            const DiagMatr& matrix,
                                            Quest_Complex exponent);

/// FullStateDiagMatr
void multiplyFullStateDiagMatr(Qureg& qureg, const FullStateDiagMatr& matrix);

void multiplyFullStateDiagMatrPower(Qureg& qureg,
                                    const FullStateDiagMatr& matrix,
                                    Quest_Complex exponent);

void applyFullStateDiagMatr(Qureg& qureg, const FullStateDiagMatr& matrix);

void applyFullStateDiagMatrPower(Qureg& qureg,
                                 const FullStateDiagMatr& matrix,
                                 Quest_Complex exponent);

/// S gate
void applyS(Qureg& qureg, int target);

void applyControlledS(Qureg& qureg, int control, int target);

void applyMultiControlledS(Qureg& qureg,
                           rust::Slice<const int> controls,
                           int target);

void applyMultiStateControlledS(Qureg& qureg,
                                rust::Slice<const int> controls,
                                rust::Slice<const int> states,
                                int target);

/// T gate
void applyT(Qureg& qureg, int target);

void applyControlledT(Qureg& qureg, int control, int target);

void applyMultiControlledT(Qureg& qureg,
                           rust::Slice<const int> controls,
                           int target);

void applyMultiStateControlledT(Qureg& qureg,
                                rust::Slice<const int> controls,
                                rust::Slice<const int> states,
                                int target);

/// Hadamard
void applyHadamard(Qureg& qureg, int target);

void applyControlledHadamard(Qureg& qureg, int control, int target);

void applyMultiControlledHadamard(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  int target);

void applyMultiStateControlledHadamard(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       int target);

/// swaps
void multiplySwap(Qureg& qureg, int qubit1, int qubit2);

void applySwap(Qureg& qureg, int qubit1, int qubit2);

void applyControlledSwap(Qureg& qureg, int control, int qubit1, int qubit2);

void applyMultiControlledSwap(Qureg& qureg,
                              rust::Slice<const int> controls,
                              int qubit1,
                              int qubit2);

void applyMultiStateControlledSwap(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   rust::Slice<const int> states,
                                   int qubit1,
                                   int qubit2);

/// sqrt-swap
void applySqrtSwap(Qureg& qureg, int qubit1, int qubit2);

void applyControlledSqrtSwap(Qureg& qureg, int control, int qubit1, int qubit2);

void applyMultiControlledSqrtSwap(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  int qubit1,
                                  int qubit2);

void applyMultiStateControlledSqrtSwap(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       int qubit1,
                                       int qubit2);

/// individual Paulis
void multiplyPauliX(Qureg& qureg, int target);
void multiplyPauliY(Qureg& qureg, int target);
void multiplyPauliZ(Qureg& qureg, int target);

void applyPauliX(Qureg& qureg, int target);
void applyPauliY(Qureg& qureg, int target);
void applyPauliZ(Qureg& qureg, int target);

void applyControlledPauliX(Qureg& qureg, int control, int target);
void applyControlledPauliY(Qureg& qureg, int control, int target);
void applyControlledPauliZ(Qureg& qureg, int control, int target);

void applyMultiControlledPauliX(Qureg& qureg,
                                rust::Slice<const int> controls,
                                int target);
void applyMultiControlledPauliY(Qureg& qureg,
                                rust::Slice<const int> controls,
                                int target);
void applyMultiControlledPauliZ(Qureg& qureg,
                                rust::Slice<const int> controls,
                                int target);

void applyMultiStateControlledPauliX(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> states,
                                     int target);
void applyMultiStateControlledPauliY(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> states,
                                     int target);
void applyMultiStateControlledPauliZ(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> states,
                                     int target);

/// Pauli strings
void multiplyPauliStr(Qureg& qureg, const PauliStr& str);

void applyPauliStr(Qureg& qureg, const PauliStr& str);

void applyControlledPauliStr(Qureg& qureg, int control, const PauliStr& str);

void applyMultiControlledPauliStr(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  int numControls,
                                  const PauliStr& str);

void applyMultiStateControlledPauliStr(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       const PauliStr& str);

/// Pauli string sums
void multiplyPauliStrSum(Qureg& qureg, const PauliStrSum& sum, Qureg& workspace);

void applyTrotterizedPauliStrSumGadget(Qureg& qureg,
                                       const PauliStrSum& sum,
                                       Quest_Real angle,
                                       int order,
                                       int reps);

/// individual axis rotations
void applyRotateX(Qureg& qureg, int target, Quest_Real angle);
void applyRotateY(Qureg& qureg, int target, Quest_Real angle);
void applyRotateZ(Qureg& qureg, int target, Quest_Real angle);

void applyControlledRotateX(Qureg& qureg,
                            int control,
                            int target,
                            Quest_Real angle);
void applyControlledRotateY(Qureg& qureg,
                            int control,
                            int target,
                            Quest_Real angle);
void applyControlledRotateZ(Qureg& qureg,
                            int control,
                            int target,
                            Quest_Real angle);

void applyMultiControlledRotateX(Qureg& qureg,
                                 rust::Slice<const int> controls,
                                 int target,
                                 Quest_Real angle);
void applyMultiControlledRotateY(Qureg& qureg,
                                 rust::Slice<const int> controls,
                                 int target,
                                 Quest_Real angle);
void applyMultiControlledRotateZ(Qureg& qureg,
                                 rust::Slice<const int> controls,
                                 int target,
                                 Quest_Real angle);

void applyMultiStateControlledRotateX(Qureg& qureg,
                                      rust::Slice<const int> controls,
                                      rust::Slice<const int> states,
                                      int target,
                                      Quest_Real angle);
void applyMultiStateControlledRotateY(Qureg& qureg,
                                      rust::Slice<const int> controls,
                                      rust::Slice<const int> states,
                                      int target,
                                      Quest_Real angle);
void applyMultiStateControlledRotateZ(Qureg& qureg,
                                      rust::Slice<const int> controls,
                                      rust::Slice<const int> states,
                                      int target,
                                      Quest_Real angle);

/// arbitrary axis rotation
void applyRotateAroundAxis(Qureg& qureg,
                           int targ,
                           Quest_Real angle,
                           Quest_Real axisX,
                           Quest_Real axisY,
                           Quest_Real axisZ);

void applyControlledRotateAroundAxis(Qureg& qureg,
                                     int ctrl,
                                     int targ,
                                     Quest_Real angle,
                                     Quest_Real axisX,
                                     Quest_Real axisY,
                                     Quest_Real axisZ);

void applyMultiControlledRotateAroundAxis(Qureg& qureg,
                                          rust::Slice<const int> ctrls,
                                          int targ,
                                          Quest_Real angle,
                                          Quest_Real axisX,
                                          Quest_Real axisY,
                                          Quest_Real axisZ);

void applyMultiStateControlledRotateAroundAxis(Qureg& qureg,
                                               rust::Slice<const int> ctrls,
                                               rust::Slice<const int> states,
                                               int targ,
                                               Quest_Real angle,
                                               Quest_Real axisX,
                                               Quest_Real axisY,
                                               Quest_Real axisZ);

/// Pauli gadgets
void multiplyPauliGadget(Qureg& qureg, const PauliStr& str, Quest_Real angle);

void applyPauliGadget(Qureg& qureg, const PauliStr& str, Quest_Real angle);

void applyControlledPauliGadget(Qureg& qureg,
                                int control,
                                const PauliStr& str,
                                Quest_Real angle);

void applyMultiControlledPauliGadget(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     const PauliStr& str,
                                     Quest_Real angle);

void applyMultiStateControlledPauliGadget(Qureg& qureg,
                                          rust::Slice<const int> controls,
                                          rust::Slice<const int> states,
                                          const PauliStr& str,
                                          Quest_Real angle);

/// phase gadgets
void multiplyPhaseGadget(Qureg& qureg,
                         rust::Slice<const int> targets,
                         Quest_Real angle);

void applyPhaseGadget(Qureg& qureg,
                      rust::Slice<const int> targets,
                      Quest_Real angle);

void applyControlledPhaseGadget(Qureg& qureg,
                                int control,
                                rust::Slice<const int> targets,
                                Quest_Real angle);

void applyMultiControlledPhaseGadget(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> targets,
                                     Quest_Real angle);

void applyMultiStateControlledPhaseGadget(Qureg& qureg,
                                          rust::Slice<const int> controls,
                                          rust::Slice<const int> states,
                                          rust::Slice<const int> targets,
                                          Quest_Real angle);

/// phase shifts and flips
void applyPhaseFlip(Qureg& qureg, int target);

void applyPhaseShift(Qureg& qureg, int target, Quest_Real angle);

void applyTwoQubitPhaseFlip(Qureg& qureg, int target1, int target2);

void applyTwoQubitPhaseShift(Qureg& qureg,
                             int target1,
                             int target2,
                             Quest_Real angle);

void applyMultiQubitPhaseFlip(Qureg& qureg, rust::Slice<const int> targets);

void applyMultiQubitPhaseShift(Qureg& qureg,
                               rust::Slice<const int> targets,
                               Quest_Real angle);

/// many-qubit CNOTs (aliases for X)
void multiplyMultiQubitNot(Qureg& qureg, rust::Slice<const int> targets);

void applyMultiQubitNot(Qureg& qureg, rust::Slice<const int> targets);

void applyControlledMultiQubitNot(Qureg& qureg,
                                  int control,
                                  rust::Slice<const int> targets);

void applyMultiControlledMultiQubitNot(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       int numControls,
                                       rust::Slice<const int> targets);

void applyMultiStateControlledMultiQubitNot(Qureg& qureg,
                                            rust::Slice<const int> controls,
                                            rust::Slice<const int> states,
                                            rust::Slice<const int> targets);

/// superoperator
void applySuperOp(Qureg& qureg,
                  rust::Slice<const int> targets,
                  const SuperOp& superop);

/// measurement
int applyQubitMeasurement(Qureg& qureg, int target);

int applyQubitMeasurementAndGetProb(Qureg& qureg,
                                    int target,
                                    Quest_Real* probability);

Quest_Real applyForcedQubitMeasurement(Qureg& qureg, int target, int outcome);

void applyQubitProjector(Qureg& qureg, int target, int outcome);

Quest_Index applyMultiQubitMeasurement(Qureg& qureg,
                                       rust::Slice<const int> qubits);

Quest_Index applyMultiQubitMeasurementAndGetProb(Qureg& qureg,
                                                 rust::Slice<const int> qubits,
                                                 Quest_Real* probability);

Quest_Real applyForcedMultiQubitMeasurement(Qureg& qureg,
                                            rust::Slice<const int> qubits,
                                            rust::Slice<const int> outcomes);

void applyMultiQubitProjector(Qureg& qureg,
                              rust::Slice<const int> qubits,
                              rust::Slice<const int> outcomes);

/// QFT
void applyQuantumFourierTransform(Qureg& qureg,
                                  rust::Slice<const int> targets,
                                  int numTargets);

void applyFullQuantumFourierTransform(Qureg& qureg);

// Pauli string creation and manipulation
std::unique_ptr<PauliStr> getPauliStr(rust::String paulis,
                                      rust::Slice<const int> indices);


std::unique_ptr<PauliStrSum> createInlinePauliStrSum(rust::String str);

std::unique_ptr<PauliStrSum> createPauliStrSumFromFile(rust::String fn);

std::unique_ptr<PauliStrSum> createPauliStrSumFromReversedFile(rust::String fn);

void destroyPauliStrSum(PauliStrSum& sum);

void reportPauliStr(PauliStr& str);

void reportPauliStrSum(PauliStrSum& str);

// Qureg
std::unique_ptr<Qureg> createQureg(int numQubits);

std::unique_ptr<Qureg> createDensityQureg(int numQubits);

std::unique_ptr<Qureg> createForcedQureg(int numQubits);

std::unique_ptr<Qureg> createForcedDensityQureg(int numQubits);

std::unique_ptr<Qureg> createCustomQureg(int numQubits,
                                         int isDensMatr,
                                         int useDistrib,
                                         int useGpuAccel,
                                         int useMultithread);

std::unique_ptr<Qureg> createCloneQureg(const Qureg& qureg);

void destroyQureg(Qureg& qureg);

void reportQuregParams(const Qureg& qureg);

void reportQureg(const Qureg& qureg);

void syncQuregToGpu(Qureg& qureg);

void syncQuregFromGpu(Qureg& qureg);

void syncSubQuregToGpu(Qureg& qureg,
                       Quest_Index localStartInd,
                       Quest_Index numLocalAmps);

void syncSubQuregFromGpu(Qureg& qureg,
                         Quest_Index localStartInd,
                         Quest_Index numLocalAmps);

rust::Vec<Quest_Complex> getQuregAmps(Qureg& qureg,
                                      Quest_Index startInd,
                                      Quest_Index numAmps);

rust::Vec<Quest_Complex> getDensityQuregAmps_flatten(Qureg& qureg,
                                                        Quest_Index startRow,
                                                        Quest_Index startCol,
                                                        Quest_Index numRows,
                                                        Quest_Index numCols);

Quest_Complex getQuregAmp(Qureg& qureg, Quest_Index index);

Quest_Complex getDensityQuregAmp(Qureg& qureg,
                                 Quest_Index row,
                                 Quest_Index column);

}  // namespace quest_sys
