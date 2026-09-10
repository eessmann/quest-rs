# QuEST workspace implementation and verification — 2026-09-10

The workspace foundation and initial M0–M5 implementation are complete for the
documented Linux GNU recipe and macro profile. All 100 workspace Nextest tests
pass. This is local implementation acceptance, not a release or a claim of
coverage on every native backend.

Work was prepared on `codex/quest-faer-workspace`, preserving the user's checkout
and edits, with the root `Cargo.lock` tracked alongside the implementation.
The user subsequently requested a local commit and merge into `main`; the
configured SSH commit signing is retained. No remote push is part of that request.
Installed QuEST, CUDA and cuQuantum files
were not changed.
The approved [plan](2026-09-10-quest-faer-implementation.md) and
[architecture](../specs/2026-09-10-quest-rust-design.md) describe the scope.

## Delivered stages

| Stage | Implementation |
| --- | --- |
| Workspace | Six explicit members under `crates/`; virtual resolver-3 root; facade package `quest-rs`, library `quest`, default member; inherited metadata/dependencies/lints/profiles; pinned nightly and one root lockfile. |
| M0 | Serialized lifecycle and non-reused owner-thread identity; safe validation-disable removed; checked matrix/density/Kraus/seed/policy adapters; regenerated 4.3 coverage; shared CMake File API configuration and final-target DT_RPATH. |
| M1 | Environment-bound state-vector/density registers, checked values and memory admission, kind-specific operations, explicit deep clone/promotion, fallible close, independent faer snapshots and optional ndarray conversion. |
| M2 | Program-owned IDs, exact rational angles and named bindings, immutable numerical operators/channels, reusable unitary definitions, private dependency DAG, classical hazards, source provenance and consuming compilation stages. |
| M3 | Binding/lowering/planning, transactional preparation and native caches, standard gates, signed controls/global phase, numerical A psi and A rho A†, measurement/reset/classical predicates/channels, explicit sampling and partial-failure context. |
| M4 | OpenQASM-style Rust macro with pinned 3.1.0 gate semantics, exact pi expressions, once-only Rust interpolation, modifiers, effects, source-local errors, runtime keyword locations and renamed-dependency hygiene. |
| M5 | Conservative adjacent symbolic rewrites and bounded sequential faer fusion; phase, ordered targets, effects and provenance retained; counts/depth/matrix storage/pass-time reports. |

Faer is locked to 0.24.4 with defaults disabled and only `std`/`linalg` enabled.
Numerical kernels explicitly use `Par::Seq`; column matvec products avoid hidden
GEMM packing allocations. Matrix admission accounts for padding and copies
logical values from strided and conjugated views. Empirical residual checks never
grant exact unitary inverse capability. The bridge has no faer dependency.

## Verification environment

- Rust `1.100.0-nightly (f248f4038 2026-09-05)`, pinned by
  `nightly-2026-09-06`; host `x86_64-unknown-linux-gnu`.
- Installed QuEST **4.3.0**, binary64, deprecated APIs disabled, at
  `/var/home/erich/Projects/opt/quest`; source at `/var/home/erich/Projects/QuEST`.
- Shared schema-2 record: `target/quest-native.json`, generated with the explicit
  CUDA directory `/var/home/erich/Projects/opt/cuda/targets/x86_64-linux/lib`.
  It records compiler/configuration, ordered includes and observed QuEST/CMake/
  library identities. It does not inventory every system header.
- Native tests use CPU simulation against the GPU-enabled installed library.
  Lifecycle tests isolate each case in a subprocess, including ordinary Cargo.

For native commands below, set
`QUEST_NATIVE_CONFIG="$PWD/target/quest-native.json"`. Generation additionally
uses `QUEST_ROOT=/var/home/erich/Projects/opt/quest`.

| Check | Final result | Local evidence |
| --- | --- | --- |
| `cargo build --workspace --all-features --locked` | Passed | `/tmp/quest-final-build.log` |
| `cargo nextest run --workspace --all-features --locked --no-fail-fast` | **100 passed, 0 failed, 0 skipped**, across 20 binaries | `/tmp/quest-final-nextest.log` |
| `cargo test --doc --workspace --all-features --locked` | **4 passed**, one intentionally ignored downstream build-script illustration | `/tmp/quest-final-doctests.log` |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed under the original lint policy; see the subsequent policy change below | `/tmp/quest-final-clippy.log` |
| `cargo fmt --all -- --check`; working/index `git diff --check` | Passed | Final local command output |
| `cargo run --locked --example minimal` | Correct Bell amplitudes | `/tmp/quest-example-minimal.log` |
| `cargo run --locked --example bell_circuit` | 1024 shots, only 00/11: 535/489 | `/tmp/quest-example-bell.log` |
| `cargo run --locked -p xtask -- generate-quest-bindings --check` | Passed freshness against installed 4.3 headers | `/tmp/quest-final-generator.log` |
| `cargo test -p quest-circuit --no-default-features --locked`, native/libclang discovery variables unset | **28 tests and 1 doctest passed** | `/tmp/quest-final-pure.log` |
| Pure all-feature circuit suite | **35 tests and 1 doctest passed**, including 12 compile-fail fixtures | `/tmp/quest-circuit-tests-final.log` and final workspace run |
| Standalone direct, wrapped and renamed consumers | All three execute outside Cargo with loader variables unset; DT_RPATH and complete indirect native closure checked | `/tmp/quest-native-consumers-final.log`; `/tmp/quest-native-consumers-8h21jg9_/` |
| Pure renamed consumer and Unicode macro source ranges | Passed without native configuration; keyword ranges resolve to original source | `/tmp/quest-core-span-review.log` |
| `cargo package --list --allow-dirty --offline`, all five publishable crates | Required files included; stale trybuild scratch removed | `/tmp/quest-{build,sys,circuit,macros}-package-files.txt`, `/tmp/quest-package-files.txt` |
| `cargo metadata --locked --offline --no-deps`; dependency features | Six relocated members, correct default, no faer default/BLAS features | `/tmp/quest-workspace-metadata.json`, `/tmp/quest-faer-features.txt` |

