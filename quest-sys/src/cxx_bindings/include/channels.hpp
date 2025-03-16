//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <rust/cxx.h>
#include <memory>

#include "types.hpp"

namespace quest_sys {
// Channel
std::unique_ptr<KrausMap> createKrausMap(int numQubits, int numOperators);

void syncKrausMap(KrausMap& map);

void destroyKrausMap(KrausMap& map);

void reportKrausMap(const KrausMap& map);

std::unique_ptr<SuperOp> createSuperOp(int numQubits);

void syncSuperOp(SuperOp& op);

void destroySuperOp(SuperOp& op);

void reportSuperOp(const SuperOp& op);

void setKrausMap(
    KrausMap& map,
    rust::Slice<const rust::Slice<const rust::Slice<const Quest_Complex>>>
        matrices);

void setSuperOp(SuperOp& op,
                rust::Slice<const rust::Slice<const Quest_Complex>> matrix);

std::unique_ptr<KrausMap> createInlineKrausMap(
    int numQubits,
    int numOperators,
    rust::Slice<const rust::Slice<const rust::Slice<const Quest_Complex>>>
        matrices);

std::unique_ptr<SuperOp> createInlineSuperOp(
    int numQubits,
    rust::Slice<const rust::Slice<const Quest_Complex>> matrix);
}  // namespace quest_sys