# Measurement data

These directories retain numerical samples, certification outputs, completion
status and compact validation summaries supporting the
[verification records](../README.md). They describe the dated configurations,
not every current platform or feature combination.

- [QSP/QSVT](2026-09-11-qsvt/astro-catalog-summary.json): catalog certification,
  C++ benchmark samples and the Rust/C++ numerical comparison.
- [Runtime and compiler consolidation](2026-09-28-consolidation/metadata.json):
  allocation measurements, focused native witnesses and validation commands.
- [Optimization](2026-09-28-optimization-roadmap/README.md): compiler and native
  execution measurements, including failed or incomplete cases.
- [Grace Hopper](2026-09-29-grace-hopper/checks.json): workspace commands and
  standalone CPU/OpenMP/GPU consumer results.
- [Workspace quality, 2026-10-08](2026-10-08-workspace-quality/acceptance.json):
  source hashes, compact result excerpts, mathematical regressions and explicit
  capacity/dependency gates; full transcripts and benchmark samples remain local.

- [Native devenv follow-up](2026-10-08-workspace-quality/native-devenv/native-completion.json):
  separate native source accounting, exact-input Rust comparisons and scoped failure
  diagnoses; preserves the initial 811-row campaign.

Numerical JSONL samples are preserved unchanged. Measurement summaries retain
their numerical values. Metadata omits temporary checkout paths and duplicated
source listings; it is not a complete build-environment archive. Source hashes
identify the original measurements and are not checksums of today's source.
The full original captures are available in Git at `7dd3741`.

Keep generated build transcripts and temporary projects in `target/` or another
output directory. The [fixtures](../fixtures/) provide reusable source for new
measurements; record the selected source revision, configuration and limitations
with new results.

- [2026-09-29 architecture and synthesis acceptance](2026-09-29-architecture-synthesis/default-validation.json), with [focused feature/platform receipts](2026-09-29-architecture-synthesis/focused-validation.json).

- [2026-09-30 Linux architecture and synthesis acceptance](2026-09-30-linux-architecture-synthesis/README.md): source manifests, portability/correctness fixes, workspace, workers, QSP and separate MPI receipts.
