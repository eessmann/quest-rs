//
// Created by erich on 3/16/25.
//
#pragma once
#include <quest.h>
#include <memory>

namespace quest_sys {
void initQuESTEnv();

void initCustomQuESTEnv(bool useDistrib, bool useGpuAccel, bool useMultithread);

void finalizeQuESTEnv();

void syncQuESTEnv();

void reportQuESTEnv();

bool isQuESTEnvInit();

std::unique_ptr<QuESTEnv> getQuESTEnv();
}  // namespace quest_sys
