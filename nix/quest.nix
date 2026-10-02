{ lib, stdenv, cmake, ninja, llvmPackages, mpich, mpi ? mpich, src }:
stdenv.mkDerivation {
  pname = "quest";
  version = "4.3.0";
  inherit src;
  nativeBuildInputs = [ cmake ninja ];
  buildInputs = lib.optionals stdenv.hostPlatform.isDarwin [ llvmPackages.openmp ]
    ++ lib.optionals stdenv.hostPlatform.isLinux [ mpi ];
  cmakeBuildType = "Release";
  cmakeFlags = [
    "-DBUILD_SHARED_LIBS=ON"
    "-DQUEST_FLOAT_PRECISION=2"
    "-DQUEST_ENABLE_DEPRECATED_API=OFF"
    "-DQUEST_ENABLE_OMP=ON"
    "-DQUEST_ENABLE_CUDA=OFF"
    "-DQUEST_ENABLE_HIP=OFF"
    "-DQUEST_ENABLE_CUQUANTUM=OFF"
    "-DQUEST_ENABLE_ADIOS2=OFF"
    "-DQUEST_DOWNLOAD_ADIOS2=OFF"
    "-DQUEST_ENABLE_NUMA=OFF"
    "-DQUEST_ENABLE_BMI2=OFF"
    "-DQUEST_ENABLE_INSTALL=ON"
    "-DQUEST_ENABLE_PACKAGING=OFF"
    "-DQUEST_BUILD_MIN_EXAMPLE=OFF"
    "-DQUEST_BUILD_EXAMPLES=OFF"
    "-DQUEST_BUILD_TESTS=OFF"
    "-DQUEST_TESTS_DOWNLOAD_CATCH2=OFF"
    "-DCMAKE_INSTALL_LIBDIR=lib"
    "-DCMAKE_INSTALL_RPATH_USE_LINK_PATH=ON"
  ] ++ (if stdenv.hostPlatform.isLinux then [
    "-DQUEST_ENABLE_MPI=ON"
    "-DQUEST_ENABLE_SUBCOMM=ON"
    "-DMPI_C_COMPILER=${lib.getDev mpi}/bin/mpicc"
    "-DMPI_CXX_COMPILER=${lib.getDev mpi}/bin/mpicxx"
    # MPI's library directory is implicit in the native build; link-path
    # inference alone does not retain it in QuEST's installed RUNPATH.
    "-DCMAKE_INSTALL_RPATH=${lib.getLib mpi}/lib"
  ] else [ "-DQUEST_ENABLE_MPI=OFF" ]);
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    mkdir -p installed-consumer
    cat > installed-consumer/CMakeLists.txt <<'CMAKE'
    cmake_minimum_required(VERSION 3.28)
    project(quest_installed_consumer LANGUAGES CXX)
    find_package(QuEST 4.3 REQUIRED CONFIG)
    add_executable(consumer main.cpp)
    target_link_libraries(consumer PRIVATE QuEST::QuEST)
    # Load QuEST independently: linking MPI into consumer can mask an absent
    # QuEST RUNPATH for those indirect dependencies.
    add_executable(library_loader library_loader.cpp)
    target_link_libraries(library_loader PRIVATE ''${CMAKE_DL_LIBS})
    CMAKE
    cat > installed-consumer/main.cpp <<'CPP'
    #include <quest.h>
    #include <cmath>
    int main() {
      initCustomQuESTEnv(0, 0, 0);
      Qureg reg = createQureg(1);
      initZeroState(reg);
      qcomp amplitude = getQuregAmp(reg, 0);
      bool valid = std::abs(amplitude - qcomp(1, 0)) < 1e-12;
      destroyQureg(reg);
      finalizeQuESTEnv();
      return valid ? 0 : 1;
    }
    CPP
    cat > installed-consumer/library_loader.cpp <<'CPP'
    #include <dlfcn.h>
    #include <cstdio>
    int main(int argc, char** argv) {
      if (argc != 2) return 2;
      void* library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
      if (!library) {
        std::fprintf(stderr, "%s\n", dlerror());
        return 1;
      }
      return dlclose(library) == 0 ? 0 : 1;
    }
    CPP
    cmake -S installed-consumer -B installed-consumer/build -G Ninja \
      -DQuEST_DIR="$out/lib/cmake/QuEST"
    cmake --build installed-consumer/build
    env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT \
      -u DYLD_LIBRARY_PATH -u DYLD_FALLBACK_LIBRARY_PATH -u DYLD_INSERT_LIBRARIES \
      installed-consumer/build/library_loader "$out/lib/libQuEST${stdenv.hostPlatform.extensions.sharedLibrary}"
    env -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT \
      -u DYLD_LIBRARY_PATH -u DYLD_FALLBACK_LIBRARY_PATH -u DYLD_INSERT_LIBRARIES \
      installed-consumer/build/consumer
    runHook postInstallCheck
  '';
  meta = {
    description = "QuEST quantum simulator with installed CMake package support";
    homepage = "https://github.com/eessmann/QuEST/tree/cmake-packaging";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
