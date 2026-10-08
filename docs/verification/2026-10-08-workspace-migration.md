# Workspace consolidation migration — 2026-10-08

These changes implement the [workspace quality plan](../plans/2026-10-08-workspace-quality.md).
The [review ledger](2026-10-08-workspace-review.md) records inspected boundaries;
execution results are recorded separately from API migration.

## Arithmetic and reusable execution

Every implementation of `mathcore::arithmetic::Backend` now declares `profile()`.
The immutable `ArithmeticProfile` identifies scalar semantics, precision and rounding.
AD adapters preserve the inner policy and distinguish their scalar representation;
budget adapters forward the profile. Resource counters and resource ceilings are
not arithmetic policies. Custom backends must report every runtime setting that
changes scalar interpretation.

Lowered expression and sparse-polynomial kernels reject a different arithmetic
profile before execution. To change precision, lower the original source again;
stored rounded constants cannot recover discarded precision.

For repeated expression execution, allocate `kernel.workspace()` once and call
`kernel.evaluate_with_workspace(&mut backend, &inputs, &mut workspace)`.
The opaque workspace retains capacity and clears temporary values on success,
error or unwinding. The convenience `evaluate` method still allocates scratch.
Arbitrary-precision scalar cloning and backend operations can allocate independently.

Neutral `Owner` and `Symbol` identities live in `mathcore::identity`, with
compatibility reexports retained. Quantum identities retain domain-specific wrappers.
`quest_numerics::arithmetic::ArithmeticError::Core` wraps the canonical MathCore
error; callers matching the formerly mirrored variants should match the inner
`mathcore::arithmetic::ArithmeticError` instead. Numerical errors remain distinct.
Finite binary64 decomposition is available through `mathcore::dyadic::parts`;
checked parts expose getters rather than writable fields.

## Native configuration and tooling

Use `NativeBuildRequest` to edit discovery inputs, then evaluate them into a
`NativeBuildContext`. Evaluated context, package, MPI and HDF5 fields now have
read-only accessors. For example, replace `native.prefix` with `native.prefix()`.
Captured configuration is reused for subsequent compiler and MPI witnesses.
QuEST and HDF5 discovery return data; explicit emission methods produce their
Cargo directives. The upstream MPI probe can emit directives; `native-doctor`
isolates that output from its JSON document.

`xtask` uses clap for commands, `cargo_metadata` for workspace metadata and
`rustc_version` for compiler metadata. Selected Cargo/compiler executables and
paths containing spaces remain supported. `native-doctor --json` emits one JSON
document, including setup failures. The existing fallible CMake launcher remains;
the inspected `cmake` API cannot provide the required fallible execution and
environment-clearing interfaces. HDF5 continues to use `hdf5-metno` and the
existing serial-HDF5 admission rules.

The small native-link test fixture sources reside in `quest-sys`, alongside the
audited FFI boundary. Those `quest-build` unit tests require the workspace layout;
ordinary library builds do not include these test-only assets.

## Owned matching scratch and safety

Prepared matching now owns a separate native scratch register on every supported
CPU/MPI deployment. `PreparedMatching::scratch_deployment()` returns
`RegisterDeployment` directly. Preparation accounts for this allocation before
publication and rejects insufficient rank or node budgets. Code that previously
tested for `None` should inspect the returned deployment instead.

QuEST's internal communication array is no longer exposed as application scratch.
The removed `quest-sys` communication-buffer staging functions have no replacement
raw-buffer API. Matching uses documented register cloning and checked amplitude
access. Historical memory reductions from borrowed communication storage do not
apply to the current two-register implementation; capacity must be reassessed.

All workspace Rust targets inherit `unsafe_code = "deny"`. Necessary audited
CXX/MPI FFI remains in `quest-sys`. Allocation measurements use safe interfaces
from established instrumentation crates, with thread-local and cross-thread
measurements kept distinct. The large-count MPI example uses `rustix` for its
address-space limit.

## Mathematical and structural changes

Complex, point, interval and AD polynomial evaluation share effective-support and
work admission. Stored coefficients retain their original support. Leading zero
padding in power bases no longer introduces a false pole, and negative powers
form a scaled reciprocal before exponentiation. Ordinary floating-point limits
still apply; this is not arbitrary-precision evaluation.

Finite QASM conditionals export explicit `bool` conversions, with negation for
the false polarity. Imported programs must still satisfy definite assignment.

Mechanical gate identities, construction, arity, parameter ordering and adjoints
use the existing registry; independent numerical formulas and verification remain
separate. Compiler search responsibilities and VM storage/frame/dispatch code
have separate modules. CFD reuses RK4 intermediate storage and shares checked
storage and normalization helpers while preserving accepted-state sequencing.

## Benchmark tooling

The workspace nextest configuration requires cargo-nextest 0.9.117 or newer for
`cargo nextest bench`. Production-stage adapters are opt-in through
`quest-qsp/benchmark-support`. Follow the
[replication commands](../../benchmarks/replication/README.md) for pinned inputs,
separate original/current targets, bounded execution and terminal-result accounting.
