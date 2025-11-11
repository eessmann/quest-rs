//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <memory>

#include "types.hpp"
#include <quest-sys/src/lib.rs.h>

namespace quest_sys {

/// CompMatr1
void multiplyCompMatr1(Qureg& qureg, int target, const CompMatr1& matr);

void applyCompMatr1(Qureg& qureg, int target, const CompMatr1& matr);

void applyControlledCompMatr1(Qureg& qureg,
                              int control,
                              int target,
                              const CompMatr1& matr);

void applyMultiControlledCompMatr1(Qureg& qureg,
                                   rust::cxxbridge1::Slice<const int> controls,
                                   int target,
                                   const CompMatr1& matr);

void applyMultiStateControlledCompMatr1(Qureg& qureg,
                                        rust::cxxbridge1::Slice<const int> controls,
                                        rust::cxxbridge1::Slice<const int> states,
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
                                   rust::cxxbridge1::Slice<const int> controls,
                                   int numControls,
                                   int target1,
                                   int target2,
                                   const CompMatr2& matr);

void applyMultiStateControlledCompMatr2(Qureg& qureg,
                                        rust::cxxbridge1::Slice<const int> controls,
                                        rust::cxxbridge1::Slice<const int> states,
                                        int target1,
                                        int target2,
                                        const CompMatr2& matr);

/// CompMatr
void multiplyCompMatr(Qureg& qureg,
                      rust::cxxbridge1::Slice<const int> targets,
                      const CompMatr& matr);

void applyCompMatr(Qureg& qureg,
                   rust::cxxbridge1::Slice<const int> targets,
                   const CompMatr& matr);

void applyControlledCompMatr(Qureg& qureg,
                             int control,
                             rust::cxxbridge1::Slice<const int> targets,
                             const CompMatr& matr);

void applyMultiControlledCompMatr(Qureg& qureg,
                                  rust::cxxbridge1::Slice<const int> controls,
                                  rust::cxxbridge1::Slice<const int> targets,
                                  const CompMatr& matr);

void applyMultiStateControlledCompMatr(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> controls,
                                       rust::cxxbridge1::Slice<const int> states,
                                       rust::cxxbridge1::Slice<const int> targets,
                                       const CompMatr& matr);

/// DiagMatr1
void multiplyDiagMatr1(Qureg& qureg, int target, const DiagMatr1& matr);

void applyDiagMatr1(Qureg& qureg, int target, const DiagMatr1& matr);

void applyControlledDiagMatr1(Qureg& qureg,
                              int control,
                              int target,
                              const DiagMatr1& matr);

void applyMultiControlledDiagMatr1(Qureg& qureg,
                                   rust::cxxbridge1::Slice<const int> controls,
                                   int target,
                                   const DiagMatr1& matr);

void applyMultiStateControlledDiagMatr1(Qureg& qureg,
                                        rust::cxxbridge1::Slice<const int> controls,
                                        rust::cxxbridge1::Slice<const int> states,
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
                                   rust::cxxbridge1::Slice<const int> controls,
                                   int target1,
                                   int target2,
                                   const DiagMatr2& matr);

void applyMultiStateControlledDiagMatr2(Qureg& qureg,
                                        rust::cxxbridge1::Slice<const int> controls,
                                        rust::cxxbridge1::Slice<const int> states,
                                        int target1,
                                        int target2,
                                        const DiagMatr2& matr);

/// DiagMatr
void multiplyDiagMatr(Qureg& qureg,
                      rust::cxxbridge1::Slice<const int> targets,
                      const DiagMatr& matrix);

void applyDiagMatr(Qureg& qureg,
                   rust::cxxbridge1::Slice<const int> targets,
                   const DiagMatr& matrix);

void applyControlledDiagMatr(Qureg& qureg,
                             int control,
                             rust::cxxbridge1::Slice<const int> targets,
                             const DiagMatr& matrix);

void applyMultiControlledDiagMatr(Qureg& qureg,
                                  rust::cxxbridge1::Slice<const int> controls,
                                  rust::cxxbridge1::Slice<const int> targets,
                                  const DiagMatr& matrix);

void applyMultiStateControlledDiagMatr(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> controls,
                                       rust::cxxbridge1::Slice<const int> states,
                                       rust::cxxbridge1::Slice<const int> targets,
                                       const DiagMatr& matrix);

/// DiagMatrPower
void multiplyDiagMatrPower(Qureg& qureg,
                           rust::cxxbridge1::Slice<const int> targets,
                           const DiagMatr& matrix,
                           Quest_Complex exponent);

void applyDiagMatrPower(Qureg& qureg,
                        rust::cxxbridge1::Slice<const int> targets,
                        const DiagMatr& matrix,
                        Quest_Complex exponent);

void applyControlledDiagMatrPower(Qureg& qureg,
                                  int control,
                                  rust::cxxbridge1::Slice<const int> targets,
                                  const DiagMatr& matrix,
                                  Quest_Complex exponent);

void applyMultiControlledDiagMatrPower(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> controls,
                                       rust::cxxbridge1::Slice<const int> targets,
                                       const DiagMatr& matrix,
                                       Quest_Complex exponent);

void applyMultiStateControlledDiagMatrPower(Qureg& qureg,
                                            rust::cxxbridge1::Slice<const int> controls,
                                            rust::cxxbridge1::Slice<const int> states,
                                            rust::cxxbridge1::Slice<const int> targets,
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
                           rust::cxxbridge1::Slice<const int> controls,
                           int target);

void applyMultiStateControlledS(Qureg& qureg,
                                rust::cxxbridge1::Slice<const int> controls,
                                rust::cxxbridge1::Slice<const int> states,
                                int target);

/// T gate
void applyT(Qureg& qureg, int target);

void applyControlledT(Qureg& qureg, int control, int target);

void applyMultiControlledT(Qureg& qureg,
                           rust::cxxbridge1::Slice<const int> controls,
                           int target);

void applyMultiStateControlledT(Qureg& qureg,
                                rust::cxxbridge1::Slice<const int> controls,
                                rust::cxxbridge1::Slice<const int> states,
                                int target);

/// Hadamard
void applyHadamard(Qureg& qureg, int target);

void applyControlledHadamard(Qureg& qureg, int control, int target);

void applyMultiControlledHadamard(Qureg& qureg,
                                  rust::cxxbridge1::Slice<const int> controls,
                                  int target);

void applyMultiStateControlledHadamard(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> controls,
                                       rust::cxxbridge1::Slice<const int> states,
                                       int target);

/// swaps
void multiplySwap(Qureg& qureg, int qubit1, int qubit2);

void applySwap(Qureg& qureg, int qubit1, int qubit2);

void applyControlledSwap(Qureg& qureg, int control, int qubit1, int qubit2);

void applyMultiControlledSwap(Qureg& qureg,
                              rust::cxxbridge1::Slice<const int> controls,
                              int qubit1,
                              int qubit2);

void applyMultiStateControlledSwap(Qureg& qureg,
                                   rust::cxxbridge1::Slice<const int> controls,
                                   rust::cxxbridge1::Slice<const int> states,
                                   int qubit1,
                                   int qubit2);

/// sqrt-swap
void applySqrtSwap(Qureg& qureg, int qubit1, int qubit2);

void applyControlledSqrtSwap(Qureg& qureg, int control, int qubit1, int qubit2);

void applyMultiControlledSqrtSwap(Qureg& qureg,
                                  rust::cxxbridge1::Slice<const int> controls,
                                  int qubit1,
                                  int qubit2);

void applyMultiStateControlledSqrtSwap(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> controls,
                                       rust::cxxbridge1::Slice<const int> states,
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
                                rust::cxxbridge1::Slice<const int> controls,
                                int target);
void applyMultiControlledPauliY(Qureg& qureg,
                                rust::cxxbridge1::Slice<const int> controls,
                                int target);
void applyMultiControlledPauliZ(Qureg& qureg,
                                rust::cxxbridge1::Slice<const int> controls,
                                int target);

void applyMultiStateControlledPauliX(Qureg& qureg,
                                     rust::cxxbridge1::Slice<const int> controls,
                                     rust::cxxbridge1::Slice<const int> states,
                                     int target);
void applyMultiStateControlledPauliY(Qureg& qureg,
                                     rust::cxxbridge1::Slice<const int> controls,
                                     rust::cxxbridge1::Slice<const int> states,
                                     int target);
void applyMultiStateControlledPauliZ(Qureg& qureg,
                                     rust::cxxbridge1::Slice<const int> controls,
                                     rust::cxxbridge1::Slice<const int> states,
                                     int target);

/// Pauli strings
void multiplyPauliStr(Qureg& qureg, const PauliStr& str);

void applyPauliStr(Qureg& qureg, const PauliStr& str);

void applyControlledPauliStr(Qureg& qureg, int control, const PauliStr& str);

void applyMultiControlledPauliStr(Qureg& qureg,
                                  rust::cxxbridge1::Slice<const int> controls,
                                  int numControls,
                                  const PauliStr& str);

void applyMultiStateControlledPauliStr(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> controls,
                                       rust::cxxbridge1::Slice<const int> states,
                                       const PauliStr& str);

/// Pauli string sums
void multiplyPauliStrSum(Qureg& qureg,
                         const PauliStrSum& sum,
                         Qureg& workspace);

void applyTrotterizedPauliStrSumGadget(Qureg& qureg,
                                       const PauliStrSum& sum,
                                       double angle,
                                       int order,
                                       int reps);

/// individual axis rotations
void applyRotateX(Qureg& qureg, int target, double angle);
void applyRotateY(Qureg& qureg, int target, double angle);
void applyRotateZ(Qureg& qureg, int target, double angle);

void applyControlledRotateX(Qureg& qureg,
                            int control,
                            int target,
                            double angle);
void applyControlledRotateY(Qureg& qureg,
                            int control,
                            int target,
                            double angle);
void applyControlledRotateZ(Qureg& qureg,
                            int control,
                            int target,
                            double angle);

void applyMultiControlledRotateX(Qureg& qureg,
                                 rust::cxxbridge1::Slice<const int> controls,
                                 int target,
                                 double angle);
void applyMultiControlledRotateY(Qureg& qureg,
                                 rust::cxxbridge1::Slice<const int> controls,
                                 int target,
                                 double angle);
void applyMultiControlledRotateZ(Qureg& qureg,
                                 rust::cxxbridge1::Slice<const int> controls,
                                 int target,
                                 double angle);

void applyMultiStateControlledRotateX(Qureg& qureg,
                                      rust::cxxbridge1::Slice<const int> controls,
                                      rust::cxxbridge1::Slice<const int> states,
                                      int target,
                                      double angle);
void applyMultiStateControlledRotateY(Qureg& qureg,
                                      rust::cxxbridge1::Slice<const int> controls,
                                      rust::cxxbridge1::Slice<const int> states,
                                      int target,
                                      double angle);
void applyMultiStateControlledRotateZ(Qureg& qureg,
                                      rust::cxxbridge1::Slice<const int> controls,
                                      rust::cxxbridge1::Slice<const int> states,
                                      int target,
                                      double angle);

/// arbitrary axis rotation
void applyRotateAroundAxis(Qureg& qureg,
                           int targ,
                           double angle,
                           double axisX,
                           double axisY,
                           double axisZ);

void applyControlledRotateAroundAxis(Qureg& qureg,
                                     int ctrl,
                                     int targ,
                                     double angle,
                                     double axisX,
                                     double axisY,
                                     double axisZ);

void applyMultiControlledRotateAroundAxis(Qureg& qureg,
                                          rust::cxxbridge1::Slice<const int> ctrls,
                                          int targ,
                                          double angle,
                                          double axisX,
                                          double axisY,
                                          double axisZ);

void applyMultiStateControlledRotateAroundAxis(Qureg& qureg,
                                               rust::cxxbridge1::Slice<const int> ctrls,
                                               rust::cxxbridge1::Slice<const int> states,
                                               int targ,
                                               double angle,
                                               double axisX,
                                               double axisY,
                                               double axisZ);

/// Pauli gadgets
void multiplyPauliGadget(Qureg& qureg, const PauliStr& str, double angle);

void applyPauliGadget(Qureg& qureg, const PauliStr& str, double angle);

void applyControlledPauliGadget(Qureg& qureg,
                                int control,
                                const PauliStr& str,
                                double angle);

void applyMultiControlledPauliGadget(Qureg& qureg,
                                     rust::cxxbridge1::Slice<const int> controls,
                                     const PauliStr& str,
                                     double angle);

void applyMultiStateControlledPauliGadget(Qureg& qureg,
                                          rust::cxxbridge1::Slice<const int> controls,
                                          rust::cxxbridge1::Slice<const int> states,
                                          const PauliStr& str,
                                          double angle);

/// phase gadgets
void multiplyPhaseGadget(Qureg& qureg,
                         rust::cxxbridge1::Slice<const int> targets,
                         double angle);

void applyPhaseGadget(Qureg& qureg,
                      rust::cxxbridge1::Slice<const int> targets,
                      double angle);

void applyControlledPhaseGadget(Qureg& qureg,
                                int control,
                                rust::cxxbridge1::Slice<const int> targets,
                                double angle);

void applyMultiControlledPhaseGadget(Qureg& qureg,
                                     rust::cxxbridge1::Slice<const int> controls,
                                     rust::cxxbridge1::Slice<const int> targets,
                                     double angle);

void applyMultiStateControlledPhaseGadget(Qureg& qureg,
                                          rust::cxxbridge1::Slice<const int> controls,
                                          rust::cxxbridge1::Slice<const int> states,
                                          rust::cxxbridge1::Slice<const int> targets,
                                          double angle);

/// phase shifts and flips
void applyPhaseFlip(Qureg& qureg, int target);

void applyPhaseShift(Qureg& qureg, int target, double angle);

void applyTwoQubitPhaseFlip(Qureg& qureg, int target1, int target2);

void applyTwoQubitPhaseShift(Qureg& qureg,
                             int target1,
                             int target2,
                             double angle);

void applyMultiQubitPhaseFlip(Qureg& qureg, rust::cxxbridge1::Slice<const int> targets);

void applyMultiQubitPhaseShift(Qureg& qureg,
                               rust::cxxbridge1::Slice<const int> targets,
                               double angle);

/// many-qubit CNOTs (aliases for X)
void multiplyMultiQubitNot(Qureg& qureg, rust::cxxbridge1::Slice<const int> targets);

void applyMultiQubitNot(Qureg& qureg, rust::cxxbridge1::Slice<const int> targets);

void applyControlledMultiQubitNot(Qureg& qureg,
                                  int control,
                                  rust::cxxbridge1::Slice<const int> targets);

void applyMultiControlledMultiQubitNot(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> controls,
                                       int numControls,
                                       rust::cxxbridge1::Slice<const int> targets);

void applyMultiStateControlledMultiQubitNot(Qureg& qureg,
                                            rust::cxxbridge1::Slice<const int> controls,
                                            rust::cxxbridge1::Slice<const int> states,
                                            rust::cxxbridge1::Slice<const int> targets);

/// measurement
int applyQubitMeasurement(Qureg& qureg, int target);

int applyQubitMeasurementAndGetProb(Qureg& qureg,
                                    int target,
                                    double* probability);

double applyForcedQubitMeasurement(Qureg& qureg, int target, int outcome);

void applyQubitProjector(Qureg& qureg, int target, int outcome);

std::int64_t applyMultiQubitMeasurement(Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> qubits);

std::int64_t applyMultiQubitMeasurementAndGetProb(Qureg& qureg,
                                                 rust::cxxbridge1::Slice<const int> qubits,
                                                 double* probability);

double applyForcedMultiQubitMeasurement(Qureg& qureg,
                                            rust::cxxbridge1::Slice<const int> qubits,
                                            rust::cxxbridge1::Slice<const int> outcomes);

void applyMultiQubitProjector(Qureg& qureg,
                              rust::cxxbridge1::Slice<const int> qubits,
                              rust::cxxbridge1::Slice<const int> outcomes);

/// QFT
void applyQuantumFourierTransform(Qureg& qureg,
                                  rust::cxxbridge1::Slice<const int> targets,
                                  int numTargets);

void applyFullQuantumFourierTransform(Qureg& qureg);

// Pauli string creation and manipulation
std::unique_ptr<PauliStr> getPauliStr(rust::cxxbridge1::String paulis,
                                      rust::cxxbridge1::Slice<const int> indices);

std::unique_ptr<PauliStrSum> createInlinePauliStrSum(rust::cxxbridge1::String str);

std::unique_ptr<PauliStrSum> createPauliStrSumFromFile(rust::cxxbridge1::String fn);

std::unique_ptr<PauliStrSum> createPauliStrSumFromReversedFile(rust::cxxbridge1::String fn);

void destroyPauliStrSum(PauliStrSum& sum);

void reportPauliStr(PauliStr& str);

void reportPauliStrSum(PauliStrSum& str);

}  // namespace quest_sys
