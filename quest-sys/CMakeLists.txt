cmake_minimum_required(VERSION 3.31)
project(quest-sys LANGUAGES C CXX)

# Export compile commands for IDE integration
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Apply feature flags from Cargo build.rs
option(ENABLE_MULTITHREADING "Enable OpenMP multithreading" OFF)
option(ENABLE_DISTRIBUTION "Enable MPI distribution" OFF)
option(ENABLE_CUDA "Enable CUDA GPU acceleration" OFF)
option(ENABLE_CUQUANTUM "Enable NVIDIA cuQuantum library" OFF)
option(ENABLE_HIP "Enable AMD HIP GPU acceleration" OFF)

# Try to find QuEST using various methods
message(STATUS "Searching for QuEST library...")

# First try: CMake's find_package with CONFIG mode
find_package(QuEST CONFIG QUIET)

# If not found with CONFIG mode, try MODULE mode
if(NOT QuEST_FOUND)
    find_package(QuEST MODULE QUIET)
endif()

# Second try: pkg-config as fallback
if(NOT QuEST_FOUND)
    find_package(PkgConfig QUIET)
    if(PkgConfig_FOUND)
        pkg_check_modules(QuEST QUIET QuEST)
        if(QuEST_FOUND)
            message(STATUS "Found QuEST using pkg-config")
        endif()
    endif()
endif()

# Third try: Manual search in common locations
if(NOT QuEST_FOUND)
    # Try to find the library in standard locations
    find_library(QUEST_LIBRARY
        NAMES QuEST libQuEST
        PATHS
            /usr/local/lib
            /usr/lib
            /usr/local/lib64
            /usr/lib64
            $ENV{QUEST_DIR}/lib
            $ENV{QUEST_ROOT}/lib
            $ENV{QUEST_DIR}/lib64
            $ENV{QUEST_ROOT}/lib64
    )

    find_path(QUEST_INCLUDE_DIR
        NAMES quest/include/quest.h
        PATHS
            /usr/local/include
            /usr/include
            $ENV{QUEST_DIR}/include
            $ENV{QUEST_ROOT}/include
    )

    if(QUEST_LIBRARY AND QUEST_INCLUDE_DIR)
        set(QuEST_FOUND TRUE)
        set(QuEST_LIBRARIES ${QUEST_LIBRARY})
        set(QuEST_INCLUDE_DIRS ${QUEST_INCLUDE_DIR})
        message(STATUS "Found QuEST using manual search: ${QUEST_LIBRARY}")
    endif()
endif()

# Fourth try: Look for QuEST source and build it
if(NOT QuEST_FOUND AND EXISTS "${CMAKE_SOURCE_DIR}/QuEST/CMakeLists.txt")
    message(STATUS "QuEST source found in ${CMAKE_SOURCE_DIR}/QuEST, will build it")

    # Configure QuEST build options
    set(FLOAT_PRECISION 2 CACHE STRING "QuEST precision" FORCE)
    set(MULTITHREADED ${ENABLE_MULTITHREADING} CACHE BOOL "Enable QuEST multithreading" FORCE)
    set(DISTRIBUTED ${ENABLE_DISTRIBUTION} CACHE BOOL "Enable QuEST distribution" FORCE)
    set(GPUACCELERATED ${ENABLE_CUDA} CACHE BOOL "Enable QuEST GPU acceleration" FORCE)

    # Add QuEST as a subdirectory
    add_subdirectory(QuEST)

    # Set up variables to match other find methods
    set(QuEST_FOUND TRUE)
    set(QuEST_LIBRARIES QuEST)
    set(QuEST_INCLUDE_DIRS "${CMAKE_SOURCE_DIR}/QuEST/include")
endif()

# Print final status message
if(QuEST_FOUND)
    message(STATUS "Found QuEST: ${QuEST_LIBRARIES}")
    message(STATUS "QuEST include dirs: ${QuEST_INCLUDE_DIRS}")
else()
    message(WARNING "QuEST not found. Please ensure QuEST is installed or set QUEST_DIR/QUEST_ROOT to point to the QuEST installation directory.")
endif()

# Create a dummy executable that links against QuEST
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/quest_dummy.cpp" "
#include <quest/include/quest.h>
int main() {
    initQuESTEnv();
    finalizeQuESTEnv();
    return 0;
}
")

add_executable(quest_dummy "${CMAKE_CURRENT_BINARY_DIR}/quest_dummy.cpp")

# Configure the dummy executable based on how QuEST was found
if(QuEST_FOUND)
    target_link_libraries(quest_dummy PUBLIC QuEST::QuEST)


    # Add any system-specific QuEST dependencies
    if(ENABLE_MULTITHREADING)
        find_package(OpenMP REQUIRED)
        if(OpenMP_FOUND)
            target_compile_definitions(quest_dummy PRIVATE COMPILE_OPENMP=1)
            target_link_libraries(quest_dummy PRIVATE OpenMP::OpenMP_CXX)
        endif()
    else()
        target_compile_definitions(quest_dummy PRIVATE COMPILE_OPENMP=0)
    endif()

    if(ENABLE_DISTRIBUTION)
        find_package(MPI REQUIRED)
        if(MPI_FOUND)
            target_compile_definitions(quest_dummy PRIVATE COMPILE_MPI=1)
            target_link_libraries(quest_dummy PRIVATE MPI::MPI_CXX)
        endif()
    else()
        target_compile_definitions(quest_dummy PRIVATE COMPILE_MPI=0)
    endif()

    if(ENABLE_CUDA OR ENABLE_HIP)
        target_compile_definitions(quest_dummy PRIVATE COMPILE_CUDA=1)
    else()
        target_compile_definitions(quest_dummy PRIVATE COMPILE_CUDA=0)
    endif()

    if(ENABLE_CUQUANTUM)
        target_compile_definitions(quest_dummy PRIVATE COMPILE_CUQUANTUM=1)
    else()
        target_compile_definitions(quest_dummy PRIVATE COMPILE_CUQUANTUM=0)
    endif()
else()
    # If QuEST is not found, we'll still create the target but the build
    # will fail with clear errors about missing QuEST
    message(WARNING "quest_dummy target will be created but may fail to build due to missing QuEST")
endif()