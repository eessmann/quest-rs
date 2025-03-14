#include "bindings.h"

namespace quest_sys {
// Calculations
qreal calcExpecPauliStr(const Qureg& qureg, const PauliStr& str) {
  return ::calcExpecPauliStr(qureg, str);
}

qreal calcExpecPauliStrSum(const Qureg& qureg, const PauliStrSum& sum) {
  return ::calcExpecPauliStrSum(qureg, sum);
}

qreal calcExpecFullStateDiagMatr(const Qureg& qureg,
                                 const FullStateDiagMatr& matr) {
  return ::calcExpecFullStateDiagMatr(qureg, matr);
}

qreal calcExpecFullStateDiagMatrPower(const Qureg& qureg,
                                      const FullStateDiagMatr& matr,
                                      Quest_Complex exponent) {
  return ::calcExpecFullStateDiagMatrPower(qureg, matr, exponent);
}

qreal calcTotalProb(const Qureg& qureg) {
  return ::calcTotalProb(qureg);
}

qreal calcProbOfBasisState(const Qureg& qureg, qindex index) {
  return ::calcProbOfBasisState(qureg, index);
}

qreal calcProbOfQubitOutcome(const Qureg& qureg, int qubit, int outcome) {
  return ::calcProbOfQubitOutcome(qureg, qubit, outcome);
}

qreal calcProbOfMultiQubitOutcome(const Qureg& qureg,
                                  rust::Slice<int> qubits,
                                  rust::Slice<int> outcomes,
                                  int numQubits) {
  return ::calcProbOfMultiQubitOutcome(qureg, qubits.data(), outcomes.data(),
                                       numQubits);
}

void calcProbsOfAllMultiQubitOutcomes(rust::Slice<qreal> outcomeProbs,
                                      const Qureg& qureg,
                                      rust::Slice<int> qubits,
                                      int numQubits) {
  return ::calcProbsOfAllMultiQubitOutcomes(outcomeProbs.data(), qureg,
                                            qubits.data(), numQubits);
}

qreal calcPurity(const Qureg& qureg) {
  return ::calcPurity(qureg);
}

qreal calcFidelity(const Qureg& qureg, const Qureg& other) {
  return ::calcFidelity(qureg, other);
}

qreal calcDistance(const Qureg& qureg1, const Qureg& qureg2) {
  return ::calcDistance(qureg1, qureg2);
}

std::unique_ptr<Qureg> calcPartialTrace(const Qureg& qureg,
                                        rust::Slice<int> traceOutQubits,
                                        int numTraceQubits) {
  return std::make_unique<Qureg>(
      ::calcPartialTrace(qureg, traceOutQubits.data(), numTraceQubits));
}

std::unique_ptr<Qureg> calcReducedDensityMatrix(const Qureg& qureg,
                                                rust::Slice<int> retainQubits,
                                                int numRetainQubits) {
  return std::make_unique<Qureg>(
      ::calcReducedDensityMatrix(qureg, retainQubits.data(), numRetainQubits));
}

void setQuregToPartialTrace(Qureg& out,
                            const Qureg& in,
                            rust::Slice<int> traceOutQubits,
                            int numTraceQubits) {
  ::setQuregToPartialTrace(out, in, traceOutQubits.data(), numTraceQubits);
}

void setQuregToReducedDensityMatrix(Qureg& out,
                                    const Qureg& in,
                                    rust::Slice<int> retainQubits,
                                    int numRetainQubits) {
  ::setQuregToReducedDensityMatrix(out, in, retainQubits.data(),
                                   numRetainQubits);
}

Quest_Complex calcInnerProduct(const Qureg& qureg1, const Qureg& qureg2) {
  return ::calcInnerProduct(qureg1, qureg2);
}

Quest_Complex calcExpecNonHermitianPauliStrSum(const Qureg& qureg,
                                               const PauliStrSum& sum) {
  return ::calcExpecNonHermitianPauliStrSum(qureg, sum);
}

Quest_Complex calcExpecNonHermitianFullStateDiagMatr(
    const Qureg& qureg,
    const FullStateDiagMatr& matr) {
  return ::calcExpecNonHermitianFullStateDiagMatr(qureg, matr);
}

Quest_Complex calcExpecNonHermitianFullStateDiagMatrPower(
    const Qureg& qureg,
    const FullStateDiagMatr& matrix,
    Quest_Complex exponent) {
  return ::calcExpecNonHermitianFullStateDiagMatrPower(qureg, matrix, exponent);
}
}  // namespace quest_sys
