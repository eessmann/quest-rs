//
// Created by Erich Essmann on 12/03/2025.
//
#include "bindings.h"

namespace quest_sys {
    void mixDephasing(Qureg& qureg, int qubit, qreal prob) {
        ::mixDephasing(qureg, qubit, prob);
    }
    void mixTwoQubitDephasing(Qureg& qureg, int qubit1, int qubit2, qreal prob) {
        ::mixTwoQubitDephasing(qureg, qubit1, qubit2, prob);
    }
    void mixDepolarising(Qureg& qureg, int qubit, qreal prob) {
        ::mixDepolarising(qureg, qubit, prob);
    }
    void mixTwoQubitDepolarising(Qureg& qureg, int qubit1, int qubit2, qreal prob) {
        ::mixTwoQubitDepolarising(qureg, qubit1, qubit2, prob);
    }
    void mixDamping(Qureg& qureg, int qubit, qreal prob) {
        ::mixDamping(qureg, qubit, prob);
    }
    void mixPaulis(Qureg& qureg, int qubit, qreal probX, qreal probY, qreal probZ) {
        ::mixPaulis(qureg, qubit, probX, probY, probZ);
    }
    void mixQureg(Qureg& qureg, Qureg& other, qreal prob) {
        ::mixQureg(qureg, other, prob);
    }
    void mixKrausMap(Qureg& qureg, rust::Slice<const int> qubits, KrausMap const& map) {
        ::mixKrausMap(qureg, const_cast<int*>(qubits.data()), static_cast<int>(qubits.size()), map);
    }
}