//
// Created by erich on 3/16/25.
//
#pragma once

#include <quest.h>
#include <rust/cxx.h>
#include <memory>
#include "types.hpp"

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
}  // namespace quest_sys