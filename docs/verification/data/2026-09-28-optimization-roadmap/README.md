# Optimization measurements

These are historical local measurements, including failed and incomplete cases.
See the [results](../../2026-09-28-optimization-roadmap-results.md) for interpretation
and the [fixtures](../../fixtures/optimization-roadmap/README.md) for reproduction
commands and measurement boundaries.

- `compiler-baseline-attempt1` contains the version-1 corpus measured against
  `a9bafa4`. `compiler-current-*`, `compiler-roadmap-*` and `compiler-workers-*`
  contain the corresponding implementation measurements.
- `native-optimization-*` contains CPU/OpenMP/GPU state-vector and density-matrix
  timing rows. `native-mpi-optimization-*` retains each rank's rows and the
  per-sample maximum-rank rows for two/four ranks.
- Each measurement directory retains `completion.json` and its numerical JSONL
  samples. Failed worker measurements remain failed even when most samples
  succeeded. `native-deployment-attempt1` and `native-mpi-deployment-attempt1`
  retain the separate direct-bridge deployment-check outcomes.
- `final-environment.json` records the toolchain and GPU configuration.
- `final-measurements-summary*.json` contains medians and modeled reuse break-even.
  Failed measurements retain raw rows and have no comparative medians. Summary
  keys identify the sibling measurement directories.

Attempt 1 exposed cache-dependent logical work accounting; attempt 2 follows
its regression fix. Use attempt 2 for the final implementation comparison.
Allocation counts describe requested storage, not RSS; small timing samples on
one machine do not establish a universal speedup.

The numerical samples are unchanged and the summaries reproduce the original
numerical values. Completion metadata retains outcomes and source identities but
omits temporary checkout paths, command transcripts and duplicated source lists.
The full original captures are available in Git at `7dd3741`; see the
[data retention notes](../README.md).
