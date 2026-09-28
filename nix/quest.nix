{ lib, stdenv, cmake, ninja, llvmPackages, src }:
stdenv.mkDerivation {
  pname = "quest";
  version = "4.3.0";
  inherit src;
  nativeBuildInputs = [ cmake ninja ];
  buildInputs = lib.optionals stdenv.hostPlatform.isDarwin [ llvmPackages.openmp ];
  cmakeBuildType = "Release";
  cmakeFlags = [
    "-DBUILD_SHARED_LIBS=ON"
    "-DQUEST_FLOAT_PRECISION=2"
    "-DQUEST_ENABLE_DEPRECATED_API=OFF"
    "-DQUEST_ENABLE_OMP=ON"
    "-DQUEST_ENABLE_MPI=OFF"
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
  ];
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
    CMAKE
    cat > installed-consumer/main.cpp <<'CPP'
    #include <quest.h>
    #include <cmath>
    int main() {
      initQuESTEnv();
      Qureg reg = createQureg(1);
      initZeroState(reg);
      qcomp amplitude = getQuregAmp(reg, 0);
      bool valid = std::abs(amplitude - qcomp(1, 0)) < 1e-12;
      destroyQureg(reg);
      finalizeQuESTEnv();
      return valid ? 0 : 1;
    }
    CPP
    cmake -S installed-consumer -B installed-consumer/build -G Ninja \
      -DQuEST_DIR="$out/lib/cmake/QuEST"
    cmake --build installed-consumer/build
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
