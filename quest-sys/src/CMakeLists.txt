add_library(cxx_wrapper bindings.h wrapper.cpp)
target_link_libraries(cxx_wrapper PUBLIC QuEST::QuEST)