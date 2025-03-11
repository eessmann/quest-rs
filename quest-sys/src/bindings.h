#pragma once

#include "quest/include/quest.h"
#include <cstdint>
#include <memory>
#include <vector>
#include <complex>

// Define some wrapper functions for Rust to interact with QuEST
namespace quest_sys {

    // Tag types for cxx::ExternType
    struct QuregTag {};
    struct QuESTEnvTag {};
    struct PauliStrTag {};
    struct PauliStrSumTag {};

    // Precision-dependent type aliases based on QuEST configuration
    #if FLOAT_PRECISION == 1
        using Complex = std::complex<float>;
        using Real = float;
    #elif FLOAT_PRECISION == 2
        using Complex = std::complex<double>;
        using Real = double;
    #elif FLOAT_PRECISION == 4
        using Complex = std::complex<long double>;
        using Real = long double;
    #else
        #error "Unsupported FLOAT_PRECISION value"
    #endif

    // Environment management
    void initQuESTEnv();
    void finalizeQuESTEnv();
    std::unique_ptr<QuESTEnv> getQuESTEnv();
    bool isMultithreaded(const QuESTEnv& env);
    bool isDistributed(const QuESTEnv& env);
    bool isGpuAccelerated(const QuESTEnv& env);

    // Qureg management
    std::unique_ptr<Qureg> createQureg(int numQubits);
    std::unique_ptr<Qureg> createDensityQureg(int numQubits);
    std::unique_ptr<Qureg> cloneQureg(const Qureg& qureg);
    void destroyQureg(Qureg& qureg);

    // Qureg property accessors
    int getNumQubits(const Qureg& qureg);
    uint64_t getNumAmps(const Qureg& qureg);
    bool isDensityMatrix(const Qureg& qureg);

    // State initialization
    void initZeroState(Qureg& qureg);
    void initPlusState(Qureg& qureg);
    void initClassicalState(Qureg& qureg, uint64_t stateInd);
    void initPureState(Qureg& qureg, const Qureg& pure);
    void initRandomPureState(Qureg& qureg);

    // Basic gates
    void pauliX(Qureg& qureg, int targetQubit);
    void pauliY(Qureg& qureg, int targetQubit);
    void pauliZ(Qureg& qureg, int targetQubit);
    void hadamard(Qureg& qureg, int targetQubit);
    void rotateX(Qureg& qureg, int targetQubit, double angle);
    void rotateY(Qureg& qureg, int targetQubit, double angle);
    void rotateZ(Qureg& qureg, int targetQubit, double angle);
    void sGate(Qureg& qureg, int targetQubit);
    void tGate(Qureg& qureg, int targetQubit);
    void phaseShift(Qureg& qureg, int targetQubit, double angle);

    // Multi-qubit gates
    void controlledNot(Qureg& qureg, int controlQubit, int targetQubit);
    void controlledPauliY(Qureg& qureg, int controlQubit, int targetQubit);
    void controlledPauliZ(Qureg& qureg, int controlQubit, int targetQubit);
    void swapGate(Qureg& qureg, int qubit1, int qubit2);

    // Matrix applications
    void applyMatrix2(Qureg& qureg, int targetQubit, const std::vector<std::vector<Complex>>& matrix);
    void applyMatrix4(Qureg& qureg, int targetQubit1, int targetQubit2, const std::vector<std::vector<Complex>>& matrix);
    void applyMultiControlledGate(Qureg& qureg, const std::vector<int>& controlQubits, int targetQubit, const std::vector<std::vector<Complex>>& matrix);

    // Measurements
    int measure(Qureg& qureg, int qubit);
    int measureWithProb(Qureg& qureg, int qubit, double& prob);
    double calcProbOfOutcome(const Qureg& qureg, int qubit, int outcome);

    // Calculations
    double calcFidelity(const Qureg& qureg1, const Qureg& qureg2);
    Complex calcInnerProduct(const Qureg& bra, const Qureg& ket);
    double calcProbOfAllOutcomes(const Qureg& qureg, const std::vector<int>& qubits);

    // Paulis and operators
    std::unique_ptr<PauliStrSum> createPauliHamil(int numQubits, int numTerms);

    // State reporting
    void reportStateToScreen(const Qureg& qureg, int precision);
}