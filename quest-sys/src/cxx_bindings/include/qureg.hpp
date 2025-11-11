//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <memory>

#include "types.hpp"
#include <quest-sys/src/lib.rs.h>

namespace quest_sys {
std::unique_ptr<Qureg> createQureg(int numQubits);

std::unique_ptr<Qureg> createDensityQureg(int numQubits);

std::unique_ptr<Qureg> createForcedQureg(int numQubits);

std::unique_ptr<Qureg> createForcedDensityQureg(int numQubits);

std::unique_ptr<Qureg> createCustomQureg(int numQubits,
                                         int isDensMatr,
                                         int useDistrib,
                                         int useGpuAccel,
                                         int useMultithread);

std::unique_ptr<Qureg> createCloneQureg(const Qureg& qureg);

void destroyQureg(Qureg& qureg);

void reportQuregParams(const Qureg& qureg);

void reportQureg(const Qureg& qureg);

void syncQuregToGpu(Qureg& qureg);

void syncQuregFromGpu(Qureg& qureg);

void syncSubQuregToGpu(Qureg& qureg,
                       std::int64_t localStartInd,
                       std::int64_t numLocalAmps);

void syncSubQuregFromGpu(Qureg& qureg,
                         std::int64_t localStartInd,
                         std::int64_t numLocalAmps);

rust::cxxbridge1::Vec<Quest_Complex> getQuregAmps(Qureg& qureg,
                                      std::int64_t startInd,
                                      std::int64_t numAmps);

rust::cxxbridge1::Vec<Quest_Complex> getDensityQuregAmps_flatten(Qureg& qureg,
                                                     std::int64_t startRow,
                                                     std::int64_t startCol,
                                                     std::int64_t numRows,
                                                     std::int64_t numCols);

Quest_Complex getQuregAmp(Qureg& qureg, std::int64_t index);

Quest_Complex getDensityQuregAmp(Qureg& qureg,
                                 std::int64_t row,
                                 std::int64_t column);
}  // namespace quest_sys
