//
// Created by Erich Essmann on 12/03/2025.
//
#include "bindings.h"
#include "helper.h"

namespace quest_sys {
std::unique_ptr<CompMatr1> getCompMatr1(
    rust::Slice<const rust::Slice<const Quest_Complex>> in) {
  std::vector<qcomp*> tmp{};
  std::ranges::transform(in, std::back_inserter(tmp), [](auto val) {
    return Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(val));
  });
  return std::make_unique<CompMatr1>(::getCompMatr1(tmp.data()));
}

std::unique_ptr<CompMatr2> getCompMatr2(
    rust::Slice<const rust::Slice<const Quest_Complex>> in) {
  std::vector<qcomp*> tmp{};
  std::ranges::transform(in, std::back_inserter(tmp), [](auto val) {
    return Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(val));
  });
  return std::make_unique<CompMatr2>(::getCompMatr2(tmp.data()));
}

std::unique_ptr<DiagMatr1> getDiagMatr1(rust::Slice<const Quest_Complex> in) {
  return std::make_unique<DiagMatr1>(::getDiagMatr1(
      Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(in))));
}

std::unique_ptr<DiagMatr2> getDiagMatr2(rust::Slice<const Quest_Complex> in) {
  return std::make_unique<DiagMatr2>(::getDiagMatr2(
      Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(in))));
}

std::unique_ptr<CompMatr> createCompMatr(int numQubits) {
  return std::make_unique<CompMatr>(::createCompMatr(numQubits));
}

std::unique_ptr<DiagMatr> createDiagMatr(int numQubits) {
  return std::make_unique<DiagMatr>(::createDiagMatr(numQubits));
}

std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatr(int numQubits) {
  return std::make_unique<FullStateDiagMatr>(
      ::createFullStateDiagMatr(numQubits));
}

std::unique_ptr<FullStateDiagMatr>
createCustomFullStateDiagMatr(int numQubits, int useDistrib, int useGpuAccel) {
  return std::make_unique<FullStateDiagMatr>(
      ::createCustomFullStateDiagMatr(numQubits, useDistrib, useGpuAccel));
}

void destroyCompMatr(CompMatr& matrix) {
  ::destroyCompMatr(matrix);
}

void destroyDiagMatr(DiagMatr& matrix) {
  ::destroyDiagMatr(matrix);
}

void destroyFullStateDiagMatr(FullStateDiagMatr& matrix) {
  ::destroyFullStateDiagMatr(matrix);
};

void syncCompMatr(CompMatr& matr) {
  ::syncCompMatr(matr);
}

void syncDiagMatr(DiagMatr& matr) {
  ::syncDiagMatr(matr);
}

void syncFullStateDiagMatr(FullStateDiagMatr& matr) {
  ::syncFullStateDiagMatr(matr);
}

void setCompMatr(CompMatr& out,
                 rust::Slice<const rust::Slice<const Quest_Complex>> in) {
  std::vector<qcomp*> tmp{};
  std::ranges::transform(in, std::back_inserter(tmp), [](auto val) {
    return Quest_Complex::to_qcomp_ptr(quest_helper::slice_to_ptr(val));
  });
  ::setCompMatr(out, tmp.data());
}

void setDiagMatr(DiagMatr& out, rust::Slice<const Quest_Complex> in) {
  ::setDiagMatr(
      out, Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(in.data())));
}

void setFullStateDiagMatr(FullStateDiagMatr& out,
                          qindex startInd,
                          rust::Slice<const Quest_Complex> in) {
  ::setFullStateDiagMatr(
      out, startInd,
      Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(in.data())),
      static_cast<int>(in.length()));
}

std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatrFromPauliStrSum(
    const PauliStrSum& in) {
  return std::make_unique<FullStateDiagMatr>(
      ::createFullStateDiagMatrFromPauliStrSum(in));
}

void setFullStateDiagMatrFromPauliStrSum(FullStateDiagMatr& out,
                                         const PauliStrSum& in) {
  ::setFullStateDiagMatrFromPauliStrSum(out, in);
}

void reportCompMatr1(const CompMatr1& matrix) {
  ::reportCompMatr1(matrix);
}

void reportCompMatr2(const CompMatr2& matrix) {
  ::reportCompMatr2(matrix);
}

void reportCompMatr(const CompMatr& matrix) {
  ::reportCompMatr(matrix);
}

void reportDiagMatr1(const DiagMatr1& matrix) {
  ::reportDiagMatr1(matrix);
}

void reportDiagMatr2(const DiagMatr2& matrix) {
  ::reportDiagMatr2(matrix);
}

void reportDiagMatr(const DiagMatr& matrix) {
  ::reportDiagMatr(matrix);
}

void reportFullStateDiagMatr(const FullStateDiagMatr& matr) {
  ::reportFullStateDiagMatr(matr);
}
}  // namespace quest_sys
