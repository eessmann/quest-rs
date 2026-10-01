# Linux MPI validation

Isolated source: `/path/to/validation/architecture-synthesis/source`.
Pinned `build.nix` selects x86_64-linux `pkgs.stdenv`, QuEST 4.3.0 with OpenMP, MPI and SUBCOMM, and OpenMPI 5.0.10. CUDA/HIP/cuQuantum are disabled. The repository package's installed-consumer check executes during Nix installation checking.

Reproduce from this directory: `bash build.sh`, `bash run.sh`, then `bash compare-abi.sh`. The runner enters the source project's devenv and selects MPI QuEST plus matching explicit MPICC. Cargo uses `../target-mpi` and three jobs. The tests launch actual OpenMPI ranks. No Darwin launcher shim, synthetic topology, compiler override, loader-path override or host profile modification is used.

Final results on 2026-09-30 Europe/London: pinned native build/install consumer passed; two-rank launcher passed; seven collective runtime tests passed (11.24 s); strict native MPI all-target Clippy passed. Independent native and MPICC ABI witnesses both exited zero and matched byte-for-byte. `build.exit`, `launcher.exit`, `collective.exit` and `lint.exit` record individual statuses; `run.exit` describes only the outer driver. Exact commands, versions, environment and source hashes accompany the logs. The driver continues after individual failures to retain independent evidence.

Initial validation exposed dependency hooks replacing the configured GCC wrapper with bare clang++, whose native witness lacked the C++ runtime RUNPATH. `initial-unwrapped-compiler/` preserves that failure, `configured-gcc-diagnostic/` the successful explicit-compiler diagnostic, and `remove-clang-only/` the insufficient package-removal attempt. `mkafter-environment/` retains the successful intermediate enterShell repair. The final project configuration instead sets `stdenv = nativeStdenv` and disables Rust's automatic Clang linker selection. In the final shell, CXX=g++ resolves to the pinned GCC 15.3.0 wrapper; the build log verifies that exact path. Final `devenv.nix` and MPI/compiler source hashes are in `final-source.sha256`. No quest-build or collective source patch was needed.

The original containing source manifest identifies the exported snapshot; parent validation receipts record subsequent coordinated portability fixes and final overall source identity. Relevant final execution-source hashes are recorded independently here.
