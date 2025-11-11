#include "calculations.hpp"
#include "helper.hpp"

namespace quest_sys {
// Calculations
double calcExpecPauliStr(const Qureg& qureg, const PauliStr& str) {
  return ::calcExpecPauliStr(qureg, str);
}

double calcExpecPauliStrSum(const Qureg& qureg, const PauliStrSum& sum) {
  return ::calcExpecPauliStrSum(qureg, sum);
}

double calcExpecFullStateDiagMatr(const Qureg& qureg,
                                      const FullStateDiagMatr& matr) {
  return ::calcExpecFullStateDiagMatr(qureg, matr);
}

double calcExpecFullStateDiagMatrPower(const Qureg& qureg,
                                           const FullStateDiagMatr& matr,
                                           double exponent) {
  return ::calcExpecFullStateDiagMatrPower(qureg, matr, exponent);
}

double calcTotalProb(const Qureg& qureg) {
  return ::calcTotalProb(qureg);
}

double calcProbOfBasisState(const Qureg& qureg, std::int64_t index) {
  return ::calcProbOfBasisState(qureg, index);
}

double calcProbOfQubitOutcome(const Qureg& qureg, int qubit, int outcome) {
  return ::calcProbOfQubitOutcome(qureg, qubit, outcome);
}

double calcProbOfMultiQubitOutcome(const Qureg& qureg,
                                       rust::cxxbridge1::Slice<const int> qubits,
                                       rust::cxxbridge1::Slice<const int> outcomes) {

  auto qubit_vec = quest_helper::to_vector(qubits);
  auto outcome_vec = quest_helper::to_vector(outcomes);
  return ::calcProbOfMultiQubitOutcome(qureg, qubit_vec, outcome_vec);

}

rust::cxxbridge1::Vec<double> calcProbsOfAllMultiQubitOutcomes(const Qureg& qureg, rust::cxxbridge1::Slice<const int> qubits) {
  auto qubit_vec = quest_helper::to_vector(qubits);
  auto res_vec = ::calcProbsOfAllMultiQubitOutcomes(qureg, qubit_vec);
  rust::cxxbridge1::Vec<double> res;
  res.reserve(res_vec.size());
  for (auto val : res_vec) {
    res.emplace_back(val);
  }
  return res;
}

double calcPurity(const Qureg& qureg) {
  return ::calcPurity(qureg);
}

double calcFidelity(const Qureg& qureg, const Qureg& other) {
  return ::calcFidelity(qureg, other);
}

double calcDistance(const Qureg& qureg1, const Qureg& qureg2) {
  return ::calcDistance(qureg1, qureg2);
}

std::unique_ptr<Qureg> calcPartialTrace(const Qureg& qureg,
                                        rust::cxxbridge1::Slice<const int> traceOutQubits) {
  return std::make_unique<Qureg>(
      ::calcPartialTrace(qureg, quest_helper::slice_to_ptr(traceOutQubits),
                         static_cast<int>(traceOutQubits.length())));
}

std::unique_ptr<Qureg> calcReducedDensityMatrix(
    const Qureg& qureg,
    rust::cxxbridge1::Slice<const int> retainQubits) {
  return std::make_unique<Qureg>(::calcReducedDensityMatrix(
      qureg, quest_helper::slice_to_ptr(retainQubits),
      static_cast<int>(retainQubits.length())));
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