The 100-test total includes 8 build-helper tests, 35 circuit tests, 13 facade
tests, 31 bridge tests and 13 generator tests. The facade total includes its
six-fixture ownership/kind/thread compile-fail harness. Generated differential
tests include 96 seeded small-circuit cases alongside explicit regressions.

The first final Nextest run passed 99/100: a compile-fail snapshot still printed
the anonymous lifetime before the independent density-promotion fix. Its actual
diagnostic correctly rejected density amplitude access. Updating only that
expected lifetime spelling and rerunning produced 100/100. Earlier implementation
failures were repaired and superseded by these final runs. The
[dated audit](../specs/2026-09-10-quest-bridge-audit.md) preserves its baseline.

The native C++ compiler still emits a `maybe-uninitialized` warning in generated
CXX `Vec<QuestComplex>` glue. The original Rust Clippy checks passed under
`-D warnings`; this is not a claim of warning-free native compilation. Trybuild's temporary workspaces also
emit unused-workspace-dependency manifest warnings.

## Subsequent Clippy policy and integration checkpoint

The user requested workspace-wide denied `pedantic`, `nursery`, panic/indexing/
arithmetic/conversion lints and four test allowances in root `clippy.toml`.
All six members inherit the policy, and the pinned Clippy accepts every setting.
Formatting and manifest checks pass. Clippy now fails on existing source
violations: the initial workspace run reports 19 errors in `quest-macros` and
20 in `quest-build`, preventing a complete downstream lint inventory. These are
known outstanding fixes, not waived lints or a passing current lint result.
Evidence: `/tmp/quest-strict-lint-policy.log`.

The user requested committing and merging this state after those failures were
reported. The local integration checkpoint passed all 100 workspace Nextest tests
and four doctests (one build-script illustration ignored). Logs are
`/tmp/quest-precommit-nextest.log` and `/tmp/quest-precommit-doctests.log`.
The original implementation and baseline audit evidence above remain historical
records of their stated policies.

## Acceptance evidence and implementation boundaries

Independent scalar references cover complex products, adjoints, residuals,
fusion order and phase. Explicit fixtures cover padded/submatrix/reversed-stride
and conjugated views, nonsymmetric complex matrices on nonsorted targets,
imaginary density off-diagonals and both rectangular block orientations.
Runtime tests cover overflow/budget rejection, preparation cleanup, independent
transfers and density promotion, snapshots after teardown, signed controls,
ambient numerical-policy rejection before mutation, channels and partial failure.
Pure tests cover ownership, cycles, quantum/classical/stochastic edges, definition
expansion rollback, exact rewrite admission and cumulative fusion budgets.
Independent bridge/runtime and pure-core reviews found no remaining load-bearing
issue after their regression fixes.

The initial exact-angle representation is an owned rational/parameter/finite
value sum type; it does not yet need a general expression arena. Parameter sums
are deliberately not introduced by rewrites. Gate semantics live in the core;
the separate macro spelling/arity table has exhaustive inventory equivalence
tests. There is no generated cross-crate gate registry yet.

Definitions contain complete immutable expanded bodies, excluding recursion by
construction. Programs are straight-line effectful DAGs with builder-level
classical predicates. General regions, versioned classical SSA and runtime loops
are future extensions. Macro provenance retains one keyword location and
occurrence IDs, without a full expansion stack.

Matrix and preparation estimates include retained payloads, native CPU/GPU
copies, conversion buffers and bounded scratch. Pure metadata is limited by
counts, without an aggregate metadata-byte budget. Allocation can still fail.
Direct bridge calls are outside facade accounting; the first seeded batch
retains a 4096-byte native RNG allowance. Execution still performs small CXX
argument conversions and is not advertised as allocation-free.

Consuming stages can be measured separately; automatic elapsed-time reports
currently cover optimization passes. No benchmark or speedup claim is made.
Numerical fusion has no certified approximation bound or cross-architecture
bitwise-equivalence guarantee.

The verified deployment recipe is **native Linux GNU, absolute DT_RPATH**.
Final executable build scripts use the same native record; installed libraries
remain unchanged. Indirect GPU **loading** was verified; GPU simulation, MPI,
other operating systems/architectures, static/relocatable deployment and
sanitizers were not verified. Distributed facade allocation is rejected pending
a collective allocation/error protocol. Package checks establish contents, not
registry publication or release-build acceptance.

OpenQASM text import/export, user gate declarations in the macro, structured
classical frontend control, advanced synthesis, ZX and hardware routing remain
the separately scoped later milestones from the approved design.
