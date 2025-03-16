//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <rust/cxx.h>
#include <memory>

#include "types.hpp"

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
                       Quest_Index localStartInd,
                       Quest_Index numLocalAmps);

void syncSubQuregFromGpu(Qureg& qureg,
                         Quest_Index localStartInd,
                         Quest_Index numLocalAmps);

rust::Vec<Quest_Complex> getQuregAmps(Qureg& qureg,
                                      Quest_Index startInd,
                                      Quest_Index numAmps);

rust::Vec<Quest_Complex> getDensityQuregAmps_flatten(Qureg& qureg,
                                                     Quest_Index startRow,
                                                     Quest_Index startCol,
                                                     Quest_Index numRows,
                                                     Quest_Index numCols);

Quest_Complex getQuregAmp(Qureg& qureg, Quest_Index index);

Quest_Complex getDensityQuregAmp(Qureg& qureg,
                                 Quest_Index row,
                                 Quest_Index column);
}  // namespace quest_sys
