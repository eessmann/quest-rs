{ pkgs, lib, inputs, ... }:
let
  nativeStdenv = if pkgs.stdenv.hostPlatform.isDarwin then pkgs.llvmPackages.stdenv else pkgs.stdenv;
  mpi = pkgs.mpich;
  quest = pkgs.callPackage ./nix/quest.nix {
    stdenv = nativeStdenv;
    src = inputs.quest-src;
    inherit mpi;
  };
  serialHdf5 = pkgs.hdf5.override { mpiSupport = false; enableShared = true; };
  # Both quest-build and hdf5-metno expect one prefix for headers and libraries.
  hdf5Prefix = pkgs.symlinkJoin {
    name = "quest-serial-hdf5";
    paths = [ (lib.getDev serialHdf5) (lib.getLib serialHdf5) (lib.getBin serialHdf5) ];
  };
in
{
  stdenv = nativeStdenv;
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
    # Editor integration is managed separately from the configured toolchain.
    lsp.enable = false;
    # Keep Cargo's native linker and C/C++ setup on the selected stdenv.
    clangLinker.enable = false;
  };
  languages.c.enable = false;
  # Native packages are realized through the explicit prefixes below; Cargo and
  # CMake discover their include, library, and runtime paths. Bindgen uses the
  # explicit CLANG/LIBCLANG_PATH dependencies below.
  packages = [
    nativeStdenv.cc
    pkgs.cmake
    pkgs.ninja
    pkgs.pkg-config
    pkgs.cargo-nextest
    pkgs.mdbook
    pkgs.llvmPackages.libclang
  ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.binutils pkgs.glibc.bin mpi ];
  env = {
    QUEST_ROOT = "${quest}";
    HDF5_DIR = "${hdf5Prefix}";
    CC = "${nativeStdenv.cc}/bin/cc";
    CXX = "${nativeStdenv.cc}/bin/c++";
    CLANG = "${pkgs.llvmPackages.clang}/bin/clang";
    LIBCLANG_PATH = "${lib.getLib pkgs.llvmPackages.libclang}/lib";
  } // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    # rsmpi must use the same installation as QuEST's CMake MPI targets.
    MPICC = "${lib.getDev mpi}/bin/mpicc";
  };
  enterShell = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
    # MPI tests invoke mpiexec by name; prefer this installation's launcher.
    export PATH="${lib.getBin mpi}/bin:$PATH"
  '';
  outputs.quest = quest;
  enterTest = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
    cargo build --workspace --all-features --locked
    cargo nextest run --workspace --all-features
    cargo test --doc --workspace --all-features --locked
    cargo nextest run -p quest-qsp --all-features --release --run-ignored only
  '' + ''
    cargo run --locked -p quest-rs --example minimal
  '';
}
