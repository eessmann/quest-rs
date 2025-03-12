//
// Created by Erich Essmann on 12/03/2025.
//
#include "bindings.h"

namespace quest_sys {
    void setSeeds(rust::Slice<const unsigned> seeds, int numSeeds) {
        ::setSeeds(const_cast<unsigned*>(seeds.data()), numSeeds);
    }
    void setSeedsToDefault() {
        ::setSeedsToDefault();
    }

    std::vector<unsigned> getSeeds() {
        unsigned* seed_ptr;
        ::getSeeds(seed_ptr);
        if (seed_ptr) {
            return {seed_ptr, seed_ptr + ::getNumSeeds()};
        } else {
            return {};
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
    void setValidationEpsilon(qreal eps) {
        ::setValidationEpsilon(eps);
    }
    qreal getValidationEpsilon() {
        return ::getValidationEpsilon();
    }

    void setMaxNumReportedItems(qindex numRows, qindex numCols) {
        ::setMaxNumReportedItems(numRows, numCols);
    }
    void setMaxNumReportedSigFigs(int numSigFigs) {
        ::setMaxNumReportedSigFigs(numSigFigs);
    }

    qindex getGpuCacheSize() {
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
}