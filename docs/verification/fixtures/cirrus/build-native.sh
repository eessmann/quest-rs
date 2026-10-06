#!/bin/bash -l
# Run in a Slurm batch allocation. Configuration is supplied by the submitter.
set -euo pipefail
: "${QUEST_CAMPAIGN_ROOT:?set an EPCCFS campaign directory}"
: "${QUEST_NATIVE_SOURCE:?set the unchanged QuEST source directory}"
: "${QUEST_NATIVE_REVISION:?set the expected QuEST revision}"
: "${SLURM_CPUS_PER_TASK:?run inside the build allocation}"
test "$(git -C "$QUEST_NATIVE_SOURCE" rev-parse HEAD)" = "$QUEST_NATIVE_REVISION"
test -z "$(git -C "$QUEST_NATIVE_SOURCE" status --porcelain --untracked-files=no)"
for quest_compiler in gnu cray; do
    module restore
    if [[ "$quest_compiler" = gnu ]]; then
        module switch PrgEnv-cray PrgEnv-gnu
    fi
    module load cmake
    if [[ "$quest_compiler" = cray ]]; then
        # QuEST does not need LibSci; its threaded startup prevents CCE OpenMP
        # entry from a later application thread. Keep QuEST OpenMP enabled.
        module unload cray-libsci
    fi
    export CC=cc CXX=CC
    quest_build="$QUEST_CAMPAIGN_ROOT/native-build/$quest_compiler"
    quest_prefix="$QUEST_CAMPAIGN_ROOT/native/$quest_compiler"
    mkdir -p "$quest_build" "$quest_prefix"
    module -t list > "$quest_build/modules.txt" 2>&1
    cmake -S "$QUEST_NATIVE_SOURCE" -B "$quest_build" \
        -DCMAKE_C_COMPILER=cc -DCMAKE_CXX_COMPILER=CC \
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$quest_prefix" \
        -DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON \
        -DQUEST_ENABLE_MPI=ON -DQUEST_ENABLE_SUBCOMM=ON \
        -DQUEST_ENABLE_OMP=ON -DQUEST_ENABLE_NUMA=ON \
        -DQUEST_ENABLE_CUDA=OFF -DQUEST_ENABLE_CUQUANTUM=OFF \
        -DQUEST_ENABLE_HIP=OFF -DQUEST_BUILD_TESTS=OFF \
        -DQUEST_BUILD_EXAMPLES=OFF -DQUEST_BUILD_MIN_EXAMPLE=ON \
        -DQUEST_ENABLE_INSTALL=ON -DQUEST_ENABLE_PACKAGING=OFF
    cmake --build "$quest_build" --parallel "$SLURM_CPUS_PER_TASK"
    cmake --install "$quest_build"
    printf '%s\n' "$QUEST_NATIVE_REVISION" > "$quest_prefix/source-revision.txt"
done
