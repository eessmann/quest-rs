# Linux MPI validation

Isolated source: `/home/erich/validation/quest-rs-architecture-20260930-35ae2cd5c86d/source`.
Pinned Nix expression is `build.nix`; this is an x86_64-linux build using `pkgs.stdenv`, OpenMP, MPI and SUBCOMM, without CUDA/HIP/cuQuantum. The installed-consumer check is inherited from the repository package and executes during Nix installation checking.

Reproduce from this directory: `bash build.sh`, then `bash run.sh`. `run.sh` enters the source project's devenv, then `run-inside.sh` selects the MPI-enabled QuEST installation and matching explicit MPICC. Cargo uses `../target-mpi` and three jobs. The test harness launches actual OpenMPI ranks. There is no Darwin launcher shim, synthetic topology, or host profile modification.

`build.exit`, `launcher.exit`, `collective.exit` and `lint.exit` are individual command statuses; `run.exit` only records the outer driver and must not substitute for those statuses. Exact commands are in `commands.log`. The run deliberately continues from test failure to lint so independent evidence is retained. `inventory.log` captures tool versions, relevant environment variables, original snapshot manifest digest and MPI-critical source file hashes. `abi-witnesses.log` captures compiled QuEST and MPICC witnesses independently; the build also rejects an ABI mismatch before publication.

The containing source manifest establishes initial source identity; any subsequent coordinated portability fixes are recorded by the parent validation receipts and their final manifest. Only relevant MPI source hashes should be interpreted as the final MPI execution snapshot.

## Result (2026-09-30 Europe/London)

Final normal project environment: pinned QuEST MPI/SUBCOMM build and installed-consumer check passed; two-rank OpenMPI launcher passed; all seven collective runtime tests passed (11.25 s); strict native MPI all-target Clippy passed. The independently compiled native and MPICC witnesses exited successfully and matched byte-for-byte (see `abi-comparison.log`). No synthetic topology, compiler override, or loader-path override was used for this final run.

Initial Linux validation exposed a project-shell compiler selection issue: evaluated `env.CXX` selected the GCC wrapper, while dependency setup hooks replaced the runtime variable with bare `clang++`. The unwrapped compiler's native witness lacked the C++ runtime RUNPATH. `initial-unwrapped-compiler/` retains this failed test/lint evidence. An explicit configured GCC wrapper diagnostic passed (`configured-gcc-diagnostic/`). Removing the duplicate Clang package alone did not fix the shell (`remove-clang-only/`). The final project-owned `enterShell` selection restores the selected native compiler after dependency hooks; `inventory.log` and `final-source.sha256` bind this run to that configuration. No quest-build or MPI runtime source patch was required.
