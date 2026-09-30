{ pkgs, lib, inputs, ... }:
let
  nativeStdenv = if pkgs.stdenv.hostPlatform.isDarwin then pkgs.llvmPackages.stdenv else pkgs.stdenv;
  quest = pkgs.callPackage ./nix/quest.nix {
    stdenv = nativeStdenv;
    src = inputs.quest-src;
  };
  serialHdf5 = pkgs.hdf5.override { mpiSupport = false; enableShared = true; };
  # Both quest-build and hdf5-metno expect one prefix for headers and libraries.
  hdf5Prefix = pkgs.symlinkJoin {
    name = "quest-serial-hdf5";
    paths = [ (lib.getDev serialHdf5) (lib.getLib serialHdf5) (lib.getBin serialHdf5) ];
  };
in
{
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
    # Editor integration is managed separately from the configured toolchain.
    lsp.enable = false;
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
    pkgs.llvmPackages.libclang
  ] ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.binutils pkgs.glibc.bin ];
  env = {
    QUEST_ROOT = "${quest}";
    HDF5_DIR = "${hdf5Prefix}";
    CC = "${nativeStdenv.cc}/bin/cc";
    CXX = "${nativeStdenv.cc}/bin/c++";
    CLANG = "${pkgs.llvmPackages.clang}/bin/clang";
    LIBCLANG_PATH = "${lib.getLib pkgs.llvmPackages.libclang}/lib";
  };
  # Clang setup hooks reached through bindgen dependencies overwrite CC/CXX
  # after env is loaded. Restore the selected native wrappers after those hooks
  # so CMake executables retain the compiler runtime closure on non-NixOS Linux.
  enterShell = lib.mkAfter ''
    export CC="${nativeStdenv.cc}/bin/cc"
    export CXX="${nativeStdenv.cc}/bin/c++"
  '';
  outputs.quest = quest;
  enterTest = ''
    cargo run --locked -p quest-rs --example minimal
  '';
}
