#include "calculations.hpp"
#include "helper.hpp"

namespace quest_sys {
// Calculations
Quest_Real calcExpecPauliStr(const Qureg& qureg, const PauliStr& str) {
  return ::calcExpecPauliStr(qureg, str);
}

Quest_Real calcExpecPauliStrSum(const Qureg& qureg, const PauliStrSum& sum) {
  return ::calcExpecPauliStrSum(qureg, sum);
}

Quest_Real calcExpecFullStateDiagMatr(const Qureg& qureg,
                                      const FullStateDiagMatr& matr) {
  return ::calcExpecFullStateDiagMatr(qureg, matr);
}

Quest_Real calcExpecFullStateDiagMatrPower(const Qureg& qureg,
                                           const FullStateDiagMatr& matr,
                                           Quest_Complex exponent) {
  return ::calcExpecFullStateDiagMatrPower(qureg, matr, exponent);
}

Quest_Real calcTotalProb(const Qureg& qureg) {
  return ::calcTotalProb(qureg);
}

Quest_Real calcProbOfBasisState(const Qureg& qureg, Quest_Index index) {
  return ::calcProbOfBasisState(qureg, index);
}

Quest_Real calcProbOfQubitOutcome(const Qureg& qureg, int qubit, int outcome) {
  return ::calcProbOfQubitOutcome(qureg, qubit, outcome);
}

Quest_Real calcProbOfMultiQubitOutcome(const Qureg& qureg,
                                       rust::Slice<const int> qubits,
                                       rust::Slice<const int> outcomes) {
  return ::calcProbOfMultiQubitOutcome(
      qureg, quest_helper::slice_to_ptr(qubits),
      quest_helper::slice_to_ptr(outcomes), static_cast<int>(qubits.length()));
}

void calcProbsOfAllMultiQubitOutcomes(rust::Slice<Quest_Real> outcomeProbs,
                                      const Qureg& qureg,
                                      rust::Slice<const int> qubits) {
  return ::calcProbsOfAllMultiQubitOutcomes(outcomeProbs.data(), qureg,
                                            quest_helper::slice_to_ptr(qubits),
                                            static_cast<int>(qubits.length()));
}

Quest_Real calcPurity(const Qureg& qureg) {
  return ::calcPurity(qureg);
}

Quest_Real calcFidelity(const Qureg& qureg, const Qureg& other) {
  return ::calcFidelity(qureg, other);
}

Quest_Real calcDistance(const Qureg& qureg1, const Qureg& qureg2) {
  return ::calcDistance(qureg1, qureg2);
}

std::unique_ptr<Qureg> calcPartialTrace(const Qureg& qureg,
                                        rust::Slice<const int> traceOutQubits) {
  return std::make_unique<Qureg>(
      ::calcPartialTrace(qureg, quest_helper::slice_to_ptr(traceOutQubits),
                         static_cast<int>(traceOutQubits.length())));
}

std::unique_ptr<Qureg> calcReducedDensityMatrix(
    const Qureg& qureg,
    rust::Slice<const int> retainQubits) {
  return std::make_unique<Qureg>(::calcReducedDensityMatrix(
      qureg, quest_helper::slice_to_ptr(retainQubits),
      static_cast<int>(retainQubits.length())));
}

void setQuregToPartialTrace(Qureg& out,
                            const Qureg& in,
                            rust::Slice<const int> traceOutQubits) {
  ::setQuregToPartialTrace(out, in, quest_helper::slice_to_ptr(traceOutQubits),
                           static_cast<int>(traceOutQubits.length()));
}

void setQuregToReducedDensityMatrix(Qureg& out,
                                    const Qureg& in,
                                    rust::Slice<const int> retainQubits) {
  ::setQuregToReducedDensityMatrix(out, in,
                                   quest_helper::slice_to_ptr(retainQubits),
                                   static_cast<int>(retainQubits.length()));
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
