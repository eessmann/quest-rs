//
// Created by Erich Essmann on 12/03/2025.
//
#include "channels.hpp"
#include "helper.hpp"

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

void reportKrausMap(const KrausMap& map) {
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

void reportSuperOp(const SuperOp& op) {
  ::reportSuperOp(op);
}

void setKrausMap(
    KrausMap& map,
    rust::Slice<const rust::Slice<const rust::Slice<const Quest_Complex>>>
        matrices) {
  auto tensor = quest_helper::slice_to_vector(matrices);
  ::setKrausMap(map, std::move(tensor));
}

void setSuperOp(SuperOp& op,
                rust::Slice<const rust::Slice<const Quest_Complex>> matrix) {
  auto mat = quest_helper::slice_to_vector(matrix);
  ::setSuperOp(op, std::move(mat));
}

std::unique_ptr<KrausMap> createInlineKrausMap(
    int numQubits,
    int numOperators,
    rust::Slice<const rust::Slice<const rust::Slice<const Quest_Complex>>>
        matrices) {
  auto mat = quest_helper::slice_to_vector(matrices);
  return std::make_unique<KrausMap>(
      ::createInlineKrausMap(numQubits, numOperators, std::move(mat)));
}

std::unique_ptr<SuperOp> createInlineSuperOp(
    int numQubits,
    rust::Slice<const rust::Slice<const Quest_Complex>> matrix) {
  auto mat = quest_helper::slice_to_vector(matrix);
  return std::make_unique<SuperOp>(
      ::createInlineSuperOp(numQubits, std::move(mat)));
}
}  // namespace quest_sys
