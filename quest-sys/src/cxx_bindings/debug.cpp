//
// Created by Erich Essmann on 12/03/2025.
//
#include <quest.h>

#include "debug.hpp"
#include "helper.hpp"

namespace quest_sys {
void setSeeds(rust::cxxbridge1::Slice<const unsigned> seeds) {
  ::setSeeds(quest_helper::slice_to_ptr(seeds),
             static_cast<int>(seeds.length()));
}

void setSeedsToDefault() {
  ::setSeedsToDefault();
}

rust::cxxbridge1::Vec<unsigned> getSeeds() {
  unsigned* seed_ptr{};
  ::getSeeds(seed_ptr);
  rust::cxxbridge1::Vec<unsigned> out{};
  if (seed_ptr) {
    for (int i = 0; i < ::getNumSeeds(); ++i) {
      out.emplace_back(seed_ptr[0]);
    }
    return out;
  } else {
    return out;
  }
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

void setValidationEpsilon(double eps) {
  ::setValidationEpsilon(eps);
}

double getValidationEpsilon() {
  return ::getValidationEpsilon();
}

void setMaxNumReportedItems(std::int64_t numRows, std::int64_t numCols) {
  ::setMaxNumReportedItems(numRows, numCols);
}

void setMaxNumReportedSigFigs(int numSigFigs) {
  ::setMaxNumReportedSigFigs(numSigFigs);
}

std::int64_t getGpuCacheSize() {
  return ::getGpuCacheSize();
}

void clearGpuCache() {
  ::clearGpuCache();
}

rust::cxxbridge1::String getEnvironmentString() {
  std::array<char, 200> str{};
  ::getEnvironmentString(str.data());
  return {str.data()};
}
}  // namespace quest_sys
