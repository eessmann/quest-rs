//
// Created by Erich Essmann on 14/03/2025.
//

#include "bindings.h"
#include "helper.h"

namespace quest_sys {
// CompMatr1 operations
void multiplyCompMatr1(Qureg& qureg, int target, const CompMatr1& matr) {
  ::multiplyCompMatr1(qureg, target, matr);
}

void applyCompMatr1(Qureg& qureg, int target, const CompMatr1& matr) {
  ::applyCompMatr1(qureg, target, matr);
}

void applyControlledCompMatr1(Qureg& qureg,
                              int control,
                              int target,
                              const CompMatr1& matr) {
  ::applyControlledCompMatr1(qureg, control, target, matr);
}

void applyMultiControlledCompMatr1(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int target,
                                   const CompMatr1& matr) {
  ::applyMultiControlledCompMatr1(qureg, quest_helper::slice_to_ptr(controls),
                                  static_cast<int>(controls.length()), target,
                                  matr);
}

void applyMultiStateControlledCompMatr1(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int target,
                                        const CompMatr1& matr) {
  ::applyMultiStateControlledCompMatr1(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target, matr);
}

// CompMatr2 operations
void multiplyCompMatr2(Qureg& qureg,
                       int target1,
                       int target2,
                       const CompMatr2& matr) {
  ::multiplyCompMatr2(qureg, target1, target2, matr);
}

void applyCompMatr2(Qureg& qureg,
                    int target1,
                    int target2,
                    const CompMatr2& matr) {
  ::applyCompMatr2(qureg, target1, target2, matr);
}

void applyControlledCompMatr2(Qureg& qureg,
                              int control,
                              int target1,
                              int target2,
                              const CompMatr2& matr) {
  ::applyControlledCompMatr2(qureg, control, target1, target2, matr);
}

void applyMultiControlledCompMatr2(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int numControls,
                                   int target1,
                                   int target2,
                                   const CompMatr2& matr) {
  ::applyMultiControlledCompMatr2(qureg, quest_helper::slice_to_ptr(controls),
                                  numControls, target1, target2, matr);
}

void applyMultiStateControlledCompMatr2(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int target1,
                                        int target2,
                                        const CompMatr2& matr) {
  ::applyMultiStateControlledCompMatr2(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target1, target2, matr);
}

// CompMatr operations
void multiplyCompMatr(Qureg& qureg,
                      rust::Slice<const int> targets,
                      const CompMatr& matr) {
  ::multiplyCompMatr(qureg, quest_helper::slice_to_ptr(targets),
                     static_cast<int>(targets.length()), matr);
}

void applyCompMatr(Qureg& qureg,
                   rust::Slice<const int> targets,
                   const CompMatr& matr) {
  ::applyCompMatr(qureg, quest_helper::slice_to_ptr(targets),
                  static_cast<int>(targets.length()), matr);
}

void applyControlledCompMatr(Qureg& qureg,
                             int control,
                             rust::Slice<const int> targets,
                             const CompMatr& matr) {
  ::applyControlledCompMatr(qureg, control, quest_helper::slice_to_ptr(targets),
                            static_cast<int>(targets.length()), matr);
}

void applyMultiControlledCompMatr(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  rust::Slice<const int> targets,
                                  const CompMatr& matr) {
  ::applyMultiControlledCompMatr(qureg, quest_helper::slice_to_ptr(controls),
                                 static_cast<int>(controls.length()),
                                 quest_helper::slice_to_ptr(targets),
                                 static_cast<int>(targets.length()), matr);
}

void applyMultiStateControlledCompMatr(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       rust::Slice<const int> targets,
                                       const CompMatr& matr) {
  ::applyMultiStateControlledCompMatr(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      quest_helper::slice_to_ptr(targets), static_cast<int>(targets.length()),
      matr);
}

// S gate operations
void applyS(Qureg& qureg, int target) {
  ::applyS(qureg, target);
}

void applyControlledS(Qureg& qureg, int control, int target) {
  ::applyControlledS(qureg, control, target);
}

void applyMultiControlledS(Qureg& qureg,
                           rust::Slice<const int> controls,
                           int target) {
  ::applyMultiControlledS(qureg, quest_helper::slice_to_ptr(controls),
                          static_cast<int>(controls.length()), target);
}

void applyMultiStateControlledS(Qureg& qureg,
                                rust::Slice<const int> controls,
                                rust::Slice<const int> states,
                                int target) {
  ::applyMultiStateControlledS(qureg, quest_helper::slice_to_ptr(controls),
                               quest_helper::slice_to_ptr(states),
                               static_cast<int>(controls.length()), target);
}

// T gate operations
void applyT(Qureg& qureg, int target) {
  ::applyT(qureg, target);
}

void applyControlledT(Qureg& qureg, int control, int target) {
  ::applyControlledT(qureg, control, target);
}

void applyMultiControlledT(Qureg& qureg,
                           rust::Slice<const int> controls,
                           int target) {
  ::applyMultiControlledT(qureg, quest_helper::slice_to_ptr(controls),
                          static_cast<int>(controls.length()), target);
}

void applyMultiStateControlledT(Qureg& qureg,
                                rust::Slice<const int> controls,
                                rust::Slice<const int> states,
                                int target) {
  ::applyMultiStateControlledT(qureg, quest_helper::slice_to_ptr(controls),
                               quest_helper::slice_to_ptr(states),
                               static_cast<int>(controls.length()), target);
}

// Hadamard operations
void applyHadamard(Qureg& qureg, int target) {
  ::applyHadamard(qureg, target);
}

void applyControlledHadamard(Qureg& qureg, int control, int target) {
  ::applyControlledHadamard(qureg, control, target);
}

void applyMultiControlledHadamard(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  int target) {
  ::applyMultiControlledHadamard(qureg, quest_helper::slice_to_ptr(controls),
                                 static_cast<int>(controls.length()), target);
}

void applyMultiStateControlledHadamard(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       int target) {
  ::applyMultiStateControlledHadamard(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target);
}

// Swap operations
void multiplySwap(Qureg& qureg, int qubit1, int qubit2) {
  ::multiplySwap(qureg, qubit1, qubit2);
}

void applySwap(Qureg& qureg, int qubit1, int qubit2) {
  ::applySwap(qureg, qubit1, qubit2);
}

void applyControlledSwap(Qureg& qureg, int control, int qubit1, int qubit2) {
  ::applyControlledSwap(qureg, control, qubit1, qubit2);
}

void applyMultiControlledSwap(Qureg& qureg,
                              rust::Slice<const int> controls,
                              int qubit1,
                              int qubit2) {
  ::applyMultiControlledSwap(qureg, quest_helper::slice_to_ptr(controls),
                             static_cast<int>(controls.length()), qubit1,
                             qubit2);
}

void applyMultiStateControlledSwap(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   rust::Slice<const int> states,
                                   int qubit1,
                                   int qubit2) {
  ::applyMultiStateControlledSwap(qureg, quest_helper::slice_to_ptr(controls),
                                  quest_helper::slice_to_ptr(states),
                                  static_cast<int>(controls.length()), qubit1,
                                  qubit2);
}

// Sqrt-swap operations
void applySqrtSwap(Qureg& qureg, int qubit1, int qubit2) {
  ::applySqrtSwap(qureg, qubit1, qubit2);
}

void applyControlledSqrtSwap(Qureg& qureg,
                             int control,
                             int qubit1,
                             int qubit2) {
  ::applyControlledSqrtSwap(qureg, control, qubit1, qubit2);
}

void applyMultiControlledSqrtSwap(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  int qubit1,
                                  int qubit2) {
  ::applyMultiControlledSqrtSwap(qureg, quest_helper::slice_to_ptr(controls),
                                 static_cast<int>(controls.length()), qubit1,
                                 qubit2);
}

void applyMultiStateControlledSqrtSwap(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       int qubit1,
                                       int qubit2) {
  ::applyMultiStateControlledSqrtSwap(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      qubit1, qubit2);
}

// Individual Pauli operations
void multiplyPauliX(Qureg& qureg, int target) {
  ::multiplyPauliX(qureg, target);
}

void multiplyPauliY(Qureg& qureg, int target) {
  ::multiplyPauliY(qureg, target);
}

void multiplyPauliZ(Qureg& qureg, int target) {
  ::multiplyPauliZ(qureg, target);
}

void applyPauliX(Qureg& qureg, int target) {
  ::applyPauliX(qureg, target);
}

void applyPauliY(Qureg& qureg, int target) {
  ::applyPauliY(qureg, target);
}

void applyPauliZ(Qureg& qureg, int target) {
  ::applyPauliZ(qureg, target);
}

void applyControlledPauliX(Qureg& qureg, int control, int target) {
  ::applyControlledPauliX(qureg, control, target);
}

void applyControlledPauliY(Qureg& qureg, int control, int target) {
  ::applyControlledPauliY(qureg, control, target);
}

void applyControlledPauliZ(Qureg& qureg, int control, int target) {
  ::applyControlledPauliZ(qureg, control, target);
}

void applyMultiControlledPauliX(Qureg& qureg,
                                rust::Slice<const int> controls,
                                int target) {
  ::applyMultiControlledPauliX(qureg, quest_helper::slice_to_ptr(controls),
                               static_cast<int>(controls.length()), target);
}

void applyMultiControlledPauliY(Qureg& qureg,
                                rust::Slice<const int> controls,
                                int target) {
  ::applyMultiControlledPauliY(qureg, quest_helper::slice_to_ptr(controls),
                               static_cast<int>(controls.length()), target);
}

void applyMultiControlledPauliZ(Qureg& qureg,
                                rust::Slice<const int> controls,
                                int target) {
  ::applyMultiControlledPauliZ(qureg, quest_helper::slice_to_ptr(controls),
                               static_cast<int>(controls.length()), target);
}

void applyMultiStateControlledPauliX(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> states,
                                     int target) {
  ::applyMultiStateControlledPauliX(qureg, quest_helper::slice_to_ptr(controls),
                                    quest_helper::slice_to_ptr(states),
                                    static_cast<int>(controls.length()),
                                    target);
}
void applyMultiStateControlledPauliY(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> states,
                                     int target) {
  ::applyMultiStateControlledPauliY(qureg, quest_helper::slice_to_ptr(controls),
                                    quest_helper::slice_to_ptr(states),
                                    static_cast<int>(controls.length()),
                                    target);
}
void applyMultiStateControlledPauliZ(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> states,
                                     int target) {
  ::applyMultiStateControlledPauliZ(qureg, quest_helper::slice_to_ptr(controls),
                                    quest_helper::slice_to_ptr(states),
                                    static_cast<int>(controls.length()),
                                    target);
}

// Rotation operations
void applyRotateX(Qureg& qureg, int target, Quest_Real angle) {
  ::applyRotateX(qureg, target, angle);
}

void applyRotateY(Qureg& qureg, int target, Quest_Real angle) {
  ::applyRotateY(qureg, target, angle);
}

void applyRotateZ(Qureg& qureg, int target, Quest_Real angle) {
  ::applyRotateZ(qureg, target, angle);
}

void applyControlledRotateX(Qureg& qureg,
                            int control,
                            int target,
                            Quest_Real angle) {
  ::applyControlledRotateX(qureg, control, target, angle);
}

void applyControlledRotateY(Qureg& qureg,
                            int control,
                            int target,
                            Quest_Real angle) {
  ::applyControlledRotateY(qureg, control, target, angle);
}

void applyControlledRotateZ(Qureg& qureg,
                            int control,
                            int target,
                            Quest_Real angle) {
  ::applyControlledRotateZ(qureg, control, target, angle);
}

void applyMultiControlledRotateX(Qureg& qureg,
                                 rust::Slice<const int> controls,
                                 int target,
                                 Quest_Real angle) {
  ::applyMultiControlledRotateX(qureg, quest_helper::slice_to_ptr(controls),
                                static_cast<int>(controls.length()), target,
                                angle);
}

void applyMultiControlledRotateY(Qureg& qureg,
                                 rust::Slice<const int> controls,
                                 int target,
                                 Quest_Real angle) {
  ::applyMultiControlledRotateY(qureg, quest_helper::slice_to_ptr(controls),
                                static_cast<int>(controls.length()), target,
                                angle);
}

void applyMultiControlledRotateZ(Qureg& qureg,
                                 rust::Slice<const int> controls,
                                 int target,
                                 Quest_Real angle) {
  ::applyMultiControlledRotateZ(qureg, quest_helper::slice_to_ptr(controls),
                                static_cast<int>(controls.length()), target,
                                angle);
}

void applyMultiStateControlledRotateX(Qureg& qureg,
                                      rust::Slice<const int> controls,
                                      rust::Slice<const int> states,
                                      int target,
                                      Quest_Real angle) {
  ::applyMultiStateControlledRotateX(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target, angle);
}
void applyMultiStateControlledRotateY(Qureg& qureg,
                                      rust::Slice<const int> controls,
                                      rust::Slice<const int> states,
                                      int target,
                                      Quest_Real angle) {
  ::applyMultiStateControlledRotateY(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target, angle);
}
void applyMultiStateControlledRotateZ(Qureg& qureg,
                                      rust::Slice<const int> controls,
                                      rust::Slice<const int> states,
                                      int target,
                                      Quest_Real angle) {
  ::applyMultiStateControlledRotateZ(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target, angle);
}

// Arbitrary axis rotation
void applyRotateAroundAxis(Qureg& qureg,
                           int targ,
                           Quest_Real angle,
                           Quest_Real axisX,
                           Quest_Real axisY,
                           Quest_Real axisZ) {
  ::applyRotateAroundAxis(qureg, targ, angle, axisX, axisY, axisZ);
}

void applyControlledRotateAroundAxis(Qureg& qureg,
                                     int ctrl,
                                     int targ,
                                     Quest_Real angle,
                                     Quest_Real axisX,
                                     Quest_Real axisY,
                                     Quest_Real axisZ) {
  ::applyControlledRotateAroundAxis(qureg, ctrl, targ, angle, axisX, axisY,
                                    axisZ);
}

void applyMultiControlledRotateAroundAxis(Qureg& qureg,
                                          rust::Slice<const int> ctrls,
                                          int targ,
                                          Quest_Real angle,
                                          Quest_Real axisX,
                                          Quest_Real axisY,
                                          Quest_Real axisZ) {
  ::applyMultiControlledRotateAroundAxis(
      qureg, quest_helper::slice_to_ptr(ctrls),
      static_cast<int>(ctrls.length()), targ, angle, axisX, axisY, axisZ);
}

void applyMultiStateControlledRotateAroundAxis(Qureg& qureg,
                                               rust::Slice<const int> ctrls,
                                               rust::Slice<const int> states,
                                               int targ,
                                               Quest_Real angle,
                                               Quest_Real axisX,
                                               Quest_Real axisY,
                                               Quest_Real axisZ) {
  ::applyMultiStateControlledRotateAroundAxis(
      qureg, quest_helper::slice_to_ptr(ctrls),
      quest_helper::slice_to_ptr(states), static_cast<int>(ctrls.length()),
      targ, angle, axisX, axisY, axisZ);
}

// Phase operations
void applyPhaseFlip(Qureg& qureg, int target) {
  ::applyPhaseFlip(qureg, target);
}

void applyPhaseShift(Qureg& qureg, int target, Quest_Real angle) {
  ::applyPhaseShift(qureg, target, angle);
}

void applyTwoQubitPhaseFlip(Qureg& qureg, int target1, int target2) {
  ::applyTwoQubitPhaseFlip(qureg, target1, target2);
}

void applyTwoQubitPhaseShift(Qureg& qureg,
                             int target1,
                             int target2,
                             Quest_Real angle) {
  ::applyTwoQubitPhaseShift(qureg, target1, target2, angle);
}

void applyMultiQubitPhaseFlip(Qureg& qureg, rust::Slice<const int> targets) {
  ::applyMultiQubitPhaseFlip(qureg, quest_helper::slice_to_ptr(targets),
                             static_cast<int>(targets.length()));
}

void applyMultiQubitPhaseShift(Qureg& qureg,
                               rust::Slice<const int> targets,
                               Quest_Real angle) {
  ::applyMultiQubitPhaseShift(qureg, quest_helper::slice_to_ptr(targets),
                              static_cast<int>(targets.length()), angle);
}

/// many-qubit CNOTs (aliases for X)
void multiplyMultiQubitNot(Qureg& qureg, rust::Slice<const int> targets) {
  ::multiplyMultiQubitNot(qureg, quest_helper::slice_to_ptr(targets),
                          static_cast<int>(targets.length()));
}

void applyMultiQubitNot(Qureg& qureg, rust::Slice<const int> targets) {
  ::applyMultiQubitNot(qureg, quest_helper::slice_to_ptr(targets),
                       static_cast<int>(targets.length()));
}

void applyControlledMultiQubitNot(Qureg& qureg,
                                  int control,
                                  rust::Slice<const int> targets) {
  ::applyControlledMultiQubitNot(qureg, control,
                                 quest_helper::slice_to_ptr(targets),
                                 static_cast<int>(targets.length()));
}

void applyMultiControlledMultiQubitNot(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       int numControls,
                                       rust::Slice<const int> targets) {
  ::applyMultiControlledMultiQubitNot(
      qureg, quest_helper::slice_to_ptr(controls), numControls,
      quest_helper::slice_to_ptr(targets), static_cast<int>(targets.length()));
}

void applyMultiStateControlledMultiQubitNot(Qureg& qureg,
                                            rust::Slice<const int> controls,
                                            rust::Slice<const int> states,
                                            rust::Slice<const int> targets) {
  ::applyMultiStateControlledMultiQubitNot(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      quest_helper::slice_to_ptr(targets), static_cast<int>(targets.length()));
}

// superoperator
void applySuperOp(Qureg& qureg,
                  rust::Slice<const int> targets,
                  const SuperOp& superop) {
  ::applySuperOp(qureg, quest_helper::slice_to_ptr(targets),
                 static_cast<int>(targets.length()), superop);
}

// Measurement operations
int applyQubitMeasurement(Qureg& qureg, int target) {
  return ::applyQubitMeasurement(qureg, target);
}

int applyQubitMeasurementAndGetProb(Qureg& qureg,
                                    int target,
                                    Quest_Real* probability) {
  return ::applyQubitMeasurementAndGetProb(qureg, target, probability);
}

Quest_Real applyForcedQubitMeasurement(Qureg& qureg, int target, int outcome) {
  return ::applyForcedQubitMeasurement(qureg, target, outcome);
}

void applyQubitProjector(Qureg& qureg, int target, int outcome) {
  ::applyQubitProjector(qureg, target, outcome);
}

Quest_Index applyMultiQubitMeasurement(Qureg& qureg,
                                       rust::Slice<const int> qubits) {
  return ::applyMultiQubitMeasurement(qureg, quest_helper::slice_to_ptr(qubits),
                                      static_cast<int>(qubits.length()));
}

Quest_Index applyMultiQubitMeasurementAndGetProb(Qureg& qureg,
                                                 rust::Slice<const int> qubits,
                                                 Quest_Real* probability) {
  return ::applyMultiQubitMeasurementAndGetProb(
      qureg, quest_helper::slice_to_ptr(qubits),
      static_cast<int>(qubits.length()), probability);
}

Quest_Real applyForcedMultiQubitMeasurement(Qureg& qureg,
                                            rust::Slice<const int> qubits,
                                            rust::Slice<const int> outcomes) {
  return ::applyForcedMultiQubitMeasurement(
      qureg, quest_helper::slice_to_ptr(qubits),
      quest_helper::slice_to_ptr(outcomes), static_cast<int>(qubits.length()));
}

void applyMultiQubitProjector(Qureg& qureg,
                              rust::Slice<const int> qubits,
                              rust::Slice<const int> outcomes) {
  ::applyMultiQubitProjector(qureg, quest_helper::slice_to_ptr(qubits),
                             quest_helper::slice_to_ptr(outcomes),
                             static_cast<int>(qubits.length()));
}

// QFT operations
void applyQuantumFourierTransform(Qureg& qureg,
                                  rust::Slice<const int> targets,
                                  int numTargets) {
  ::applyQuantumFourierTransform(qureg, quest_helper::slice_to_ptr(targets),
                                 numTargets);
}

void applyFullQuantumFourierTransform(Qureg& qureg) {
  ::applyFullQuantumFourierTransform(qureg);
}

// Pauli string operations
void multiplyPauliStr(Qureg& qureg, const PauliStr& str) {
  ::multiplyPauliStr(qureg, str);
}

void applyPauliStr(Qureg& qureg, const PauliStr& str) {
  ::applyPauliStr(qureg, str);
}

void applyControlledPauliStr(Qureg& qureg, int control, const PauliStr& str) {
  ::applyControlledPauliStr(qureg, control, str);
}

void applyMultiControlledPauliStr(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  int numControls,
                                  const PauliStr& str) {
  ::applyMultiControlledPauliStr(qureg, quest_helper::slice_to_ptr(controls),
                                 numControls, str);
}

void applyMultiStateControlledPauliStr(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       const PauliStr& str) {
  ::applyMultiStateControlledPauliStr(qureg,
                                      quest_helper::slice_to_ptr(controls),
                                      quest_helper::slice_to_ptr(states),
                                      static_cast<int>(controls.length()), str);
}

// Pauli gadget operations
void multiplyPauliGadget(Qureg& qureg, const PauliStr& str, Quest_Real angle) {
  ::multiplyPauliGadget(qureg, str, angle);
}

void applyPauliGadget(Qureg& qureg, const PauliStr& str, Quest_Real angle) {
  ::applyPauliGadget(qureg, str, angle);
}

void applyControlledPauliGadget(Qureg& qureg,
                                int control,
                                const PauliStr& str,
                                Quest_Real angle) {
  ::applyControlledPauliGadget(qureg, control, str, angle);
}

void applyMultiControlledPauliGadget(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     const PauliStr& str,
                                     Quest_Real angle) {
  ::applyMultiControlledPauliGadget(qureg, quest_helper::slice_to_ptr(controls),
                                    static_cast<int>(controls.length()), str,
                                    angle);
}

void applyMultiStateControlledPauliGadget(Qureg& qureg,
                                          rust::Slice<const int> controls,
                                          rust::Slice<const int> states,
                                          const PauliStr& str,
                                          Quest_Real angle) {
  ::applyMultiStateControlledPauliGadget(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      str, angle);
}

// Phase gadget operations
void multiplyPhaseGadget(Qureg& qureg,
                         rust::Slice<const int> targets,
                         Quest_Real angle) {
  ::multiplyPhaseGadget(qureg, quest_helper::slice_to_ptr(targets),
                        static_cast<int>(targets.length()), angle);
}

void applyPhaseGadget(Qureg& qureg,
                      rust::Slice<const int> targets,
                      Quest_Real angle) {
  ::applyPhaseGadget(qureg, quest_helper::slice_to_ptr(targets),
                     static_cast<int>(targets.length()), angle);
}

void applyControlledPhaseGadget(Qureg& qureg,
                                int control,
                                rust::Slice<const int> targets,
                                Quest_Real angle) {
  ::applyControlledPhaseGadget(qureg, control,
                               quest_helper::slice_to_ptr(targets),
                               static_cast<int>(targets.length()), angle);
}

void applyMultiControlledPhaseGadget(Qureg& qureg,
                                     rust::Slice<const int> controls,
                                     rust::Slice<const int> targets,
                                     Quest_Real angle) {
  ::applyMultiControlledPhaseGadget(qureg, quest_helper::slice_to_ptr(controls),
                                    static_cast<int>(controls.length()),
                                    quest_helper::slice_to_ptr(targets),
                                    static_cast<int>(targets.length()), angle);
}

void applyMultiStateControlledPhaseGadget(Qureg& qureg,
                                          rust::Slice<const int> controls,
                                          rust::Slice<const int> states,
                                          rust::Slice<const int> targets,
                                          Quest_Real angle) {
  ::applyMultiStateControlledPhaseGadget(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      quest_helper::slice_to_ptr(targets), static_cast<int>(targets.length()),
      angle);
}

// Pauli sum operations
void multiplyPauliStrSum(Qureg& qureg, const PauliStrSum& sum, Qureg& workspace) {
  ::multiplyPauliStrSum(qureg, sum, workspace);
}

void applyTrotterizedPauliStrSumGadget(Qureg& qureg,
                                       const PauliStrSum& sum,
                                       Quest_Real angle,
                                       int order,
                                       int reps) {
  ::applyTrotterizedPauliStrSumGadget(qureg, sum, angle, order, reps);
}

// Additional Pauli functions for Rust API convenience
std::unique_ptr<PauliStr> getPauliStr(rust::String paulis,
                                      rust::Slice<const int> indices) {
  return std::make_unique<PauliStr>(
      ::getPauliStr(paulis.c_str(), quest_helper::slice_to_ptr(indices),
                    static_cast<int>(indices.length())));
}

std::unique_ptr<PauliStrSum> createInlinePauliStrSum(rust::String str) {
  return std::make_unique<PauliStrSum>(::createInlinePauliStrSum(str.c_str()));
}

std::unique_ptr<PauliStrSum> createPauliStrSumFromFile(rust::String fn) {
  return std::make_unique<PauliStrSum>(::createPauliStrSumFromFile(fn.c_str()));
}

std::unique_ptr<PauliStrSum> createPauliStrSumFromReversedFile(
    rust::String fn) {
  return std::make_unique<PauliStrSum>(
      ::createPauliStrSumFromReversedFile(fn.c_str()));
}

void destroyPauliStrSum(PauliStrSum& sum) {
  ::destroyPauliStrSum(sum);
}

void reportPauliStr(PauliStr& str) {
  ::reportPauliStr(str);
}

void reportPauliStrSum(PauliStrSum& str) {
  ::reportPauliStrSum(str);
}

/// DiagMatr1
void multiplyDiagMatr1(Qureg& qureg, int target, const DiagMatr1& matr) {
  ::multiplyDiagMatr1(qureg, target, matr);
}

void applyDiagMatr1(Qureg& qureg, int target, const DiagMatr1& matr) {
  ::applyDiagMatr1(qureg, target, matr);
}

void applyControlledDiagMatr1(Qureg& qureg,
                              int control,
                              int target,
                              const DiagMatr1& matr) {
  ::applyControlledDiagMatr1(qureg, control, target, matr);
}

void applyMultiControlledDiagMatr1(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int target,
                                   const DiagMatr1& matr) {
  ::applyMultiControlledDiagMatr1(qureg, quest_helper::slice_to_ptr(controls),
                                  static_cast<int>(controls.length()), target,
                                  matr);
}

void applyMultiStateControlledDiagMatr1(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int target,
                                        const DiagMatr1& matr) {
  ::applyMultiStateControlledDiagMatr1(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target, matr);
}

/// DiagMatr2
void multiplyDiagMatr2(Qureg& qureg,
                       int target1,
                       int target2,
                       const DiagMatr2& matr) {
  ::multiplyDiagMatr2(qureg, target1, target2, matr);
}

void applyDiagMatr2(Qureg& qureg,
                    int target1,
                    int target2,
                    const DiagMatr2& matr) {
  ::applyDiagMatr2(qureg, target1, target2, matr);
}

void applyControlledDiagMatr2(Qureg& qureg,
                              int control,
                              int target1,
                              int target2,
                              const DiagMatr2& matr) {
  ::applyControlledDiagMatr2(qureg, control, target1, target2, matr);
}

void applyMultiControlledDiagMatr2(Qureg& qureg,
                                   rust::Slice<const int> controls,
                                   int target1,
                                   int target2,
                                   const DiagMatr2& matr) {
  ::applyMultiControlledDiagMatr2(qureg, quest_helper::slice_to_ptr(controls),
                                  static_cast<int>(controls.length()), target1,
                                  target2, matr);
}

void applyMultiStateControlledDiagMatr2(Qureg& qureg,
                                        rust::Slice<const int> controls,
                                        rust::Slice<const int> states,
                                        int target1,
                                        int target2,
                                        const DiagMatr2& matr) {
  ::applyMultiStateControlledDiagMatr2(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      target1, target2, matr);
}

/// DiagMatr
void multiplyDiagMatr(Qureg& qureg,
                      rust::Slice<const int> targets,
                      const DiagMatr& matrix) {
  ::multiplyDiagMatr(qureg, quest_helper::slice_to_ptr(targets),
                     static_cast<int>(targets.length()), matrix);
}

void applyDiagMatr(Qureg& qureg,
                   rust::Slice<const int> targets,
                   const DiagMatr& matrix) {
  ::applyDiagMatr(qureg, quest_helper::slice_to_ptr(targets),
                  static_cast<int>(targets.length()), matrix);
}

void applyControlledDiagMatr(Qureg& qureg,
                             int control,
                             rust::Slice<const int> targets,
                             const DiagMatr& matrix) {
  ::applyControlledDiagMatr(qureg, control, quest_helper::slice_to_ptr(targets),
                            static_cast<int>(targets.length()), matrix);
}

void applyMultiControlledDiagMatr(Qureg& qureg,
                                  rust::Slice<const int> controls,
                                  rust::Slice<const int> targets,
                                  const DiagMatr& matrix) {
  ::applyMultiControlledDiagMatr(qureg, quest_helper::slice_to_ptr(controls),
                                 static_cast<int>(controls.length()),
                                 quest_helper::slice_to_ptr(targets),
                                 static_cast<int>(targets.length()), matrix);
}

void applyMultiStateControlledDiagMatr(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> states,
                                       rust::Slice<const int> targets,
                                       const DiagMatr& matrix) {
  ::applyMultiStateControlledDiagMatr(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      quest_helper::slice_to_ptr(targets), static_cast<int>(targets.length()),
      matrix);
}

/// DiagMatrPower
void multiplyDiagMatrPower(Qureg& qureg,
                           rust::Slice<const int> targets,
                           const DiagMatr& matrix,
                           Quest_Complex exponent) {
  ::multiplyDiagMatrPower(qureg, quest_helper::slice_to_ptr(targets),
                          static_cast<int>(targets.length()), matrix, exponent);
}

void applyDiagMatrPower(Qureg& qureg,
                        rust::Slice<const int> targets,
                        const DiagMatr& matrix,
                        Quest_Complex exponent) {
  ::applyDiagMatrPower(qureg, quest_helper::slice_to_ptr(targets),
                       static_cast<int>(targets.length()), matrix, exponent);
}

void applyControlledDiagMatrPower(Qureg& qureg,
                                  int control,
                                  rust::Slice<const int> targets,
                                  const DiagMatr& matrix,
                                  Quest_Complex exponent) {
  ::applyControlledDiagMatrPower(
      qureg, control, quest_helper::slice_to_ptr(targets),
      static_cast<int>(targets.length()), matrix, exponent);
}

void applyMultiControlledDiagMatrPower(Qureg& qureg,
                                       rust::Slice<const int> controls,
                                       rust::Slice<const int> targets,
                                       const DiagMatr& matrix,
                                       Quest_Complex exponent) {
  ::applyMultiControlledDiagMatrPower(
      qureg, quest_helper::slice_to_ptr(controls),
      static_cast<int>(controls.length()), quest_helper::slice_to_ptr(targets),
      static_cast<int>(targets.length()), matrix, exponent);
}

void applyMultiStateControlledDiagMatrPower(Qureg& qureg,
                                            rust::Slice<const int> controls,
                                            rust::Slice<const int> states,
                                            rust::Slice<const int> targets,
                                            const DiagMatr& matrix,
                                            Quest_Complex exponent) {
  ::applyMultiStateControlledDiagMatrPower(
      qureg, quest_helper::slice_to_ptr(controls),
      quest_helper::slice_to_ptr(states), static_cast<int>(controls.length()),
      quest_helper::slice_to_ptr(targets), static_cast<int>(targets.length()),
      matrix, exponent);
}

/// FullStateDiagMatr
void multiplyFullStateDiagMatr(Qureg& qureg, const FullStateDiagMatr& matrix) {
  ::multiplyFullStateDiagMatr(qureg, matrix);
}

void multiplyFullStateDiagMatrPower(Qureg& qureg,
                                    const FullStateDiagMatr& matrix,
                                    Quest_Complex exponent) {
  ::multiplyFullStateDiagMatrPower(qureg, matrix, exponent);
}

void applyFullStateDiagMatr(Qureg& qureg, const FullStateDiagMatr& matrix) {
  ::applyFullStateDiagMatr(qureg, matrix);
}

void applyFullStateDiagMatrPower(Qureg& qureg,
                                 const FullStateDiagMatr& matrix,
                                 Quest_Complex exponent) {
  ::applyFullStateDiagMatrPower(qureg, matrix, exponent);
}

}  // namespace quest_sys
