#include "bindings.h"

namespace quest_sys {
    // Calculations
    qreal calcExpecPauliStr(Qureg const& qureg, PauliStr const& str) {
        return ::calcExpecPauliStr(qureg, str);
    }
    qreal calcExpecPauliStrSum(Qureg const& qureg, PauliStrSum const& sum) {
        return ::calcExpecPauliStrSum(qureg, sum);
    }
    qreal calcExpecFullStateDiagMatr(Qureg const& qureg, FullStateDiagMatr const& matr) {
        return ::calcExpecFullStateDiagMatr(qureg, matr);
    }
    qreal calcExpecFullStateDiagMatrPower(Qureg const& qureg, FullStateDiagMatr const& matr, Quest_Complex exponent) {
        return ::calcExpecFullStateDiagMatrPower(qureg, matr, exponent);
    }

    qreal calcTotalProb(Qureg const& qureg) {
        return ::calcTotalProb(qureg);
    }
    qreal calcProbOfBasisState(Qureg const& qureg , qindex index) {
        return ::calcProbOfBasisState(qureg, index);
    }
    qreal calcProbOfQubitOutcome(Qureg const& qureg, int qubit, int outcome) {
        return ::calcProbOfQubitOutcome(qureg, qubit, outcome);
    }
    qreal calcProbOfMultiQubitOutcome(Qureg const& qureg, rust::Slice<int> qubits, rust::Slice<int> outcomes, int numQubits) {
        return ::calcProbOfMultiQubitOutcome(qureg, qubits.data(), outcomes.data(), numQubits);
    }
    void  calcProbsOfAllMultiQubitOutcomes(rust::Slice<qreal> outcomeProbs, Qureg const& qureg, rust::Slice<int> qubits, int numQubits) {
        return ::calcProbsOfAllMultiQubitOutcomes(outcomeProbs.data(), qureg, qubits.data(), numQubits);
    }

    qreal calcPurity(Qureg const& qureg) {
        return ::calcPurity(qureg);
    }
    qreal calcFidelity(Qureg const& qureg, Qureg const& other) {
        return ::calcFidelity(qureg, other);
    }
    qreal calcDistance(Qureg const& qureg1, Qureg const& qureg2) {
        return ::calcDistance(qureg1, qureg2);
    }

    std::unique_ptr<Qureg> calcPartialTrace(Qureg const& qureg, rust::Slice<int> traceOutQubits, int numTraceQubits) {
        return std::make_unique<Qureg>(::calcPartialTrace(qureg, traceOutQubits.data(), numTraceQubits));
    }
    std::unique_ptr<Qureg> calcReducedDensityMatrix(Qureg const& qureg, rust::Slice<int> retainQubits, int numRetainQubits) {
        return std::make_unique<Qureg>(::calcReducedDensityMatrix(qureg, retainQubits.data(), numRetainQubits));
    }
    void setQuregToPartialTrace(Qureg& out, Qureg const& in, rust::Slice<int> traceOutQubits, int numTraceQubits) {
        ::setQuregToPartialTrace(out, in, traceOutQubits.data(), numTraceQubits);
    }
    void setQuregToReducedDensityMatrix(Qureg& out, Qureg const& in, rust::Slice<int> retainQubits, int numRetainQubits) {
        ::setQuregToReducedDensityMatrix(out, in, retainQubits.data(), numRetainQubits);
    }

    Quest_Complex calcInnerProduct(Qureg const& qureg1, Qureg const& qureg2) {
        return ::calcInnerProduct(qureg1, qureg2);
    }
    Quest_Complex calcExpecNonHermitianPauliStrSum(Qureg const& qureg, PauliStrSum const& sum) {
        return ::calcExpecNonHermitianPauliStrSum(qureg, sum);
    }
    Quest_Complex calcExpecNonHermitianFullStateDiagMatr(Qureg const& qureg, FullStateDiagMatr const& matr) {
        return ::calcExpecNonHermitianFullStateDiagMatr(qureg, matr);
    }
    Quest_Complex calcExpecNonHermitianFullStateDiagMatrPower(Qureg const& qureg, FullStateDiagMatr const& matrix, Quest_Complex exponent) {
        return ::calcExpecNonHermitianFullStateDiagMatrPower(qureg, matrix, exponent);
    }
}