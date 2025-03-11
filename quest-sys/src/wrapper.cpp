#include "bindings.h"
#include <stdexcept>
#include <cstring>

namespace quest_sys {

    // Environment management
    void initQuESTEnv() {
        ::initQuESTEnv();
    }

    void finalizeQuESTEnv() {
        ::finalizeQuESTEnv();
    }

    std::unique_ptr<QuESTEnv> getQuESTEnv() {
        auto env = std::make_unique<QuESTEnv>(::getQuESTEnv());
        return env;
    }

    bool isMultithreaded(const QuESTEnv& env) {
        return env.isMultithreaded != 0;
    }

    bool isDistributed(const QuESTEnv& env) {
        return env.isDistributed != 0;
    }

    bool isGpuAccelerated(const QuESTEnv& env) {
        return env.isGpuAccelerated != 0;
    }

    // Qureg management
    std::unique_ptr<Qureg> createQureg(int numQubits) {
        auto qureg = std::make_unique<Qureg>(::createQureg(numQubits));
        return qureg;
    }

    std::unique_ptr<Qureg> createDensityQureg(int numQubits) {
        auto qureg = std::make_unique<Qureg>(::createDensityQureg(numQubits));
        return qureg;
    }

    std::unique_ptr<Qureg> cloneQureg(const Qureg& qureg) {
        auto clone = std::make_unique<Qureg>(::createCloneQureg(qureg));
        return clone;
    }

    void destroyQureg(Qureg& qureg) {
        ::destroyQureg(qureg);
    }

    // Getters for Qureg properties
    int getNumQubits(const Qureg& qureg) {
        return qureg.numQubits;
    }

    uint64_t getNumAmps(const Qureg& qureg) {
        return qureg.numAmps;
    }

    bool isDensityMatrix(const Qureg& qureg) {
        return qureg.isDensityMatrix != 0;
    }

    // State initialization
    void initZeroState(Qureg& qureg) {
        ::initZeroState(qureg);
    }

    void initPlusState(Qureg& qureg) {
        ::initPlusState(qureg);
    }

    void initClassicalState(Qureg& qureg, uint64_t stateInd) {
        ::initClassicalState(qureg, stateInd);
    }

    void initPureState(Qureg& qureg, const Qureg& pure) {
        ::initPureState(qureg, pure);
    }

    void initRandomPureState(Qureg& qureg) {
        ::initRandomPureState(qureg);
    }

    // Basic gates
    void pauliX(Qureg& qureg, int targetQubit) {
        ::applyPauliX(qureg, targetQubit);
    }

    void pauliY(Qureg& qureg, int targetQubit) {
        ::applyPauliY(qureg, targetQubit);
    }

    void pauliZ(Qureg& qureg, int targetQubit) {
        ::applyPauliZ(qureg, targetQubit);
    }

    void hadamard(Qureg& qureg, int targetQubit) {
        ::applyHadamard(qureg, targetQubit);
    }

    void rotateX(Qureg& qureg, int targetQubit, double angle) {
        ::applyRotateX(qureg, targetQubit, static_cast<Real>(angle));
    }

    void rotateY(Qureg& qureg, int targetQubit, double angle) {
        ::applyRotateY(qureg, targetQubit, static_cast<Real>(angle));
    }

    void rotateZ(Qureg& qureg, int targetQubit, double angle) {
        ::applyRotateZ(qureg, targetQubit, static_cast<Real>(angle));
    }

    void sGate(Qureg& qureg, int targetQubit) {
        ::applyS(qureg, targetQubit);
    }

    void tGate(Qureg& qureg, int targetQubit) {
        ::applyT(qureg, targetQubit);
    }

    void phaseShift(Qureg& qureg, int targetQubit, double angle) {
        ::applyPhaseShift(qureg, targetQubit, static_cast<Real>(angle));
    }

    // Multi-qubit gates
    void controlledNot(Qureg& qureg, int controlQubit, int targetQubit) {
        ::applyControlledPauliX(qureg, controlQubit, targetQubit);
    }

    void controlledPauliY(Qureg& qureg, int controlQubit, int targetQubit) {
        ::applyControlledPauliY(qureg, controlQubit, targetQubit);
    }

    void controlledPauliZ(Qureg& qureg, int controlQubit, int targetQubit) {
        ::applyControlledPauliZ(qureg, controlQubit, targetQubit);
    }

    void swapGate(Qureg& qureg, int qubit1, int qubit2) {
        ::applySwap(qureg, qubit1, qubit2);
    }

