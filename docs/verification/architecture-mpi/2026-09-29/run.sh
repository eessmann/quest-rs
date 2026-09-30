#!/usr/bin/env bash
set -euo pipefail
export QUEST_ROOT=/nix/store/9zfxh0j6dngfdaj972bxfcd9mm44cnjc-quest-mpi-validation-4.3.0
export MPICC=/nix/store/rdalrrvgqig1hz05ma3j8ws07fnc8s6l-openmpi-5.0.10-dev/bin/mpicc
export CARGO_TARGET_DIR=/tmp/quest-architecture-mpi
export CMAKE_PREFIX_PATH=/nix/store/rdalrrvgqig1hz05ma3j8ws07fnc8s6l-openmpi-5.0.10-dev
export PKG_CONFIG_PATH=/nix/store/rdalrrvgqig1hz05ma3j8ws07fnc8s6l-openmpi-5.0.10-dev/lib/pkgconfig
receipt_bin="$(cd -- "$(dirname -- "$0")/bin" && pwd)"
export PATH="$receipt_bin":/nix/store/rdalrrvgqig1hz05ma3j8ws07fnc8s6l-openmpi-5.0.10-dev/bin:/nix/store/4qip1zra36mhxwfqrw9h5imqghwl4wcl-openmpi-5.0.10/bin:$PATH
exec cargo "$@"
