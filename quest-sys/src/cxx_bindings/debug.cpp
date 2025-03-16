//
// Created by Erich Essmann on 12/03/2025.
//
#include "debug.hpp"
#include "helper.hpp"

namespace quest_sys {
void setSeeds(rust::Slice<const unsigned> seeds) {
  ::setSeeds(quest_helper::slice_to_ptr(seeds),
             static_cast<int>(seeds.length()));
}

void setSeedsToDefault() {
  ::setSeedsToDefault();
}

rust::Vec<unsigned> getSeeds() {
  unsigned* seed_ptr{};
  ::getSeeds(seed_ptr);
  rust::Vec<unsigned> out{};
  if (seed_ptr) {
    for (int i = 0; i < ::getNumSeeds(); ++i) {
      out.emplace_back(seed_ptr[0]);
    }
    return out;
  } else {
    return out;
  }
}

void invalidQuESTInputError(rust::String msg, rust::String func) {
  ::invalidQuESTInputError(msg.c_str(), func.c_str());
}

void setValidationOn() {
  ::setValidationOn();
}

void setValidationOff() {
  ::setValidationOff();
}

void setValidationEpsilonToDefault() {
  ::setValidationEpsilonToDefault();
}

void setValidationEpsilon(Quest_Real eps) {
  ::setValidationEpsilon(eps);
}

Quest_Real getValidationEpsilon() {
  return ::getValidationEpsilon();
}

void setMaxNumReportedItems(Quest_Index numRows, Quest_Index numCols) {
  ::setMaxNumReportedItems(numRows, numCols);
}

void setMaxNumReportedSigFigs(int numSigFigs) {
  ::setMaxNumReportedSigFigs(numSigFigs);
}

Quest_Index getGpuCacheSize() {
  return ::getGpuCacheSize();
}

void clearGpuCache() {
  ::clearGpuCache();
}

rust::String getEnvironmentString() {
  std::array<char, 200> str{};
  ::getEnvironmentString(str.data());
  return {str.data()};
}
}  // namespace quest_sys
