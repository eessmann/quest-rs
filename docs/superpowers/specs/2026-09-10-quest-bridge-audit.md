# QuEST bridge and public Rust API baseline audit

Date: 2026-09-10. Read-only production audit for the [proposed design](2026-09-10-quest-rust-design.md). No production fixes were applied. The initial checkout was clean at `6f2e5d5`.

## Installed configuration

Verified installed headers, exported CMake package, and ELF library identify **QuEST 4.3.0**, shared Release, binary64, deprecated APIs disabled. OpenMP, CUDA, cuQuantum and BMI2 are enabled; MPI, subcommunicators, HIP, ADIOS2 and NUMA are disabled.

Install: `/var/home/erich/Projects/opt/quest`. CMake metadata: `lib64/cmake/QuEST`. Library: `lib64/libQuEST.so.4.3.0`. Native source `/var/home/erich/Projects/QuEST` declares 4.3.0; git describes that source as `v4.0.0-89-gaa19534a`. The source build cache has different feature settings from the installed package and must not be used as installed-build provenance.

## Findings requiring implementation

### P1 — Safe validation-disable call invalidates safe adapter assumptions

`crates/quest-sys/src/generated_api.rs:2886` exposes `set_qu_est_validation_off()` as a safe function. The manual amplitude adapter delegates index validation to native QuEST. An isolated downstream program initialized a one-qubit register and called `get_qureg_amp(&qureg, -1)`: validation-on returned `QuestError::Validation`; after the safe disable call, the same safe read returned invalid/garbage amplitude data instead of rejecting the index.

This is a concrete safe-API soundness problem, not just poor input diagnostics. The public facade cannot compensate for another safe bridge caller disabling its assumptions. Remove global validation-disable controls from the safe surface or retain them only behind an explicit unsafe contract. Audit tolerance setters and all other global knobs that may affect memory-safety checks. Preserve independent adapter validation where lengths or borrowed slices impose Rust-side obligations.

Evidence: `crates/quest-sys/src/generated_api.rs`, `crates/quest-sys/src/cxx_bindings/generated_api.cpp`, and the manual amplitude forwarding functions in `quest_bindings.cpp`. Isolated reproducer: `/tmp/quest-rs-bridge-audit/src/bin/safety.rs` (temporary, not a committed regression test).

### P1 — Lifecycle accounting does not serialize global native state

`crates/quest-sys/src/cxx_bindings/quest_bindings.cpp:31` contains atomic live-resource counts. Initialization and finalization at lines 445 onward directly call native QuEST. Resource creation, live-count publication, finalization admission, and native shutdown do not form one synchronized transaction.

Native `/var/home/erich/Projects/QuEST/quest/src/api/environment.cpp:52` owns an ordinary global pointer; shutdown near line 476 frees and clears it without locking. Safe free functions can race even when opaque resource handles are never transferred across threads. This finding is source-derived; no deliberate concurrent native race was run.

Centralize the lifecycle in the bridge and enforce the supported calling-thread policy before touching native state. Test duplicate attempts and creation/finalization ordering in isolated processes. Native OpenMP support does not establish thread safety for multiple independent Rust callers.

### P1 — Runtime paths do not reach a downstream executable

`crates/quest-sys/build.rs:60` emits RPATH link arguments and runtime-path metadata. Root `build.rs:11` consumes that metadata for its own targets. A separate Cargo project depending on the public crate builds successfully, but its ELF has `NEEDED libQuEST.so.4` with no RPATH or RUNPATH. Direct execution with `LD_LIBRARY_PATH` unset exits 127 because `libQuEST.so.4` cannot be found.

This matches Cargo's documented package-local link arguments and immediate-dependency metadata boundary. A final-executable build helper or explicit deployment configuration is required; the current metadata is not automatic propagation through library dependencies. [Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html).

Temporary reproducer: `/tmp/quest-rs-bridge-audit/Cargo.toml` and `src/bin/downstream.rs`. It was built into the shared repository target directory at `/var/home/erich/Projects/quest-rs/target/debug/downstream`. It succeeds when both native QuEST and CUDA directories are explicitly supplied through `LD_LIBRARY_PATH`; a direct rerun from `/tmp` with that variable unset reproduced the missing-QuEST loader failure.

### P1 deployment prerequisite — Indirect GPU libraries are unresolved

Workspace test binaries already find QuEST. The installed QuEST library has RUNPATH:

```text
$ORIGIN/../lib64:/home/erich/Projects/opt/cuquantum/lib
```

