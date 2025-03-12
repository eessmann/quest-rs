//
// Created by Erich Essmann on 12/03/2025.
//
#include "bindings.h"
#include "helper.h"

namespace quest_sys {
    std::unique_ptr<KrausMap> createKrausMap(int numQubits, int numOperators) {
        return std::make_unique<KrausMap>(::createKrausMap(numQubits, numOperators));
    }
    void syncKrausMap(KrausMap& map) {
        ::syncKrausMap(map);
    }
    void destroyKrausMap(KrausMap& map) {
        ::destroyKrausMap(map);
    }
    void reportKrausMap(KrausMap const& map) {
        ::reportKrausMap(map);
    }

    std::unique_ptr<SuperOp> createSuperOp(int numQubits) {
        return std::make_unique<SuperOp>(::createSuperOp(numQubits));
    }
    void syncSuperOp(SuperOp& op) {
        ::syncSuperOp(op);
    }
    void destroySuperOp(SuperOp& op) {
        ::destroySuperOp(op);
    }
    void reportSuperOp(SuperOp const& op) {
        ::reportSuperOp(op);
    }

    void setKrausMap(KrausMap& map, std::vector<std::vector<std::vector<Quest_Complex>>>& matrices) {
        auto mat = quest_helper::transform_deep_eager(matrices, [](Quest_Complex val) {return qcomp(val);});
        ::setKrausMap(map, std::move(mat));
    }
    void setSuperOp(SuperOp& op, std::vector<std::vector<Quest_Complex>>& matrix) {
        auto mat = quest_helper::transform_deep_eager(matrix, [](Quest_Complex val) {return qcomp(val);});
        ::setSuperOp(op, std::move(mat));
    }

    std::unique_ptr<KrausMap> createInlineKrausMap(int numQubits, int numOperators, std::vector<std::vector<std::vector<Quest_Complex>>>& matrices) {
        auto mat = quest_helper::transform_deep_eager(matrices, [](Quest_Complex val) {return qcomp(val);});
        return std::make_unique<KrausMap>(::createInlineKrausMap(numQubits, numOperators, std::move(mat)));
    }
    std::unique_ptr<SuperOp> createInlineSuperOp(int numQubits, std::vector<std::vector<Quest_Complex>>& matrix) {
        auto mat = quest_helper::transform_deep_eager(matrix, [](Quest_Complex val){return qcomp(val);});
        return std::make_unique<SuperOp>(::createInlineSuperOp(numQubits, std::move(mat)));
    }
}
