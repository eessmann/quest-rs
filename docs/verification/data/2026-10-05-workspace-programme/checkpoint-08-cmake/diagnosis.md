# Checkpoint 08: CMake module-open failure diagnosis

Read-only source/environment investigation with isolated temporary CMake projects and one authorized five-test reproduction. No production/test/native/system files changed; no scientific jobs or broad suite rerun.

## Conclusion

The default gate's five failures are an **unresolved intermittent CMake own-module open failure**. There is no evidence supporting a quest-build source patch, compiler replacement, package repair, or limit change. Current successful checks do not retrospectively fix or invalidate the failed gate. The exact failing syscall errno and process limits at the failure instant were not captured.

The first diagnostic is `cmListFileCache: error can not open file` for `/usr/share/cmake/Modules/CMakeDetermineCXXCompiler.cmake`; one job additionally cannot open `CMakeCXXInformation.cmake`. C++ not found, missing generated compiler metadata, and unknown `.cpp`/`.cxx` extension messages follow the unsuccessful language setup. An absolute `/usr/bin/c++` is selected in the profile-child failure, so these errors do not independently establish a PATH lookup problem. The five same tests passed earlier in the all-feature gate. `imported_header_identity_survives_compiler_implicit_include_suppression` also passed later in the failed default gate (1.609 seconds), inconsistent with a persistently absent compiler/module.

## Harness and environment audit

`probe.rs::configure` writes to a supplied work directory and runs CMake synchronously. Test work roots come from distinct tempdirs whose owners remain live while child commands execute. Child environments set only their requested fixture, profile, compiler and output variables; no process-global environment mutation or resource-limit/pre-exec operation was found in quest-build or cmake-rs 0.1.58. `lib.rs::run` uses ordinary `Command::output`. No source-visible lifetime/cleanup race was found.

`native/CMakeLists.txt:2` invokes `project(... LANGUAGES CXX)` before `find_package(QuEST...)` at line 4. The failing system compiler-module read occurs before test-generated imported-package configuration is evaluated. Nextest serializes quest-sys/quest-rs runtime tests, not quest-build; this permits simultaneous distinct CMake processes but does not establish why any module open failed.

Current `/usr/bin/cmake` is CMake 4.3.0; `/usr/bin/c++` is GCC 16.2.1. Both binaries and both failed module files match installed RPM sizes and SHA-256 digests exactly. Module permissions are 0644 and complete direct reads succeed (9084 and 6262 bytes). Files reside in the read-only composefs/overlay system view. Full RPM verification prints ownership/group/time differences in this view; it was not treated as a completely clean metadata verification. Targeted size/digest checks are recorded in `host.json`.

Current NOFILE is 1,048,576, address space and file size unlimited; the system file table reads 9277 allocated against a very large system maximum. No CMAKE, CMAKE_ROOT, CMAKE_MODULE_PATH, CMAKE_TOOLCHAIN_FILE, CC, CXX or LD_PRELOAD override exists in the inspection shell. MPICH CMAKE_PREFIX_PATH and MPICH/CUDA loader directories are inherited and recorded. These current observations cannot certify failure-time state. There is no evidence of a package-content change between gates; no complete historical CMake module byte snapshot was captured at the failed instant.

## Controlled observations

1. The first metadata-trace attempt was denied by sandbox ptrace policy before CMake ran; its failure log is retained. An authorized escalated temporary CMake project then configured successfully under a 20-second outer bound. The trace captures only openat/openat2/newfstatat, not environment or file contents. Both named modules opened successfully, and there were no unexpected failed syscalls beyond normal lookup misses. This changes sandbox and scheduling relative to the failed gate.
2. Four independent minimal CXX projects launched concurrently in the sandbox each configured successfully in approximately 0.18 seconds, with no stderr. They compile only CMake's ordinary compiler checks, use private temporary build trees, and do not invoke QuEST.
3. One focused workspace/default-feature nextest invocation selected exactly the five failed test names, used default nextest parallelism and no fail-fast, and passed **5/5 in 0.970 seconds** (6.872270077 seconds including Cargo). Quest-build files remained byte-identical before and after. Log SHA-256: `e6a8f5de1604d7f63656236e60015f80b7668f1f63cd726e70312237cb098de3`.

The focused invocation started before the original wrapper's exact PATH was communicated. It prepended MPICH to inherited PATH, whereas checkpoint08 used `/usr/lib64/mpich/bin:/usr/bin:/bin:<home>/.cargo/bin`. This is explicitly **not an exact-environment reproduction**. Read-only comparison resolves cmake/c++/cc/ar/make/gmake/ninja to the same `/usr/bin` files under both paths; Rust launchers use home-directory spelling aliases. QUEST_ROOT and MPICC match the gate. No additional invocation was made to chase a green result. Five selected tests also do not reproduce the full workspace's simultaneous load.

## Evidence and next decision

Private evidence directory: `<private-artifacts>/quest-checkpoint-08-cmake-diagnosis`. Machine-readable `checkpoint-08-cmake-diagnosis.json` records command/context, exact limits and artifact hashes (SHA-256 `3d559e9c738dee92191a7e013623f0df3a83ff24f48a6272267002ac7c72666a`). Original checkpoint08 failed log is preserved and hashed independently from the successful focused result.

A future already-planned acceptance gate should retain the exact wrapper environment and its own outcome. If it fails again, a contemporaneous file-open errno trace under the same launcher/concurrency is the discriminating evidence; present data cannot choose among filesystem/runtime/sandbox/timing causes. Do not label this fixed by retry, change source or serialize tests speculatively, or replace the original failed gate with the focused pass.
