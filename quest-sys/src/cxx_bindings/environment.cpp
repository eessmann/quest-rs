//
// Created by Erich Essmann on 12/03/2025.
//
#include "environment.hpp"

namespace quest_sys {
void initQuESTEnv() {
  ::initQuESTEnv();
}

void initCustomQuESTEnv(bool useDistrib,
                        bool useGpuAccel,
                        bool useMultithread) {
  ::initCustomQuESTEnv(useDistrib, useGpuAccel, useMultithread);
}

void finalizeQuESTEnv() {
  ::finalizeQuESTEnv();
}

void syncQuESTEnv() {
  ::syncQuESTEnv();
}

void reportQuESTEnv() {
  ::reportQuESTEnv();
}

bool isQuESTEnvInit() {
  return ::isQuESTEnvInit();
}

std::unique_ptr<QuESTEnv> getQuESTEnv() {
  return std::make_unique<QuESTEnv>(::getQuESTEnv());
}
}  // namespace quest_sys