That locates `libcustatevec.so.1`. The latter has no RPATH/RUNPATH and depends on `libcublas.so.13` and `libcublasLt.so.13`, installed in `/var/home/erich/Projects/opt/cuda/targets/x86_64-linux/lib`. The loader consequently fails before tests can be enumerated.

This native dependency-closure problem is independent of the downstream Cargo issue. The final-target helper must not claim that adding the QuEST directory, or adding another executable RUNPATH, repairs every indirect GPU dependency. Select and verify a complete native deployment policy. No installed binaries, symlinks, loader configuration, or upstream QuEST source were changed during this audit.

### P2 — Valid pure-state initialization of a density matrix is rejected

`crates/quest-sys/src/cxx_bindings/quest_bindings.cpp:538` checks the input amplitude count against `raw.numAmps`. For an n-qubit density register, that storage count is `4^n`; native `initArbitraryPureState` accepts `2^n` pure amplitudes and constructs the outer product, as shown in `/var/home/erich/Projects/QuEST/quest/src/api/initialisations.cpp:100`.

An isolated one-qubit density register initialized from `[1, 0]` returns the bridge's length error. Fix the admission rule and test complex asymmetric states, expected outer products, invalid lengths, and both register kinds. Temporary reproducer: `/tmp/quest-rs-bridge-audit/src/bin/density.rs`.

### P2 — Version admission and coverage descriptions are inconsistent

`crates/quest-sys/src/cxx_bindings/include/quest_bindings.hpp:15` admits major 4, minor at least 2, but patch exactly zero. This accepts unverified future minor versions while rejecting patch releases. Its precision, deprecated-API and 64-bit index checks are useful and should be preserved.

`crates/quest-sys/generated/api_coverage.json:4` and crate documentation still name 4.2.0. The reported 279 generated and 7 RAII entries describe overload classifications, not complete usable API coverage. The manifest marks manually exposed `initArbitraryPureState` unsupported. Missing adapter categories include small matrix constructors, channel setters, custom seed access, density submatrix access, and weighted sums.

Regenerate from the reviewed 4.3 headers and make the manifest distinguish generated/manual/RAII/excluded/unsupported signatures. Prioritize adapters required by the public runtime and executor, rather than inferring readiness from an aggregate percentage. Generator freshness is an independent acceptance check.

### P2 — Public Rust facade does not enforce the intended ownership and input rules

| Location | Current behavior | Required direction |
|---|---|---|
| `src/core/environment.rs:4` and `:21` | Defines a singleton claim function which constructors never call; constructors invoke native initialization directly | One lifecycle authority in the bridge; no disconnected facade singleton |
| `src/core/environment.rs:16` | Environment owns only a copyable capability snapshot, with no explicit calling-thread marker | Unique active guard; explicit runtime thread policy |
| `src/core/environment.rs:44` | Drop discards every finalization error | Explicit fallible close plus non-panicking fallback with truthful failure semantics |
| `src/core/register.rs:23` | Register has no environment lifetime; register kind is a boolean | Environment-bound RAII owner with state-vector/density capability types |
| `src/core/register.rs:30`, `:38`, `:56` | Narrows `usize` to `i32` using `as` | Checked conversion, range and ownership validation |
| `src/core/register.rs:62` | Full state read uses unchecked shifts and is callable on density registers | Kind-specific bounded access; checked dimensions and allocation limits |
| `src/core/errors.rs` | Useful structured error types and context conversion exist, but public methods return raw `QuestResult` | Adopt one actual public error path |
| `src/lib.rs`, `examples/minimal.rs` | Demonstration `add` function and hello-world example | Quantum examples demonstrating safe lifecycle and numerical behavior |
| Root `Cargo.toml` | Mandatory ndarray with BLAS features for simple amplitude copying | Optional ndarray integration; minimal core numeric dependencies |

The bridge currently rejects sequential finalization with live handles; a register lacking a Rust environment lifetime therefore does not alone prove a use-after-free. It does permit misuse, failed shutdown and lost error reporting. Distinguish this API design defect from the independently demonstrated validation-off soundness problem.

### P2 — Documentation and alternate build files are stale

The low-level README example near line 71 imports nonexistent `quest_rs_sys::safe`, advertises features absent from Cargo.toml, and gives a native installation command without the required deprecated-off setting. `crates/quest-sys/src/cxx_bindings/CMakeLists.txt` names removed source/header files. Either support and test that path or remove the stale build route; rewrite the README around the actual supported API and QuEST configuration.

