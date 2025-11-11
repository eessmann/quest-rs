//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include "types.hpp"
#include <quest-sys/src/lib.rs.h>

namespace quest_sys {
//  Decoherence
void mixDephasing(Qureg& qureg, int qubit, double prob);

void mixTwoQubitDephasing(Qureg& qureg,
                          int qubit1,
                          int qubit2,
                          double prob);

void mixDepolarising(Qureg& qureg, int qubit, double prob);

void mixTwoQubitDepolarising(Qureg& qureg,
                             int qubit1,
                             int qubit2,
                             double prob);

void mixDamping(Qureg& qureg, int qubit, double prob);

void mixPaulis(Qureg& qureg,
               int qubit,
               double probX,
               double probY,
               double probZ);

void mixQureg(Qureg& qureg, Qureg& other, double prob);

void mixKrausMap(Qureg& qureg,
                 rust::Slice<const int> qubits,
                 const KrausMap& map);

void mixSuperOp(Qureg& qureg, rust::Slice<const int> qubits,
                 const SuperOp& superop);
}  // namespace quest_sys
