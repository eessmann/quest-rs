//
// Created by Erich Essmann on 12/03/2025.
//
#include "bindings.h"
#include "helper.h"

namespace quest_sys {
void initBlankState(Qureg& qureg) {
  ::initBlankState(qureg);
}

void initZeroState(Qureg& qureg) {
  ::initZeroState(qureg);
}

void initPlusState(Qureg& qureg) {
  ::initPlusState(qureg);
}

void initPureState(Qureg& qureg, Qureg& pure) {
  ::initPureState(qureg, pure);
}

void initClassicalState(Qureg& qureg, qindex stateInd) {
  ::initClassicalState(qureg, stateInd);
}

void initDebugState(Qureg& qureg) {
  ::initDebugState(qureg);
}

void initArbitraryPureState(Qureg& qureg,
                            rust::Slice<const Quest_Complex> amps) {
  ::initArbitraryPureState(
      qureg, Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(amps)));
}

void initRandomPureState(Qureg& qureg) {
  ::initRandomPureState(qureg);
}

void initRandomMixedState(Qureg& qureg, qindex numPureStates) {
  ::initRandomMixedState(qureg, numPureStates);
}

void setQuregAmps(Qureg& qureg,
                  qindex startInd,
                  rust::Slice<const Quest_Complex> amps) {
  ::setQuregAmps(qureg, startInd,
                 Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(amps)),
                 static_cast<int>(amps.length()));
}

void setDensityQuregAmps(
    Qureg& qureg,
    qindex startRow,
    qindex startCol,
    rust::Slice<const rust::Slice<const Quest_Complex>> amps) {
  int rows = amps.length();
  int cols = amps[0].length();

  std::vector<qcomp*> tmp{};
  std::ranges::transform(amps, std::back_inserter(tmp), [](auto val) {
    return Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(val));
  });
  ::setDensityQuregAmps(qureg, startRow, startCol, tmp.data(), rows, cols);
}

void setDensityQuregFlatAmps(Qureg& qureg,
                             qindex startInd,
                             rust::Slice<const Quest_Complex> amps) {
  ::setDensityQuregFlatAmps(
      qureg, startInd,
      Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(amps)),
      static_cast<int>(amps.length()));
}

void setQuregToClone(Qureg& targetQureg, const Qureg& copyQureg) {
  ::setQuregToClone(targetQureg, copyQureg);
}

void setQuregToSuperposition(Quest_Complex facOut,
                             Qureg& out,
                             Quest_Complex fac1,
                             const Qureg& qureg1,
                             qcomp fac2,
                             const Qureg& qureg2) {
  ::setQuregToSuperposition(facOut, out, fac1, qureg1, fac2, qureg2);
}

qreal setQuregToRenormalized(Qureg& qureg) {
  return ::setQuregToRenormalized(qureg);
}

void setQuregToPauliStrSum(Qureg& qureg, const PauliStrSum& sum) {
  ::setQuregToPauliStrSum(qureg, sum);
}
}  // namespace quest_sys
