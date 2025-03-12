//
// Created by Erich Essmann on 12/03/2025.
//
#include "bindings.h"
#include "helper.h"

namespace quest_sys {
    std::unique_ptr<CompMatr1> getCompMatr1(rust::Slice<const rust::Slice<const Quest_Complex>> in) {
        std::vector<qcomp*> tmp{};
        std::ranges::transform(
            in,
            std::back_inserter(tmp),
            [](auto val) {return Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(val.data()));}
            );
        return std::make_unique<CompMatr1>(::getCompMatr1(tmp.data()));
    }
    std::unique_ptr<CompMatr2> getCompMatr2(rust::Slice<const rust::Slice<const Quest_Complex>> in) {
        std::vector<qcomp*> tmp{};
        std::ranges::transform(
            in,
            std::back_inserter(tmp),
            [](auto val) {return Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(val.data()));}
            );
        return std::make_unique<CompMatr2>(::getCompMatr2(tmp.data()));
    }

    std::unique_ptr<DiagMatr1> getDiagMatr1(rust::Slice<const Quest_Complex> in) {
        return std::make_unique<DiagMatr1>(::getDiagMatr1(Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(in.data()))));
    }
    std::unique_ptr<DiagMatr2> getDiagMatr2(rust::Slice<const Quest_Complex> in) {
        return std::make_unique<DiagMatr2>(::getDiagMatr2(Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(in.data()))));
    }

    std::unique_ptr<CompMatr> createCompMatr(int numQubits) {
        return std::make_unique<CompMatr>(::createCompMatr(numQubits));
    }
    std::unique_ptr<DiagMatr> createDiagMatr(int numQubits) {
        return std::make_unique<DiagMatr>(::createDiagMatr(numQubits));
    }
    std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatr(int numQubits) {
        return std::make_unique<FullStateDiagMatr>(::createFullStateDiagMatr(numQubits));
    }
    std::unique_ptr<FullStateDiagMatr> createCustomFullStateDiagMatr(int numQubits, int useDistrib, int useGpuAccel) {
        return std::make_unique<FullStateDiagMatr>(::createCustomFullStateDiagMatr(numQubits, useDistrib, useGpuAccel));
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

    void setCompMatr(CompMatr& out, rust::Slice<const rust::Slice<const Quest_Complex>> in) {
        std::vector<qcomp*> tmp{};
        std::ranges::transform(
            in,
            std::back_inserter(tmp),
            [](auto val) {return Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(val.data()));}
            );
        ::setCompMatr(out, tmp.data());
    }
    void setDiagMatr(DiagMatr& out, rust::Slice<const Quest_Complex> in) {
        ::setDiagMatr(out, Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(in.data())));
    }
    void setFullStateDiagMatr(FullStateDiagMatr& out, qindex startInd, rust::Slice<const Quest_Complex> in) {
        ::setFullStateDiagMatr(out, startInd, Quest_Complex::to_qcomp_ptr(const_cast<Quest_Complex*>(in.data())),static_cast<int>(in.length()));
    }

    void setInlineCompMatr(CompMatr& matr, int numQb, rust::Slice<const rust::Slice<const Quest_Complex>> in) {
    }
    void setInlineDiagMatr(DiagMatr& matr, int numQb, rust::Slice<const Quest_Complex> in);
    void setInlineFullStateDiagMatr(FullStateDiagMatr& matr, qindex startInd, qindex numElems, rust::Slice<const Quest_Complex> in);

    std::unique_ptr<CompMatr> createInlineCompMatr(int numQb, rust::Slice<const rust::Slice<const Quest_Complex>> elems);
    std::unique_ptr<DiagMatr> createInlineDiagMatr(int numQb, rust::Slice<const Quest_Complex> elems);

    void setDiagMatrFromMultiVarFunc(DiagMatr& out, rust::Fn<Quest_Complex(rust::Slice<const qindex>)> func, rust::Slice<const int> numQubitsPerVar, int numVars, int areSigned);

    std::unique_ptr<FullStateDiagMatr> createFullStateDiagMatrFromPauliStrSum(PauliStrSum const& in);
    void setFullStateDiagMatrFromPauliStrSum(FullStateDiagMatr& out, PauliStrSum const& in);
    void setFullStateDiagMatrFromMultiVarFunc(FullStateDiagMatr& out, rust::Fn<Quest_Complex(rust::Slice<const qindex>)> func, rust::Slice<const int> numQubitsPerVar, int numVars, int areSigned);

    void reportCompMatr1(CompMatr1 const& matrix);
    void reportCompMatr2(CompMatr2 const& matrix);
    void reportCompMatr(CompMatr const& matrix);

    void reportDiagMatr1(DiagMatr1 const& matrix);
    void reportDiagMatr2(DiagMatr2 const& matrix);
    void reportDiagMatr(DiagMatr const& matrix);
    void reportFullStateDiagMatr(FullStateDiagMatr const& matr);

}
