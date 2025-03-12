add_library(cxx_wrapper)
target_link_libraries(cxx_wrapper PUBLIC QuEST::QuEST)

target_sources(
        cxx_wrapper
        PUBLIC
        FILE_SET headers TYPE HEADERS
        FILES
        bindings.h
        helper.h
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
)