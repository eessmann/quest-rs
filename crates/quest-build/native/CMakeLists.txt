cmake_minimum_required(VERSION 3.24)
project(quest_rust_bridge LANGUAGES CXX)
file(WRITE "${CMAKE_BINARY_DIR}/quest-compiler-arguments.txt" "${CMAKE_CXX_COMPILER_ARG1}\n")
string(REPLACE ";" "\n" quest_dynamic_loader_libraries "${CMAKE_DL_LIBS}")
file(WRITE "${CMAKE_BINARY_DIR}/quest-dynamic-loader-libraries.txt" "${quest_dynamic_loader_libraries}\n")
if(APPLE)
    set(quest_evaluated_sysroot "${CMAKE_OSX_SYSROOT}")
else()
    set(quest_evaluated_sysroot "${CMAKE_SYSROOT}")
endif()
file(WRITE "${CMAKE_BINARY_DIR}/quest-sysroot.txt" "${quest_evaluated_sysroot}\n")
find_package(QuEST 4.3 REQUIRED CONFIG)
if(NOT TARGET QuEST::QuEST)
    message(FATAL_ERROR "The installed package does not export QuEST::QuEST")
endif()
include("${CMAKE_CURRENT_SOURCE_DIR}/bridge-inputs.cmake")
add_library(quest_bridge STATIC abi.cpp ${QUEST_BRIDGE_SOURCES})
target_include_directories(quest_bridge PRIVATE ${QUEST_BRIDGE_INCLUDE_DIRECTORIES})
target_compile_features(quest_bridge PRIVATE cxx_std_20)
if(NOT QUEST_RUST_ARCH MATCHES "^(x86_64|aarch64|powerpc64|powerpc64le|riscv64|s390x|i686|armv7)$")
    message(FATAL_ERROR "No compiled native architecture admission for ${QUEST_RUST_ARCH}")
endif()
target_compile_definitions(quest_bridge PRIVATE
    "QUEST_RUST_ARCH_${QUEST_RUST_ARCH}=1" "QUEST_RUST_PLATFORM_${QUEST_RUST_PLATFORM}=1")
target_link_libraries(quest_bridge PRIVATE QuEST::QuEST ${CMAKE_DL_LIBS})
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(quest_bridge PRIVATE
        -Wall -Wextra -Wpedantic -Wconversion
        -Wno-unused-parameter -Wno-unknown-pragmas)
endif()
set_target_properties(quest_bridge PROPERTIES POSITION_INDEPENDENT_CODE ON)
# An unbuilt executable provides the evaluated link interface in the same
# project, compiler and configuration as the actual bridge archive.
add_executable(quest_link_query EXCLUDE_FROM_ALL query.cpp)
target_compile_features(quest_link_query PRIVATE cxx_std_20)
target_link_libraries(quest_link_query PRIVATE QuEST::QuEST ${CMAKE_DL_LIBS})
# The codemodel omits include directories that the compiler already treats as
# implicit (for example a Nix wrapper). Preserve the imported target's evaluated
# include interface so discovery retains package identity in that environment.
file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/quest-includes-$<CONFIG>.txt"
    CONTENT "$<JOIN:$<TARGET_GENEX_EVAL:QuEST::QuEST,$<TARGET_PROPERTY:QuEST::QuEST,INTERFACE_INCLUDE_DIRECTORIES>>,\n>\n"
    TARGET quest_link_query)
file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/quest-library-$<CONFIG>.txt"
    CONTENT "$<TARGET_FILE:QuEST::QuEST>\n")
# Built only when the Rust rsmpi feature requests an ABI/link-identity witness.
# This uses the exact same imported native target as the production bridge.
set_source_files_properties(mpi_abi.c PROPERTIES LANGUAGE CXX)
add_executable(quest_mpi_abi EXCLUDE_FROM_ALL mpi_abi.c)
target_compile_definitions(quest_mpi_abi PRIVATE QUEST_RSMPI_NATIVE_PROBE=1)
target_link_libraries(quest_mpi_abi PRIVATE QuEST::QuEST ${CMAKE_DL_LIBS})
