# Linux validation receipts

These receipts describe the 2026-09-30 Bazzite x86_64 Linux validation of the
architecture/synthesis overhaul. See the [verification record](../../2026-09-30-linux-architecture-synthesis.md)
for findings, corrections and platform limits.

- [Workspace acceptance](workspace/acceptance.json): 838 tests, 49 doctests,
  strict lint, formatting, bindings, CPU/OpenMP consumers and book build.
- [QSP/CLI](qsp/acceptance.json): all features, three explicit large release
  fixtures, native CLI and strict lint.
- [Compiler selection](environment/compiler-selection.json): corrected native
  compiler setup and native consumers on Linux and Darwin.
- [MPI](mpi/README.md): isolated pinned MPI/SUBCOMM build, matching ABI witnesses,
  normal launcher, seven collective tests and strict lint.
- [Compiler error classification](compiler/README.md): portable regression and
  the unchanged Linux optimizer tests, including the 249-test facade run.
- [Optional workers](workers/README.md): isolated actual process execution,
  client limits, compiler adapters, strict lint and native state-vector/density checks.
- [Synthesis](synthesis/README.md): exact pruning, resource reservation,
  independent review and local mathematical acceptance.

`source-manifest.json` identifies the original 825-file export. The final
`validation-source-manifest.json` identifies 826 files and the 16 synchronized
source/configuration changes; its SHA-256 is
`59f8829a64737a6ea9a68c91ca8ebeaa5fb182b4bfc179988ada3de80798a184`.
`linux-fixes.patch` preserves those corrections against the original export.
Verification records written afterward are outside the execution snapshot.
Individual component receipts bind their own earlier/final run to relevant
source hashes, so a successful earlier run does not silently acquire a new source
identity. Source hashes identify evidence; they are not mathematical certificates.

Failed initial runs, diagnostic probes and successful final runs are retained
with distinct names. Use the final acceptance list and individual command exits,
not an outer shell success or a process count, when assessing completion.

The workspace receipt also includes an explicit `-D warnings` optional-worker
lint run using the isolated worker target, supplementing the configured-lint
command in the original worker driver.
