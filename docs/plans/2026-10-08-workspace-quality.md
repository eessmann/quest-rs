# Workspace quality, consolidation, and benchmark replication

## Summary and agreed constraints

Review all 23 workspace packages, fix demonstrated correctness problems, consolidate shared implementations, and measure the resulting changes against reproducible local baselines.

- Allow justified breaking APIs; migrate workspace consumers and document changes.
- Require safe first-party Rust across libraries, binaries, build scripts, tests, examples, and benchmarks. Confine necessary audited FFI to `quest-sys`.
- Use unmodified QuEST through documented APIs and documented amplitude access. Unsupported capabilities receive explicit outcomes.
- Preserve independent mathematical verification when consolidating implementations.
- Complete the full benchmark inventory locally, retaining accuracy failures, unsupported cases, timeouts, and resource-limit outcomes.
- Use the installed system dependencies. Keep upstream checkouts unchanged and checked-in documentation free of personal paths.

## Implementation work packages

Execute these as independently reviewed packages using Superpowers. Establish the benchmark harness and baseline before changing the measured production algorithms.

**1. Establish evidence and workspace-wide review coverage.**

Record source revisions, dependency versions, native configuration, features, toolchain, and existing failures. Maintain a per-crate review ledger covering public invariants, ownership, numerical correctness, duplication, allocation, error handling, lint exceptions, and tests. Every finding receives a regression, a documented consolidation, or a specific unsupported explanation.

Preserve the existing crate boundaries unless a demonstrated ownership or dependency problem requires changing them.

**2. Complete MathCore integration.**

Keep `quest-mathcore` as the canonical workspace dependency, preserving its provenance and license.

- Move shared scoped identities into a neutral MathCore module. Quantum-specific identifiers become thin newtypes where domain separation matters.
- Keep backend-neutral arithmetic contracts, exact constants, ordered expressions, and exact algebra in MathCore. Concrete numerical backends, intervals, directed arithmetic, and AD remain in `quest-numerics`.
- Replace mirrored neutral error variants with structured wrapping of the canonical error.
- Share checked finite-dyadic decoding across production adapters; retain independently implemented verifier checks.
- Add an opaque `KernelWorkspace<S>` and `evaluate_with_workspace` for repeated expression evaluation. Admission precedes execution; RAII clears scratch after success or failure.
- Associate lowered kernels with an immutable arithmetic profile covering scalar semantics, precision, and rounding. Reject incompatible execution; changing precision requires re-lowering the original source.

Preserve original expression order, cancelled-binding obligations, source-domain guards, and independent replay in `quest-symbolic`.

**3. Fix numerical and language inconsistencies.**

- Unify polynomial support planning and work admission across complex, point, interval, and AD evaluation. Preserve stored support separately from effective support.
- Correct Laurent evaluation at extreme magnitudes using effective support and scaled reciprocal arithmetic.
- Fix finite QASM conditional export to emit expressions admitted by the existing type system.
- Consolidate directed multiprecision primitives in numerics while retaining QSP-specific certificate policy and independently checked conclusions.
- Preserve algorithm-specific NLFT/RHW completion, cumulative resource accounting, and explicit precision selection.

Initial regressions include constant `Laurent(-2, [0,0,1])` at `1e-200`, reciprocal `Laurent(-1, [1])` at `1e200`, and both QASM conditional polarities.

**4. Enforce safety and native ownership.**

Apply the safety policy to independently compiled targets, including maintained MathCore tests and generated fixtures.

Replace handwritten unsafe allocation instrumentation with safe interfaces from established instrumentation crates. Preserve allocation-count and peak-allocation assertions in isolated measurements; RSS is a separate metric. Replace avoidable OS-level unsafe calls with `rustix`.

Retain existing environment/register lifetimes and thread confinement. Audit each remaining FFI obligation explicitly.

Remove reliance on QuEST’s undocumented communication-buffer scratch ownership. Use the existing admitted owned-storage route; report insufficient-capacity configurations. Keep supported public C density operations and checked documented amplitude access.

**5. Consolidate `quest-build` and `xtask`.**

Use established libraries for generic tooling:

| Responsibility | Implementation |
|---|---|
| Package discovery | Existing `pkg-config` and `build-probe-mpi` |
| Evaluated CMake metadata | Existing `cmake-file-api` |
| CLI parsing | `clap` derives |
| Cargo metadata | `cargo_metadata` |
| Rust compiler metadata | `rustc_version` |
| Link-argument tokenization | Existing `shlex` |

Make evaluated native configurations immutable, with separate editable requests and read-only results. Carry configuration consistently through discovery, bridge compilation, MPI witnesses, and diagnostics. Separate discovery data from Cargo directive emission; preserve one valid JSON document for diagnostic commands.

