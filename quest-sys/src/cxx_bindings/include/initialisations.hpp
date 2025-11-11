//
// Created by Erich Essmann on 11/11/2025.
//
#pragma once
#include <quest.h>

#include "types.hpp"
#include <quest-sys/src/lib.rs.h>

namespace quest_sys {
void initBlankState(Qureg& qureg);

void initZeroState(Qureg& qureg);

void initPlusState(Qureg& qureg);

void initPureState(Qureg& qureg, Qureg& pure);

void initClassicalState(Qureg& qureg, std::int64_t stateInd);

void initDebugState(Qureg& qureg);

void initArbitraryPureState(Qureg& qureg,
                            rust::Slice<const QuestComplex> amps);

void initRandomPureState(Qureg& qureg);

void initRandomMixedState(Qureg& qureg, std::int64_t numPureStates);

void setQuregAmps(Qureg& qureg,
                  std::int64_t startInd,
                  rust::Slice<const QuestComplex> amps);

void setDensityQuregAmps(
    Qureg& qureg,
    std::int64_t startRow,
    std::int64_t startCol,
    rust::Slice<const rust::Slice<const QuestComplex>> amps);

void setDensityQuregFlatAmps(Qureg& qureg,
                             std::int64_t startInd,
                             rust::Slice<const QuestComplex> amps);

void setQuregToClone(Qureg& targetQureg, const Qureg& copyQureg);

double setQuregToRenormalized(Qureg& qureg);

void setQuregToPauliStrSum(Qureg& qureg, const PauliStrSum& sum);

void setQuregToPartialTrace(Qureg& out, const Qureg& in, rust::Vec<int> traceQubits);

void setQuregToReducedDensityMatrix(Qureg& out, const Qureg& in, rust::Vec<int> retainQubits);


}  // namespace quest_sys
