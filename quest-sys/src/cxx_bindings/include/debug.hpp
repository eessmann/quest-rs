//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include "types.hpp"
#include <quest-sys/src/lib.rs.h>

namespace quest_sys {
// Debug
void setSeeds(rust::Slice<const unsigned> seeds);

void setSeedsToDefault();

rust::Vec<unsigned> getSeeds();

void setValidationOn();

void setValidationOff();

void setValidationEpsilonToDefault();

void setValidationEpsilon(double eps);

double getValidationEpsilon();

void setMaxNumReportedItems(std::int64_t numRows, std::int64_t numCols);

void setMaxNumReportedSigFigs(int numSigFigs);

void setNumReportedNewlines(int numNewlines);

void setReportedPauliChars(rust::Str paulis);

std::int64_t getGpuCacheSize();

void clearGpuCache();

rust::String getEnvironmentString();
}  // namespace quest_sys
