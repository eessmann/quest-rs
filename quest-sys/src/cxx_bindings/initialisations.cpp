//
// Created by Erich Essmann on 12/03/2025.
//
#include "initialisation.hpp"
#include "helper.hpp"

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

void initClassicalState(Qureg& qureg, std::int64_t stateInd) {
  ::initClassicalState(qureg, stateInd);
}

void initDebugState(Qureg& qureg) {
  ::initDebugState(qureg);
}

void initArbitraryPureState(Qureg& qureg,
                            rust::cxxbridge1::Slice<const Quest_Complex> amps) {
  ::initArbitraryPureState(
      qureg, Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(amps)));
}

void initRandomPureState(Qureg& qureg) {
  ::initRandomPureState(qureg);
}

void initRandomMixedState(Qureg& qureg, std::int64_t numPureStates) {
  ::initRandomMixedState(qureg, numPureStates);
}

void setQuregAmps(Qureg& qureg,
                  std::int64_t startInd,
                  rust::cxxbridge1::Slice<const Quest_Complex> amps) {
  ::setQuregAmps(qureg, startInd,
                 Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(amps)),
                 static_cast<int>(amps.length()));
}

void setDensityQuregAmps(
    Qureg& qureg,
    std::int64_t startRow,
    std::int64_t startCol,
    rust::cxxbridge1::Slice<const rust::cxxbridge1::Slice<const Quest_Complex>> amps) {
  int rows = static_cast<int>(amps.length());
  int cols = static_cast<int>(amps[0].length());

  std::vector<qcomp*> tmp{};
  std::ranges::transform(amps, std::back_inserter(tmp), [](auto val) {
    return Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(val));
  });
  ::setDensityQuregAmps(qureg, startRow, startCol, tmp.data(), rows, cols);
}

void setDensityQuregFlatAmps(Qureg& qureg,
                             std::int64_t startInd,
                             rust::cxxbridge1::Slice<const Quest_Complex> amps) {
  ::setDensityQuregFlatAmps(
      qureg, startInd,
      Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(amps)),
      static_cast<int>(amps.length()));
}

void setQuregToClone(Qureg& targetQureg, const Qureg& copyQureg) {
  ::setQuregToClone(targetQureg, copyQureg);
}


double setQuregToRenormalized(Qureg& qureg) {
  return ::setQuregToRenormalized(qureg);
}

void setQuregToPauliStrSum(Qureg& qureg, const PauliStrSum& sum) {
  ::setQuregToPauliStrSum(qureg, sum);
}
}  // namespace quest_sys
