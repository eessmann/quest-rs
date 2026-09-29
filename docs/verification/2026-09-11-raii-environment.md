# RAII environment lifecycle verification — 2026-09-11

This record covers the [environment lifecycle](../book/src/runtime.md) after
removing explicit facade shutdown. Earlier dated records describe the former
explicit-close/recovery API.

## Behavior and regression evidence

The facade no longer exports `Environment::close()`, `CloseError`, or a
replacement shutdown method. It has no published environment `active` flag.
Drop unconditionally calls the hidden, unit-returning bridge cleanup helper.
The same helper handles failed publication after successful native initialization;
an initialization rejection never invokes cleanup on another owner's runtime.

The bridge retains its owner-thread and live-handle checks. Drop cleanup catches
native exceptions, including lifecycle-lock acquisition failures, and uses a
one-way atomic retirement latch to reject subsequent initialization and native
operation admission. Failed cleanup preserves native storage unsafe to destroy
and allows the process to continue. Ordinary low-level finalization retains its
recoverable preflight rejection and idempotence after successful cleanup.

The new facade orphan-handle regression first failed against the original Drop:
`is_quest_env_init()` remained true after the environment scope ended. It passes
with terminal retirement. Successful-cleanup tests require both inactive bridge
state and a successful idempotent low-level finalization; retirement tests
require that low-level finalization remain an error.

Eight new facade tests and four new bridge tests run in isolated child processes.
They cover normal scope exit, early `?`, restart rejection through both APIs,
duplicate initialization leaving the owner usable, configuration rejection
before native entry, orphan-handle retirement/destruction, wrong-thread cleanup,
and cleanup during Rust unwinding, including retirement during unwinding.

The preparation test observes actual live native counts for two registers,
two dense matrices, two diagonal matrices, a channel and a structured reset
map before cleanup. It verifies accounting returns to zero before environment
Drop and successful native finalization afterward. Owned snapshots and pure
Rust numerical payloads remain usable beyond the environment scope. The two
new compile-fail fixtures reject structured prepared programs outliving their
environment or crossing threads; existing ownership/register-kind/thread
fixtures remain passing.

Examples, tutorials, diagnostics/snapshot tests and generated downstream facade
fixtures use lexical cleanup. Deliberate resource drops used for accounting,
input-independence checks or reuse of a memory budget remain. No dependency or
workspace lint-policy changes were needed. Two unwind tests have scoped
`panic_in_result_fn` expectations for their deliberately caught sentinel panics.

## Configuration and commands

Verified on Linux x86_64 GNU using pinned `nightly-2026-09-06` and the installed
QuEST **4.3.0** package at `/var/home/erich/Projects/opt/quest`. The package is
binary64 with deprecated APIs disabled; CUDA, cuQuantum, MPI, OpenMP,
subcommunicators and BMI2 are enabled. The bridge compiler is `/usr/bin/c++`.
No native installation or native source changes were made.

The checks used:

```sh
unset QUEST_NATIVE_CONFIG QUEST_RUNTIME_LIBRARY_PATH
unset LD_LIBRARY_PATH LD_PRELOAD LD_AUDIT LIBRARY_PATH
export QUEST_ROOT=/var/home/erich/Projects/opt/quest
export CARGO_TARGET_DIR=/var/home/erich/Projects/quest-rs/target
export CARGO_BUILD_JOBS=4
export QUEST_TUTORIAL_WORKER="$CARGO_TARGET_DIR/debug/quest-optimizer-worker"
```

| Check | Result |
| --- | --- |
| Baseline `cargo nextest run --workspace --locked` | 303 passed with default features. |
| `cargo build --workspace --all-features --locked --offline` | Passed. |
| `cargo nextest run --workspace --all-features --locked --offline` | **345 passed, zero skipped**, run `b1df6208-91e0-498a-9ae7-201f83a6b2ce`. Includes existing native lifecycle, synthesis and ZX coverage. |
| `cargo test --doc --workspace --all-features --locked --offline` | **13 passed**, one intentional ignored final-package `build.rs` snippet. |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | Passed under the existing strict policy. |
| `cargo fmt --all -- --check`, `git diff --check` | Passed. |
| `cargo run -p xtask --locked --offline -- generate-quest-bindings --check` | Passed; no generator-owned artifacts changed. |
| Facade `minimal`, `bell_circuit`, `tutorials` examples | Passed with `--all-features --locked --offline`. Tutorials also executed certified synthesis using the rebuilt worker. |
| `cargo run -p quest-sys --example min_example --locked --offline` | Passed: GPU-backed 20-qubit register, total probability `0.9999999999999997`, one QuEST-owned MPI rank. |
| `cargo run -p xtask --locked --offline -- check-native-consumers --work-dir '/tmp/quest raii native consumers'` | All four independent consumers passed: direct, facade, wrapped and renamed. |
| `cargo package --workspace --list --allow-dirty --locked --offline` | Passed; includes the new lifecycle tests and compile-fail fixtures. |
| mdBook 0.5.4 `build docs/book` | Passed using the previously built local executable; mdBook is not on PATH. |

Native MPI tests and the native GPU example required local network access
outside the sandbox. Facade lifecycle tests explicitly use the default
non-distributed CPU configuration. The independent consumers ran outside Cargo
with loader variables unset; ELF inspection confirmed RUNPATH and resolution of
QuEST, MPI, OpenMP, cuStateVec and cuBLAS/cuBLASLt. This dependency check is
separate from the low-level example's limited GPU execution smoke check.

## Limits

Mutex-acquisition failure, the builder's post-initialization query failure and
actual GPU/MPI failures partway through native finalization were reviewed by
inspection, not injected. No production fault-injection API was added. Guarded
retirement with live handles and wrong-thread cleanup is tested directly.

The GPU example retains ordinary low-level finalization; its smoke result does
not claim injected GPU teardown coverage, distributed simulation correctness,
or validation on other operating systems. RAII cannot guarantee cleanup after
abort, forced termination or deliberately forgetting the environment. Forgotten
or retired native allocations can remain until process exit. Retirement never
permits restarting the native runtime.
