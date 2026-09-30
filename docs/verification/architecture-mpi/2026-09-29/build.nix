let
  nixpkgs = builtins.getFlake "github:NixOS/nixpkgs/6774f7bc253789b113a4f39285dc0fa100abeacc";
  pkgs = import nixpkgs { system = "aarch64-darwin"; };
  src = builtins.fetchTree { type = "github"; owner = "eessmann"; repo = "QuEST"; rev = "503552065045eaf89baba85e6cd6aad728525554"; narHash = "sha256-q2ZvDK57q5kjOxOYFivUexH8ARN8Q5zCf5HVUG+FwSY="; };
  base = pkgs.callPackage ../../../../nix/quest.nix { stdenv = pkgs.llvmPackages.stdenv; inherit src; };
in {
  quest = base.overrideAttrs (old: {
    pname = "quest-mpi-validation";
    buildInputs = old.buildInputs ++ [ pkgs.openmpi ];
    cmakeFlags = builtins.filter (flag: flag != "-DQUEST_ENABLE_MPI=OFF") old.cmakeFlags ++ [ "-DQUEST_ENABLE_MPI=ON" "-DQUEST_ENABLE_SUBCOMM=ON" ];
  });
  mpi = pkgs.openmpi;
}
