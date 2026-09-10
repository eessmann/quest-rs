# Native CMake implementation — 2026-09-10

Approved by the user in the implementation request. The Rust baseline is
`codex/openqasm-ssa` at `2c2135d`; the native baseline is QuEST `411b762c`.
Preserve the completed compiler, bridge ownership and numerical admission.

## Agreed architecture

Use the installed `QuEST::QuEST` target to compile the static CXX bridge through
`cmake 0.1.58`. Use `cmake-package 0.2.0` for discovery, and a focused CMake File
API query for evaluated link requirements. The query and bridge share a CMake
project, configuration and toolchain. Do not flatten unevaluated target
properties into compiler arguments or rely on `cmake-package::target.link()`.

Keep a final-executable helper: Cargo does not propagate non-library linker
arguments through Rust library dependencies. Preserve evaluated argument groups
and library ordering and reject unsupported linker-state constructs. Generate
CXX sources through `cxx-build`; CMake owns C++20 compilation and target usage
requirements. Pass paths through escaped CMake inputs, including spaces.

Keep typed build errors, QuEST 4.3.x/binary64/deprecated-disabled checks and the
native Linux GNU target boundary. Translate the `cmake` dependency's panic-based
build failures at a narrow boundary. Keep coherent compiler/package selection
and normal Cargo/CMake native-input rebuild tracking.

## Tasks and acceptance

1. **Native installation.** In QuEST's `setup_quest_rpath`, append the relative
   runtime path and respect the initialized `INSTALL_RPATH_USE_LINK_PATH`.
   Test defaults, explicit paths and link-path opt-in. Reconfigure the existing
   build with `CMAKE_INSTALL_RPATH_USE_LINK_PATH=ON`, preserving all other
   configuration, and reinstall. A plain imported-target consumer must build
   and run with loader variables unset before native Rust validation proceeds.
2. **Build core.** Replace compiler-flag replay and the attested JSON native
   record with target-based discovery, compilation and evaluated linking.
   Remove production `ldd`, SHA records and forced legacy `DT_RPATH`. Obsolete
   `QUEST_NATIVE_CONFIG` and `QUEST_RUNTIME_LIBRARY_PATH` receive migration
   errors. Retain aliases and consistent standard CMake package selection.
3. **Tooling.** Replace `configure-native` and the Python native consumer
   harness with Rust `xtask check-native-consumers`. Cover direct `quest-sys`,
   facade, wrapped and renamed consumers outside Cargo. Binding generation
   consumes the selected target's evaluated header context while retaining
   matching LLVM driver/libclang resource admission.
4. **Integration and documentation.** Centralize dependencies using cargo-edit,
   connect the bridge build script, update current setup documentation and
   preserve dated audit/verification records. Keep the independent mathematical
   oracle fixture generator; it is unrelated to native build discovery.
5. **Verification.** Exercise generator expressions, paired linker options,
   unsupported arguments, missing dependencies, ABI errors, compiler coherence
   and spaces in paths. Run workspace build, Nextest, doctests, strict Clippy,
   formatting, examples, generator freshness and package contents with the
   tracked lockfile. Recheck pure compiler builds without native prerequisites.
   Distinguish GPU loader resolution from actual GPU execution. Record fresh
   native configuration and test results without rewriting historical evidence.

No new cross-compilation, operating-system or static-linker-group support is
implied. No merge or push is part of this implementation request.
