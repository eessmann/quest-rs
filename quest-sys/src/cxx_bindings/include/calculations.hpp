//
// Created by erich on 3/16/25.
//
#pragma once

#include <quest.h>

#include <memory>
#include "types.hpp"
#include "helper.hpp"

namespace quest_sys {
double calcExpecPauliStr(Qureg const& qureg, PauliStr const& str);

double calcExpecPauliStrSum(Qureg const& qureg, PauliStrSum const& sum);

double calcExpecFullStateDiagMatr(Qureg const& qureg, FullStateDiagMatr const& matr);

double calcExpecFullStateDiagMatrPower(Qureg const& qureg, FullStateDiagMatr const& matr, double exponent);

double calcProbOfBasisState(Qureg const& qureg, std::int64_t index);

double calcProbOfQubitOutcome(Qureg const& qureg, int qubit, int outcome);

double calcProbOfMultiQubitOutcome(Qureg const& qureg, rust::Slice<const int> qubits, rust::Slice<const int> outcomes);

rust::Vec<qreal> calcProbsOfAllMultiQubitOutcomes(Qureg const& qureg, rust::Slice<const int> qubits);

double calcTotalProb(Qureg const& qureg);

double calcPurity(Qureg const& qureg);

double calcFidelity(Qureg const& qureg, Qureg const& other);

double calcDistance(Qureg const& qureg1, Qureg const& qureg2);

std::unique_ptr<Qureg> calcPartialTrace(Qureg const& qureg, rust::Slice<const int> traceOutQubits);

std::unique_ptr<Qureg> calcReducedDensityMatrix(Qureg const& qureg, rust::Slice<const int> retainQubits);

QuestComplex calcInnerProduct(Qureg const& qureg1, Qureg const& qureg2);

QuestComplex calcExpecNonHermitianPauliStrSum(Qureg const& qureg, PauliStrSum const& sum);

QuestComplex calcExpecNonHermitianFullStateDiagMatr(Qureg const& qureg, FullStateDiagMatr const& matr);

QuestComplex calcExpecNonHermitianFullStateDiagMatrPower(Qureg const& qureg, FullStateDiagMatr const& matrix, QuestComplex exponent);

}
