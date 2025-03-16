//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <rust/cxx.h>
#include <memory>

#include "types.hpp"

namespace quest_sys {
// Matrices
std::unique_ptr<CompMatr1> getCompMatr1(
    rust::Slice<const rust::Slice<const Quest_Complex>> in);

std::unique_ptr<CompMatr2> getCompMatr2(
    rust::Slice<const rust::Slice<const Quest_Complex>> in);

std::unique_ptr<DiagMatr1> getDiagMatr1(rust::Slice<const Quest_Complex> in);

std::unique_ptr<DiagMatr2> getDiagMatr2(rust::Slice<const Quest_Complex> in);

std::unique_ptr<CompMatr> createCompMatr(int numQubits);

std::unique_ptr<DiagMatr> createDiagMatr(int numQubits);

std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatr(int numQubits);

std::unique_ptr<FullStateDiagMatr>
createCustomFullStateDiagMatr(int numQubits, int useDistrib, int useGpuAccel);

void destroyCompMatr(CompMatr& matrix);

void destroyDiagMatr(DiagMatr& matrix);

void destroyFullStateDiagMatr(FullStateDiagMatr& matrix);

void syncCompMatr(CompMatr& matr);

void syncDiagMatr(DiagMatr& matr);

void syncFullStateDiagMatr(FullStateDiagMatr& matr);

void setCompMatr(CompMatr& out,
                 rust::Slice<const rust::Slice<const Quest_Complex>> in);

void setDiagMatr(DiagMatr& out, rust::Slice<const Quest_Complex> in);

void setFullStateDiagMatr(FullStateDiagMatr& out,
                          Quest_Index startInd,
                          rust::Slice<const Quest_Complex> in);

std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatrFromPauliStrSum(
    const PauliStrSum& in);

void setFullStateDiagMatrFromPauliStrSum(FullStateDiagMatr& out,
                                         const PauliStrSum& in);

void reportCompMatr1(const CompMatr1& matrix);

void reportCompMatr2(const CompMatr2& matrix);

void reportCompMatr(const CompMatr& matrix);

void reportDiagMatr1(const DiagMatr1& matrix);

void reportDiagMatr2(const DiagMatr2& matrix);

void reportDiagMatr(const DiagMatr& matrix);

void reportFullStateDiagMatr(const FullStateDiagMatr& matr);

}  // namespace quest_sys
