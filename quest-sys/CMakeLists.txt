cmake_minimum_required(VERSION 3.15)
project(quest-sys LANGUAGES C CXX)

find_package(QuEST CONFIG REQUIRED)

file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/quest_dummy.cpp" "int main(){return 0;}")

add_executable(quest_dummy EXCLUDE_FROM_ALL
    "${CMAKE_CURRENT_BINARY_DIR}/quest_dummy.cpp"
)

# Link it against QuEST to confirm everything resolves
target_link_libraries(quest_dummy PRIVATE QuEST::QuEST)

# If QuEST provides an imported target named QuEST::QuEST, we can query it:
get_target_property(QUEST_INCLUDE_DIRS       QuEST::QuEST INTERFACE_INCLUDE_DIRECTORIES)
get_target_property(QUEST_LINK_LIBS          QuEST::QuEST INTERFACE_LINK_LIBRARIES)
get_target_property(QUEST_COMPILE_DEFINITIONS QuEST::QuEST INTERFACE_COMPILE_DEFINITIONS)

# Convert the lists to semicolon-separated strings (for easy writing into a file).
string(JOIN ";" QUEST_INCLUDE_DIRS_JOINED         ${QUEST_INCLUDE_DIRS})
string(JOIN ";" QUEST_LINK_LIBS_JOINED            ${QUEST_LINK_LIBS})
string(JOIN ";" QUEST_COMPILE_DEFINITIONS_JOINED  ${QUEST_COMPILE_DEFINITIONS})

configure_file(
  "${CMAKE_CURRENT_SOURCE_DIR}/quest_export.cmake.in"
  "${CMAKE_CURRENT_BINARY_DIR}/quest_export.cmake"
  @ONLY
)
