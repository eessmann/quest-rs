//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <rust/cxx.h>

#include "types.hpp"

namespace quest_sys {
// Debug
void setSeeds(rust::Slice<const unsigned> seeds);

void setSeedsToDefault();

rust::Vec<unsigned> getSeeds();

void invalidQuESTInputError(rust::String msg, rust::String func);

void setValidationOn();

void setValidationOff();

void setValidationEpsilonToDefault();

void setValidationEpsilon(Quest_Real eps);

Quest_Real getValidationEpsilon();

void setMaxNumReportedItems(Quest_Index numRows, Quest_Index numCols);

void setMaxNumReportedSigFigs(int numSigFigs);

Quest_Index getGpuCacheSize();

void clearGpuCache();

rust::String getEnvironmentString();
}  // namespace quest_sys
