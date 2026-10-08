# Verification records

These dated records describe the native configurations, checks and measurements
performed at the time. They are historical evidence, not current installation
instructions or a claim that every platform and feature combination has passed.
Use the [build guide](../../README.md#build), [Grace Hopper setup](../grace-hopper.md)
and [tutorial validation guide](../book/src/validation.md) for current commands.

| Record | Scope |
| --- | --- |
| [Large-polynomial workspace acceptance, 2026-10-08](2026-10-08-large-polynomial-workspace.md) | Final shared-resource source: default/all-feature tests, doctests, strict all-target Clippy and explicit exclusions |
| [Large-polynomial resource and capacity, 2026-10-08](2026-10-08-large-polynomial.md) | Preserved native inverse checks, independent forward up to degree one million, workspace reuse and modelled/RSS observations |
| [Native benchmark devenv follow-up, 2026-10-08](2026-10-08-native-devenv.md) | Unchanged C++ source suites, exact inverse fixtures and matched original/current nextest measurements |
| [Workspace quality and consolidation, 2026-10-08](2026-10-08-workspace-quality.md) | Local acceptance and benchmark accounting; [23-package review](2026-10-08-workspace-review.md) and [migration notes](2026-10-08-workspace-migration.md) |
| [Portable native builds and Cirrus, 2026-10-06](2026-10-06-portable-cirrus.md) | Compiler/module discovery, ordered native links, shared MPI supervision, central HDF5, exclusive hybrid jobs and explicit capacity gates |
| [Reviewed portable-native source, 2026-10-06](2026-10-06-portable-reviewed.md) | HDF5 fallback, fatal-drop negative control, safe Rust supervision and separate acceptance ledger for the corrected source |
| [Explicit scaling mode, 2026-10-06](2026-10-06-scaling-only.md) | Observed process limits, managed budgets, focused local MPI checks and a separately frozen GNU/Cray release campaign; capacity remains open |
| [Torc on Cirrus, 2026-10-06](2026-10-06-torc-cirrus.md) | Earlier standalone GNU/Cray build workflows and source-specific acceptance; the later [scaling campaign](2026-10-06-scaling-only.md) verifies a server-managed eight-node Cray runtime |
| [Native discovery follow-up, 2026-10-06](2026-10-06-native-discovery-followup.md) | Bounded whole-archive file-kind admission, HDF5 pkg-config parity, independent regressions and downstream Linux verification |
| [Cargo host policy on Cirrus Cray, 2026-10-06](2026-10-06-cargo-host-policy.md) | Documented host/target compiler configuration, expected linker failure, executed subnormal checks and source-specific workspace acceptance |
| [MPI request ABI admission, 2026-10-06](2026-10-06-mpi-request-abi.md) | Independent request size/alignment witnesses, negative controls, local Linux verification and current Cirrus smoke evidence; macOS unrun |
| [Linux native Clang and MPI, 2026-10-06](2026-10-06-native-clang.md) | Clang-built QuEST with LLVM OpenMP, default/all-feature workspace validation, independent MPI consumer and separate native installation loader outcomes |
| [Linux native GNU, 2026-10-06](2026-10-06-native-gnu.md) | Frozen-source GCC workspace campaign, CPU/OpenMP consumers in normal and isolated loader modes, and independent local MPI at 1/2/4/8 ranks |
| [Cirrus sparse capacity and scaling, 2026-10-06](2026-10-06-cirrus-capacity.md) | Original-input byte counts, multi-host placement, enforced process envelopes and separate execution/capacity outcomes |
| [Real large-count MPI transport, 2026-10-06](2026-10-06-large-count-mpi.md) | GNU and Cray two-node transfers above the signed-int count boundary, full-byte verification and separate capacity limits |
| [Sparse capacity follow-up, 2026-10-06](2026-10-06-sparse-capacity-followup.md) | Producer lifetime accounting, native array telemetry and resource-only native execution receipts |
| [Matching communication-buffer reuse, 2026-10-06](2026-10-06-matching-buffer-reuse.md) | Reduced native array storage, whole-unitary and failure regressions, local affinity diagnosis and remaining admission limits |
| [Cirrus node enforcement diagnostic, 2026-10-06](2026-10-06-node-enforcement.md) | Read-only compute-node cgroup observations, site memory limits, HugeTLB coverage uncertainty and remaining capacity gate |
| [Persisted matching load phases, 2026-10-06](2026-10-06-matching-load-phases.md) | Measured collective directory costs and completed GNU/Cray comparisons separating portable admission from native loading |
| [PennyLane and standard formats, 2026-10-03](2026-10-03-pennylane-formats.md) | Owned runtime HDF5 catalog, provenance, typed JSON, bounded output, TOML/CSV/Glaze migrations and offline distribution checks |
| [All-feature QSP optimization, 2026-10-02](2026-10-02-qsp-optimization.md) | Installed-system and clean-devenv validation, algorithm-specific completion, root transfer pruning and shared FFT measurements |
| [Dashu migration, 2026-10-02](2026-10-02-dashu-migration.md) | Pure-Rust arbitrary precision, canonical exact interchange, dependency cleanup and numerical/performance validation |
| [Static architecture, 2026-10-02](2026-10-02-static-architecture.md) | Generic functions/arithmetic, MP intervals, unified Remez/root coverage, compiler/native consolidation and matched performance |
| [Mathematical audit, 2026-10-01](2026-10-01-mathematical-audit.md) | Paper and C++ capability comparison, QSP/QSVT defaults, mathematical contracts and nightly generic interfaces |
| [Linux architecture and synthesis, 2026-09-30](2026-09-30-linux-architecture-synthesis.md) | Linux workspace, bounded processes, CPU/OpenMP/MPI, large QSP fixtures and portability corrections |
| [Architecture and synthesis, 2026-09-29](2026-09-29-architecture-synthesis.md) | Unified program lifecycle, bounded Rust synthesis, RHW/NLFT, payload-bound QSVT evidence and pinned reference comparisons |
| [Grace Hopper, 2026-09-29](2026-09-29-grace-hopper.md) | Manual/Spack setup, CPU/OpenMP/GPU execution, dependency closure and workspace checks |
| [Optimization, 2026-09-28](2026-09-28-optimization-roadmap-results.md) | Compiler and native measurements, feature coverage and cost-model limits |
| [Consolidation, 2026-09-28](2026-09-28-consolidation-review.md) | Correctness regressions, allocation measurements and validation |
| [QSP/QSVT, 2026-09-11](2026-09-11-qsvt-port.md) | Catalog certificates, reference comparisons and native execution |
| [QSP/QSVT documentation, 2026-09-11](2026-09-11-qsp-qsvt-documentation.md) | Executable examples, rustdoc and book checks |
| [Environment lifecycle, 2026-09-11](2026-09-11-raii-environment.md) | RAII cleanup, retirement and ownership regressions |
| [Native packaging, 2026-09-10](2026-09-10-native-cmake.md) | CMake imported targets, installed runtime paths and downstream consumers |
| [Compiler and runtime, 2026-09-10](2026-09-10-m6-m11.md) | OpenQASM profile, SSA, optional workers and numerical checks |

[Static architecture migration](2026-10-02-static-architecture-migration.md),
[consolidation migration notes](2026-09-28-consolidation-migration.md) and
[optimizer migration notes](2026-09-28-optimization-roadmap-migration.md) explain
caller-visible changes. The [optimization reference map](2026-09-28-optimization-roadmap-implementation.md)
and [correctness regressions](2026-09-28-optimization-roadmap-review.md) link
technical contracts to source and tests.

Numerical results and compact environment/check summaries live under `data/`.
Complete historical captures remain available in Git history. Published local
paths and user identifiers are replaced with generic placeholders, including
in logs and environment summaries. Commands retain their options and results;
recorded hashes identify the original captures. Compiler versions describe the
measured setup and are not portable defaults.
Numerical comparisons, mathematical certificates and timing measurements answer
different questions, and each record states its scope and limitations.

The [optimization fixtures](fixtures/optimization-roadmap/README.md),
[allocation probe](fixtures/consolidation/run.py) and
[native probability witness](fixtures/native-probability/README.md) provide
reproduction sources. `xtask check-native-consumers` also uses the maintained
native deployment and mode fixtures. Keep fixture sources when pruning results.
