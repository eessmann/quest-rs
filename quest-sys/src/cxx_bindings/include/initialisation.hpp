//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <rust/cxx.h>

#include "types.hpp"

namespace quest_sys {
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

}  // namespace quest_sys