## Executed baseline

```bash
env -u LD_LIBRARY_PATH QUEST_ROOT=/var/home/erich/Projects/opt/quest \
  CARGO_BUILD_JOBS=4 cargo build --workspace
```

Passed. This establishes compilation/linking in the workspace, not runtime deployability.

```bash
env -u LD_LIBRARY_PATH QUEST_ROOT=/var/home/erich/Projects/opt/quest \
  CARGO_BUILD_JOBS=4 cargo nextest run --workspace
```

Failed before test enumeration: missing `libcublas.so.13`; Nextest exit 104, loader exit 127.

```bash
env -u LD_LIBRARY_PATH QUEST_ROOT=/var/home/erich/Projects/opt/quest \
  CARGO_BUILD_JOBS=4 cargo test --doc --workspace
```

Passed with **zero doctests** in both libraries. This supplies no documentation-example coverage.

```bash
env QUEST_ROOT=/var/home/erich/Projects/opt/quest \
  LD_LIBRARY_PATH=/var/home/erich/Projects/opt/cuda/targets/x86_64-linux/lib \
  CARGO_BUILD_JOBS=4 cargo nextest run --workspace --no-fail-fast
```

Diagnostic workaround: **32 executed, 30 passed, 2 failed, 0 skipped**. All 21 `quest-sys` tests passed. Two xtask parser tests failed in libclang with messages including `no member named 'abs' / 'acos' in namespace 'std'`. That toolchain issue remains unresolved. Complete diagnostic log: `/tmp/quest-rs-bridge-audit/nextest-diagnostic.log`.

Additional checks:

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | Failed: two existing formatting differences in `crates/quest-sys/build-support/quest.rs`, displayed twice because the module is also included in tests |
| `cargo clippy --workspace --all-targets` | Passed with existing dead-code, example, and generated-C++ warnings |
| `cargo run -p xtask -- generate-quest-bindings --check` | Failed during libclang parsing with the same standard-library errors, before any freshness comparison; generated freshness remains unverified |

Clippy and the generator check used the same explicit QuEST prefix, `CARGO_BUILD_JOBS=4`, and unset `LD_LIBRARY_PATH`. Their temporary logs are `/tmp/quest-rs-bridge-audit/clippy.log` and `/tmp/quest-rs-bridge-audit/generator-check.log`. These checks did not rewrite source or generated files.

The CUDA environment-variable workaround is diagnostic evidence, not acceptance of the RPATH requirement. Neither successful compilation nor the focused bridge tests constitutes complete release validation. GPU execution, MPI, macOS, Windows, static deployment, and sanitizer coverage are not established by these checks.

## Implementation acceptance

Before declaring the bridge ready: safe callers cannot disable its safety assumptions or race lifecycle state; every public owned resource participates in lifetime/shutdown rules; initialization and error behavior are documented; density pure-state initialization passes; generated/manual coverage matches 4.3; documented examples and generator checks work; and fresh direct and wrapped downstream executables run outside Cargo with loader variables unset under the selected native deployment policy.

The ownership and numerical tests in the main design extend this baseline. Preserve actual failing checks in reports until they are repaired and rerun successfully.

## Implementation follow-up (2026-09-10)

The findings and baseline test counts above are retained as observed at the
original audit. The approved implementation replaces native discovery, adds
serialized owner-thread admission, removes safe validation-disable access, and
adds corrected pure-state, dense-matrix, rectangular-density, channel, seed and
numerical-policy adapters. Regression and integration results are recorded in
[the implementation ledger](../plans/implementation-status.md).

A source-level follow-up found QuEST 4.3's C++ rectangular density setter passing
the row count for both dimensions. The bridge uses the C overload with independent
counts; tests exercise both rectangular orientations. Final-executable DT_RPATH
and explicit recorded native closure paths resolve this installation's indirect
cuBLAS lookup without modifying the native installation.

Final follow-up verification passed all 100 workspace Nextest tests (including
31 bridge tests), four doctests, strict Rust Clippy, formatting, both facade
examples and generator freshness. Standalone direct, wrapped and renamed
consumers executed numerical checks outside Cargo with loader variables unset;
their ELF tags and complete indirect cuQuantum/cuBLAS closure were inspected.
Native execution coverage remains CPU-only. The ledger records the remaining
generated-CXX warning and unsupported deployment/backend coverage. These results
supersede the baseline failures without rewriting the original evidence above.
