#!/usr/bin/bash
set -u
export PATH=/home/erich/.nix-profile/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin
export CARGO_BUILD_JOBS=3
unset CC CXX NIX_CC NIX_CC_FOR_BUILD CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER
cd /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source
trap 'printf "%s\n" "$?" > ../receipts/qsp-compiler-candidate.exit' EXIT
sha256sum devenv.nix > ../receipts/qsp-compiler-candidate.sha256
devenv shell -- /usr/bin/bash -c 'env | sort | grep -E "^(CC=|CXX=|NIX_CC=|NIX_CC_FOR_BUILD=|CARGO_TARGET_.*LINKER=|CLANG=|LIBCLANG_PATH=)"; printf "cc_path="; command -v cc; printf "cxx_path="; command -v c++; "$CC" --version; "$CXX" --version' > ../receipts/qsp-compiler-candidate-env.log 2>&1
probe=$?;printf '%s\n' "$probe" > ../receipts/qsp-compiler-candidate-env.exit
if [ "$probe" -ne 0 ]; then exit "$probe"; fi
devenv shell -- cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp --work-dir /home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/receipts/qsp-compiler-consumers > ../receipts/qsp-compiler-candidate-consumers.log 2>&1
consumer=$?;printf '%s\n' "$consumer" > ../receipts/qsp-compiler-candidate-consumers.exit
exit "$consumer"