Retain a narrow CMake process launcher with structured errors and controlled child environments. The inspected `cmake` crate lacks the fallible execution and environment-clearing interfaces required here; document that limitation without adding compensating machinery. [CMake API](https://docs.rs/cmake/latest/cmake/struct.Config.html)

Preserve imported `QuEST::QuEST`, compiler-wrapper arguments, MPI ABI checks, serial-HDF5 selection, ordered linking, and installed-consumer runtime paths.

**6. Consolidate remaining production code.**

- Extend the existing gate registry to generate mechanical identity, arity, parameter-order, adjoint, and export mappings. Keep numerical formulas and verification oracles independently testable.
- Share checked size calculations and reservation machinery while retaining domain-specific budget meanings.
- Separate compiler search ownership, scoring, worker integration, and publication; separate VM storage, frames, and dispatch. Keep transactional sequencing explicit.
- Consolidate CFD geometry admission, shape/storage contracts, normalization, and reusable RK4 scratch. Preserve independent reference calculations and accepted-state/error semantics.
- Narrow broad lint allowances to justified kernels. Introduce type-level machinery where it eliminates an identified invalid state.

## Full benchmark replication

Pin the inspected Rust, `quest-qsvt`, and SoftwareX revisions. Preserve exact input coefficients and hashes; identical random seeds across languages do not establish identical fixtures.

| Suite | Required coverage |
|---|---|
| Named NLFT fixture | All 17 inverse orders through 1,000,000 and nine solver cases |
| SoftwareX | Frozen 62-case full corpus |
| Root isolation | Five source workloads |
| Polynomial evaluation | Both roots-of-unity workloads |
| Circuit operations | Four degrees × five stages |
| Unitary admission | Four matrix dimensions |
| Coordinate preparation | Degree 8105, including its original three-sample protocol |

Verify the frozen SoftwareX corpus against SHA-256 `806b1557498debe5d409b214609ad30a74a86d529ae36045a2b3c428b9546e6d`.

Add opt-in safe `benchmark-support` interfaces that invoke production kernels directly. Distinguish inverse-only, forward-only, completion/inverse, validated roundtrip, and full-pipeline operations. Preserve original SoftwareX solver/kernel boundaries separately where their contracts differ.

Use Criterion through actual `cargo nextest bench`, with a supported minimum nextest version. Keep discovery free of fixture construction and numerical execution. [Nextest benchmark support](https://nexte.st/docs/features/benchmarks/)

Build an expected-result manifest identifying source suite, input hash, scope, implementation, backend, workers, and measurement protocol. Preserve source-defined configurations; keep the historical 1,488-row campaign separate from current coverage.

Perform correctness preflight outside timing. Preserve source tolerances, explicitly recording requested and achieved accuracy. Never time a successful return from an error path or relax tolerances to obtain results.

Keep source fixed-sample measurements distinct from Criterion distributions. Bound preparation, validation, warmup, and measurement; retain the phase where a timeout or failure occurred.

Run every `quest-qsvt` configure, build, test, and execution under:

```sh
systemd-run --user --scope \
  -p MemoryHigh=24G \
  -p MemoryMax=28G \
  <command>
```

Verify effective limits before workloads start. Serialize heavyweight builds and measurements, use separate baseline/current build directories, and preserve source-defined deadlines. Use a four-hour per-case ceiling where the source supplies none.

Publish raw measurements and a completion marker only after every expected row has a terminal outcome. Distinguish complete accounting from successful performance coverage.

## Validation and acceptance

- **Mathematics:** extreme Laurent values, genuine poles, padding invariance, interval containment, exact rational bounds, rounding boundaries, arithmetic-profile mismatch, and source-versus-simplified-expression obligations.
- **Ownership:** compile-fail lifetime/thread checks, FFI lifecycle failures, scratch reuse after errors, allocation positive controls, and warm-allocation contracts.
- **Language/compiler:** QASM round trips, gate mappings, deterministic optimization, budget exhaustion, interruption, and publication semantics.
- **Build tooling:** paths with spaces, selected Cargo/compiler executables, missing dependencies, stale configuration, MPI mismatches, HDF5 selection, ordered linking, and parseable diagnostics.
- **Workspace:** formatting, strict all-target Clippy, default and all-feature nextest suites, separate doctests, explicit release scale tests, binding freshness, and installed consumers.
- **Local native execution:** CPU/OpenMP and matching two-/four-rank MPI. Record unavailable hardware/platform gates explicitly.
- **Performance:** compare equivalent inputs and operation boundaries; report timing distributions, allocation measurements, and memory separately.

Deliver the consolidated code, migrated APIs and examples, review ledger, reproducible benchmark commands, complete result accounting, and a concise report of improvements and remaining upstream limitations.

## Subsequent dependency decisions

Retain `hdf5-metno`. Do not adopt `system-deps`, Woodshed, or `rust-hdf5`. Simplify existing HDF5 discovery and build integration while preserving formats and admission contracts.

The user subsequently authorized the existing locked `quest-qsvt` devenv for
native reference dependencies. Use it locally under the same memory scope;
preserve the installed-dependency attempt and upstream lockfiles.
