//
// Created by Erich Essmann on 12/03/2025.
//
#include "bindings.h"

namespace quest_sys {
    void initQuESTEnv() {
        ::initQuESTEnv();
    }
    void initCustomQuESTEnv(int useDistrib, int useGpuAccel, int useMultithread) {
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

}