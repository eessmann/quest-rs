//
// Created by Erich Essmann on 14/03/2025.
//
#include "bindings.h"
#include "helper.h"

namespace quest_sys {
// Qureg
std::unique_ptr<Qureg> createQureg(int numQubits) {
  return std::make_unique<Qureg>(::createQureg(numQubits));
}

std::unique_ptr<Qureg> createDensityQureg(int numQubits) {
  return std::make_unique<Qureg>(::createDensityQureg(numQubits));
}

std::unique_ptr<Qureg> createForcedQureg(int numQubits) {
  return std::make_unique<Qureg>(::createForcedQureg(numQubits));
}

std::unique_ptr<Qureg> createForcedDensityQureg(int numQubits) {
  return std::make_unique<Qureg>(::createForcedDensityQureg(numQubits));
}

std::unique_ptr<Qureg> createCustomQureg(int numQubits,
                                         int isDensMatr,
                                         int useDistrib,
                                         int useGpuAccel,
                                         int useMultithread) {
  return std::make_unique<Qureg>(::createCustomQureg(
      numQubits, isDensMatr, useDistrib, useGpuAccel, useMultithread));
}

std::unique_ptr<Qureg> createCloneQureg(const Qureg& qureg) {
  return std::make_unique<Qureg>(::createCloneQureg(qureg));
}

void destroyQureg(Qureg& qureg) {
  ::destroyQureg(qureg);
}

void reportQuregParams(const Qureg& qureg) {
  ::reportQuregParams(qureg);
}

void reportQureg(const Qureg& qureg) {
  ::reportQureg(qureg);
}

void syncQuregToGpu(Qureg& qureg) {
  ::syncQuregToGpu(qureg);
}

void syncQuregFromGpu(Qureg& qureg) {
  ::syncQuregFromGpu(qureg);
}

void syncSubQuregToGpu(Qureg& qureg,
                       Quest_Index localStartInd,
                       Quest_Index numLocalAmps) {
  ::syncSubQuregToGpu(qureg, localStartInd, numLocalAmps);
}

void syncSubQuregFromGpu(Qureg& qureg,
                         Quest_Index localStartInd,
                         Quest_Index numLocalAmps) {
  ::syncSubQuregFromGpu(qureg, localStartInd, numLocalAmps);
}

rust::Vec<Quest_Complex> getQuregAmps(Qureg& qureg,
                                      Quest_Index startInd,
                                      Quest_Index numAmps) {
  std::vector<qcomp> amps(numAmps);
  rust::Vec<Quest_Complex> out_amps;
  ::getQuregAmps(amps.data(), qureg, startInd, numAmps);
  auto convert_amps = quest_helper::apply_deep(
      amps, [](qcomp val) { return Quest_Complex(val); });
  for (auto amp : convert_amps) {
    out_amps.emplace_back(amp);
  }
  return out_amps;
}

rust::Vec<Quest_Complex> getDensityQuregAmps(Qureg& qureg,
                                                        Quest_Index startRow,
                                                        Quest_Index startCol,
                                                        Quest_Index numRows,
                                                        Quest_Index numCols) {
  std::vector<std::vector<qcomp>> amps(numRows);
  for (auto& row : amps) {
    row.reserve(numCols);
  }
  std::vector<qcomp*> amp_ptr(numRows);
  ::getDensityQuregAmps(amp_ptr.data(), qureg, startRow, startCol, numRows,
                        numCols);

  auto convert_amps = quest_helper::apply_deep(
      amps, [](qcomp val) { return Quest_Complex(val); });

 rust::Vec<Quest_Complex> out;
  for (auto row : convert_amps) {
    for (auto val : row) {
      out.emplace_back(val);
    }
  }

  return out;
}

Quest_Complex getQuregAmp(Qureg& qureg, Quest_Index index) {
  return ::getQuregAmp(qureg, index);
}

Quest_Complex getDensityQuregAmp(Qureg& qureg,
                                 Quest_Index row,
                                 Quest_Index column) {
  return ::getDensityQuregAmp(qureg, row, column);
}
}  // namespace quest_sys