    // Matrix applications
    void applyMatrix2(Qureg& qureg, int targetQubit, const std::vector<std::vector<Complex>>& matrix) {
        if (matrix.size() != 2 || matrix[0].size() != 2 || matrix[1].size() != 2) {
            throw std::invalid_argument("Matrix must be 2x2");
        }

        // Create CompMatr1
        CompMatr1 matr;
        matr.numQubits = 1;
        matr.numRows = 2;

        // Fill in the matrix elements
        for (int i = 0; i < 2; i++) {
            for (int j = 0; j < 2; j++) {
                matr.elems[i][j] = getQcomp(matrix[i][j].real(), matrix[i][j].imag());
            }
        }

        // Apply the matrix
        ::applyCompMatr1(qureg, targetQubit, matr);
    }

    void applyMatrix4(Qureg& qureg, int targetQubit1, int targetQubit2, const std::vector<std::vector<Complex>>& matrix) {
        if (matrix.size() != 4 || matrix[0].size() != 4) {
            throw std::invalid_argument("Matrix must be 4x4");
        }

        // Create CompMatr2
        CompMatr2 matr;
        matr.numQubits = 2;
        matr.numRows = 4;

        // Fill in the matrix elements
        for (int i = 0; i < 4; i++) {
            for (int j = 0; j < 4; j++) {
                matr.elems[i][j] = getQcomp(matrix[i][j].real(), matrix[i][j].imag());
            }
        }

        // Apply the matrix
        ::applyCompMatr2(qureg, targetQubit1, targetQubit2, matr);
    }

    void applyMultiControlledGate(Qureg& qureg, const std::vector<int>& controlQubits, int targetQubit, const std::vector<std::vector<Complex>>& matrix) {
        if (matrix.size() != 2 || matrix[0].size() != 2) {
            throw std::invalid_argument("Matrix must be 2x2");
        }

        // Create CompMatr1
        CompMatr1 matr;
        matr.numQubits = 1;
        matr.numRows = 2;

        // Fill in the matrix elements
        for (int i = 0; i < 2; i++) {
            for (int j = 0; j < 2; j++) {
                matr.elems[i][j] = getQcomp(matrix[i][j].real(), matrix[i][j].imag());
            }
        }

        // Apply the multi-controlled gate
        ::applyMultiControlledCompMatr1(qureg, const_cast<int*>(controlQubits.data()), controlQubits.size(), targetQubit, matr);
    }

    // Measurements
    int measure(Qureg& qureg, int qubit) {
        return ::applyQubitMeasurement(qureg, qubit);
    }

    int measureWithProb(Qureg& qureg, int qubit, double& prob) {
        qreal questProb;
        int result = ::applyQubitMeasurementAndGetProb(qureg, qubit, &questProb);
        prob = static_cast<double>(questProb);
        return result;
    }

    double calcProbOfOutcome(const Qureg& qureg, int qubit, int outcome) {
        return static_cast<double>(::calcProbOfQubitOutcome(qureg, qubit, outcome));
    }

    // Calculations
    double calcFidelity(const Qureg& qureg1, const Qureg& qureg2) {
        return static_cast<double>(::calcFidelity(qureg1, qureg2));
    }

    Complex calcInnerProduct(const Qureg& bra, const Qureg& ket) {
        qcomp result = ::calcInnerProduct(bra, ket);
        return Complex(real(result), imag(result));
    }

    double calcProbOfAllOutcomes(const Qureg& qureg, const std::vector<int>& qubits) {
        std::vector<qreal> probs(1 << qubits.size());
        ::calcProbsOfAllMultiQubitOutcomes(probs.data(), qureg, const_cast<int*>(qubits.data()), qubits.size());
        // Sum probabilities
        double totalProb = 0.0;
        for (qreal p : probs) {
            totalProb += static_cast<double>(p);
        }
        return totalProb;
    }
    
    // Paulis and operators
    std::unique_ptr<PauliStrSum> createPauliHamil(int numQubits, int numTerms) {
        PauliStr* strings = new PauliStr[numTerms];
        qcomp* coeffs = new qcomp[numTerms];

        // Create an empty PauliStrSum
        auto sum = ::createPauliStrSum(strings, coeffs, numTerms);

        // Clean up temporary arrays
        delete[] strings;
        delete[] coeffs;

        return std::make_unique<PauliStrSum>(sum);
    }

    void reportStateToScreen(const Qureg& qureg, int precision) {
        ::reportQureg(qureg);
    }
}