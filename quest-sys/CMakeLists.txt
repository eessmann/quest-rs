cmake_minimum_required(VERSION 3.15)
project(quest-sys LANGUAGES C CXX)

# Export compile commands for IDE integration
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Apply feature flags from Cargo build.rs
option(ENABLE_MULTITHREADING "Enable OpenMP multithreading" OFF)
option(ENABLE_DISTRIBUTION "Enable MPI distribution" OFF)
option(ENABLE_CUDA "Enable CUDA GPU acceleration" OFF)
option(ENABLE_CUQUANTUM "Enable NVIDIA cuQuantum library" OFF)
option(ENABLE_HIP "Enable AMD HIP GPU acceleration" OFF)

# Find QuEST
message(STATUS "Searching for QuEST library...")
find_package(QuEST CONFIG QUIET)

if(NOT QuEST_FOUND)
    # Try with pkg-config as fallback
    find_package(PkgConfig QUIET)
    if(PkgConfig_FOUND)
        pkg_check_modules(QuEST QUIET QuEST)
    endif()
endif()

if(NOT QuEST_FOUND)
    message(WARNING "QuEST not found. Please ensure QuEST is installed or set QUEST_DIR/QUEST_ROOT to point to the QuEST installation directory.")
endif()

# Create a dummy executable to link against QuEST
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/quest_dummy.cpp" "int main() { return 0; }")
add_executable(quest_dummy "${CMAKE_CURRENT_BINARY_DIR}/quest_dummy.cpp")

if(QuEST_FOUND)
    if(TARGET QuEST::QuEST)
        # Modern CMake with imported target
        message(STATUS "Found QuEST with imported target")
        target_link_libraries(quest_dummy PRIVATE QuEST::QuEST)

        # Extract properties from the imported target
        get_target_property(QUEST_INCLUDE_DIRS      QuEST::QuEST INTERFACE_INCLUDE_DIRECTORIES)
        get_target_property(QUEST_LINK_LIBS         QuEST::QuEST INTERFACE_LINK_LIBRARIES)
        get_target_property(QUEST_COMPILE_DEFINITIONS QuEST::QuEST INTERFACE_COMPILE_DEFINITIONS)
        get_target_property(QUEST_LINK_OPTIONS      QuEST::QuEST INTERFACE_LINK_OPTIONS)

        # If any properties are not found, initialize them as empty lists
        if(NOT QUEST_INCLUDE_DIRS)
            set(QUEST_INCLUDE_DIRS "")
        endif()
        if(NOT QUEST_LINK_LIBS)
            set(QUEST_LINK_LIBS "")
        endif()
        if(NOT QUEST_COMPILE_DEFINITIONS)
            set(QUEST_COMPILE_DEFINITIONS "")
        endif()
        if(QUEST_LINK_OPTIONS)
            # Append link options to link libs
            list(APPEND QUEST_LINK_LIBS ${QUEST_LINK_OPTIONS})
        endif()

    else()
        # Older style with variables
        message(STATUS "Found QuEST with variables")
        target_include_directories(quest_dummy PRIVATE ${QuEST_INCLUDE_DIRS})
        target_link_libraries(quest_dummy PRIVATE ${QuEST_LIBRARIES})
        target_compile_definitions(quest_dummy PRIVATE ${QuEST_CFLAGS_OTHER})

        set(QUEST_INCLUDE_DIRS ${QuEST_INCLUDE_DIRS})
        set(QUEST_LINK_LIBS ${QuEST_LIBRARIES} ${QuEST_LDFLAGS})
        set(QUEST_COMPILE_DEFINITIONS ${QuEST_CFLAGS_OTHER})
    endif()

    # Add any system-specific QuEST dependencies
    if(ENABLE_MULTITHREADING)
        find_package(OpenMP REQUIRED)
        if(OpenMP_FOUND)
            list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_OPENMP=1")
            list(APPEND QUEST_LINK_LIBS ${OpenMP_CXX_LIBRARIES})
        endif()
    else()
        list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_OPENMP=0")
    endif()

    if(ENABLE_DISTRIBUTION)
        find_package(MPI REQUIRED)
        if(MPI_FOUND)
            list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_MPI=1")
            list(APPEND QUEST_LINK_LIBS ${MPI_CXX_LIBRARIES})
            list(APPEND QUEST_INCLUDE_DIRS ${MPI_CXX_INCLUDE_DIRS})
        endif()
    else()
        list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_MPI=0")
    endif()

    if(ENABLE_CUDA OR ENABLE_HIP)
        list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_CUDA=1")
    else()
        list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_CUDA=0")
    endif()

    if(ENABLE_CUQUANTUM)
        list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_CUQUANTUM=1")
    else()
        list(APPEND QUEST_COMPILE_DEFINITIONS "COMPILE_CUQUANTUM=0")
    endif()

else()
    # Set empty variables if QuEST is not found
    set(QUEST_INCLUDE_DIRS "")
    set(QUEST_LINK_LIBS "")
    set(QUEST_COMPILE_DEFINITIONS "")
endif()

# For Windows compatibility, we need to ensure backslashes are properly handled
if(WIN32)
    # On Windows, normalize paths and replace backslashes with forward slashes
    foreach(DIR ${QUEST_INCLUDE_DIRS})
        file(TO_CMAKE_PATH "${DIR}" NORMALIZED_DIR)
        list(APPEND QUEST_INCLUDE_DIRS_NORMALIZED "${NORMALIZED_DIR}")
    endforeach()

    foreach(LIB ${QUEST_LINK_LIBS})
        file(TO_CMAKE_PATH "${LIB}" NORMALIZED_LIB)
        list(APPEND QUEST_LINK_LIBS_NORMALIZED "${NORMALIZED_LIB}")
    endforeach()

    # Use the normalized paths
    set(QUEST_INCLUDE_DIRS "${QUEST_INCLUDE_DIRS_NORMALIZED}")
    set(QUEST_LINK_LIBS "${QUEST_LINK_LIBS_NORMALIZED}")
endif()

# Clean up any trailing backslashes in compile definitions
# that can cause issues in Windows command prompt
set(CLEAN_COMPILE_DEFINITIONS "")
foreach(DEF ${QUEST_COMPILE_DEFINITIONS})
    string(REGEX REPLACE "\\\\+$" "" CLEAN_DEF "${DEF}")
    list(APPEND CLEAN_COMPILE_DEFINITIONS "${CLEAN_DEF}")
endforeach()
set(QUEST_COMPILE_DEFINITIONS "${CLEAN_COMPILE_DEFINITIONS}")

# Convert the lists to semicolon-separated strings for easy writing to a file
string(REPLACE ";" "\\;" QUEST_INCLUDE_DIRS_JOINED "${QUEST_INCLUDE_DIRS}")
string(REPLACE ";" "\\;" QUEST_LINK_LIBS_JOINED "${QUEST_LINK_LIBS}")
string(REPLACE ";" "\\;" QUEST_COMPILE_DEFINITIONS_JOINED "${QUEST_COMPILE_DEFINITIONS}")

# Write out the configuration for the Rust build script
configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/quest_export.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/quest_export.cmake"
    @ONLY
)