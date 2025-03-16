add_library(cxx_wrapper)
target_link_libraries(cxx_wrapper PUBLIC QuEST::QuEST)

target_sources(
        cxx_wrapper
        PUBLIC
        FILE_SET headers TYPE HEADERS
        BASE_DIRS include
        FILES
        include/calculations.hpp
        include/channels.hpp
        include/debug.hpp
        include/decoherence.hpp
        include/environment.hpp
        include/helper.hpp
        include/initialisation.hpp
        include/matrices.hpp
        include/operations.hpp
        include/qureg.hpp
        include/types.hpp
)

target_sources(
        cxx_wrapper
        PRIVATE
        calculations.cpp
        channel.cpp
        debug.cpp
        decoherence.cpp
        environment.cpp
        initialisation.cpp
        matrices.cpp
        operations.cpp
        qureg.cpp
)
target_include_directories(
        cxx_wrapper
        PRIVATE
        "${CMAKE_CURRENT_SOURCE_DIR}/../../../target/cxxbridge"
)