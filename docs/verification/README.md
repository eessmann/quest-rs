# Verification records

These dated records describe the native configurations, checks and measurements
performed at the time. They are historical evidence, not current installation
instructions or a claim that every platform and feature combination has passed.
Use the [build guide](../../README.md#build), [Grace Hopper setup](../grace-hopper.md)
and [tutorial validation guide](../book/src/validation.md) for current commands.

| Record | Scope |
| --- | --- |
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

[Consolidation migration notes](2026-09-28-consolidation-migration.md) and
[optimizer migration notes](2026-09-28-optimization-roadmap-migration.md) explain
caller-visible changes. The [optimization reference map](2026-09-28-optimization-roadmap-implementation.md)
and [correctness regressions](2026-09-28-optimization-roadmap-review.md) link
technical contracts to source and tests.

Numerical results and compact environment/check summaries live under `data/`.
Complete historical captures remain available in Git history. Local paths and
compiler versions identify the measured setup; they are not portable defaults.
Numerical comparisons, mathematical certificates and timing measurements answer
different questions, and each record states its scope and limitations.

The [optimization fixtures](fixtures/optimization-roadmap/README.md),
[allocation probe](fixtures/consolidation/run.py) and
[native probability witness](fixtures/native-probability/README.md) provide
reproduction sources. `xtask check-native-consumers` also uses the maintained
native deployment and mode fixtures. Keep fixture sources when pruning results.
