//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <rust/cxx.h>

#include "types.hpp"

namespace quest_sys {
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
}  // namespace quest_sys
