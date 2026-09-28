# Optimization roadmap evidence

These are preserved local observations, including failed attempts. They are not
remote CI results. See the [results report](../../2026-09-28-optimization-roadmap-results.md)
for interpretation and the [reproduction fixtures](../../fixtures/optimization-roadmap/README.md)
for commands and measurement boundaries.

- `compiler-baseline-attempt1` is the historical version-1 corpus against merged
  main `a9bafa4`; its archived `src` is authoritative for that measurement.
- `compiler-current-*`, `compiler-roadmap-*` and `compiler-workers-*` record the
  corresponding implementation campaigns. `completion.json` establishes whether
  all expected case/stage/sample keys completed. A failed worker campaign remains
  failed even if most samples succeeded.
- `native-optimization-*` contains CPU/OpenMP/GPU state-vector/density timing rows.
  `native-mpi-optimization-*` retains every rank's rows and separate per-sample
  maximum-rank rows for two/four ranks.
- `native-deployment-attempt1` and `native-mpi-deployment-attempt1` are independent
  direct-bridge full-complex deployment witnesses. They precede the final
  optimizer timing campaigns and have their own archived source hashes.
- `final-validation-*` preserves each command, exit status, source hashes and log,
  including initial lint failures and the transient test-helper compile error.
- `native-consumers-*` records the four independent executable shapes with
  RUNPATH, resolved native closure and numerical execution. The later frozen
  check supersedes the earlier source snapshot for final-source claims.
- `hardware-inventory.json` retains the initial sandboxed GPU probe failure;
  `final-environment.json` records successful GPU/toolchain discovery outside the
  sandbox. The later actual GPU execution witnesses are the deployment evidence.
- `final-measurements-summary*.json` contains exact-key-checked medians and modeled
  reuse break-even. Failed campaigns retain raw rows and have no comparative
  medians. Read manifests and completion reasons alongside every number.

Absolute local paths in manifests record the actual command and dependency
origins; reruns use the fixture scripts' repository/output/target arguments.
Committed HEAD alone does not identify uncommitted measured sources: manifests
also retain the tracked diff digest and untracked source digests. The repository
implementation and independent source snapshots must accompany those manifests.
No binaries, Cargo target directories or duplicate full worktree patches are
included in this evidence directory. Original complete artifacts remain locally
under `.superpowers/sdd/2026-09-28-optimization-roadmap-implementation/`.

`sha256.json` hashes every other archived file. It is refreshed only when adding
new evidence; prior attempt directories and failures are preserved unchanged.

Raw `.log` transcripts retain their original whitespace; the local Git attribute
exempts those evidence files from source-code whitespace checks.
