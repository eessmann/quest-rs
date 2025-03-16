//
// Created by Erich Essmann on 12/03/2025.
//
#include "decoherence.hpp"
#include "helper.hpp"

namespace quest_sys {
void mixDephasing(Qureg& qureg, int qubit, Quest_Real prob) {
  ::mixDephasing(qureg, qubit, prob);
}

void mixTwoQubitDephasing(Qureg& qureg,
                          int qubit1,
                          int qubit2,
                          Quest_Real prob) {
  ::mixTwoQubitDephasing(qureg, qubit1, qubit2, prob);
}

void mixDepolarising(Qureg& qureg, int qubit, Quest_Real prob) {
  ::mixDepolarising(qureg, qubit, prob);
}

void mixTwoQubitDepolarising(Qureg& qureg,
                             int qubit1,
                             int qubit2,
                             Quest_Real prob) {
  ::mixTwoQubitDepolarising(qureg, qubit1, qubit2, prob);
}

void mixDamping(Qureg& qureg, int qubit, Quest_Real prob) {
  ::mixDamping(qureg, qubit, prob);
}

void mixPaulis(Qureg& qureg,
               int qubit,
               Quest_Real probX,
               Quest_Real probY,
               Quest_Real probZ) {
  ::mixPaulis(qureg, qubit, probX, probY, probZ);
}

void mixQureg(Qureg& qureg, Qureg& other, Quest_Real prob) {
  ::mixQureg(qureg, other, prob);
}

void mixKrausMap(Qureg& qureg,
                 rust::Slice<const int> qubits,
                 const KrausMap& map) {
  ::mixKrausMap(qureg, quest_helper::slice_to_ptr(qubits),
                static_cast<int>(qubits.size()), map);
}
}  // namespace quest_sys
