# Native MPI architecture validation — aarch64 macOS

This receipt builds QuEST 4.3.0 with MPI and subcommunicators, compiles the real
Rust MPI feature against that installation, verifies the rsmpi/QuEST MPI ABI,
and runs the seven collective runtime tests on two local MPI ranks. It does not
claim Linux, GPU, cluster, placement, or distributed performance validation.
The default CPU/OpenMP development profile is unchanged.

## Reproduction

Run from the repository root on the recorded aarch64-darwin host:

```sh
nix build --impure --file docs/verification/architecture-mpi/2026-09-29/build.nix quest mpi --no-link --print-out-paths
devenv shell -- bash docs/verification/architecture-mpi/2026-09-29/run.sh clippy -p quest-rs --features mpi,qsvt --all-targets --message-format short
devenv shell -- bash docs/verification/architecture-mpi/2026-09-29/run.sh test -p quest-rs --features mpi,qsvt --test collective_runtime -- --test-threads=1
```

[build.nix](build.nix) uses the repository's existing `nix/quest.nix`, pinned
nixpkgs `6774f7bc253789b113a4f39285dc0fa100abeacc`, and QuEST source
`503552065045eaf89baba85e6cd6aad728525554`. It adds OpenMPI 5.0.10 and changes
only the MPI/subcommunicator CMake options. [original-build.nix](original-build.nix)
preserves the exact original absolute-path expression; the reproducible copy
uses the equivalent repository-relative path. [build.log](build.log) records
the successful Nix build and installed-consumer validation. [paths.txt](paths.txt)
records returned store outputs; the OpenMPI development output used by Rust is
explicit in [run.sh](run.sh). The script uses a separate Cargo target directory
and explicitly supplies the MPI compiler, pkg-config directory and PATH.

[clippy-final.log](clippy-final.log) records successful all-target strict
Clippy with `mpi,qsvt`. Its build output separately confirms the loaded QuEST
MPI library and rsmpi MPICC ABI match. Compilation and ABI matching alone are
not runtime evidence. [runtime-final.log](runtime-final.log) records the seven
passing two-rank tests; [runtime-reproduced.log](runtime-reproduced.log) records
a subsequent run through the persisted reproduction script. Tests cover Bell
execution, oracle/projection execution, subgroup behavior, rank admission
failures, semantic agreement, published classical result/step agreement, and
recoverable rejection of an unsupported native communication-buffer size.

## Scoped launcher workaround

The unmodified launcher smoke command
`mpiexec -n 2 /usr/bin/true` failed with status 139 in PRRTE's Darwin hwloc
topology discovery; see [launcher-smoke.log](launcher-smoke.log).
`HWLOC_COMPONENTS=-darwin` alone still failed with status 213; see
[launcher-no-darwin.log](launcher-no-darwin.log). This is a launcher failure,
before Rust or QuEST execution.

The receipt-local [bin/mpiexec](bin/mpiexec) sets
`HWLOC_SYNTHETIC='package:1 core:10 pu:1'`, `HWLOC_COMPONENTS=synthetic`, and
`--bind-to none`. The smoke command then exited zero (its empty output is
[launcher-synthetic.log](launcher-synthetic.log)); all runtime evidence above
uses this scoped override. Synthetic topology is suitable for this local
correctness check but provides no evidence for real topology discovery or CPU
placement. No global launcher or operating-system configuration was changed.

[result-regression.log](result-regression.log) preserves the initially failing
new regression: a one-qubit dense gate split across two ranks reaches QuEST
with only one communication-buffer amplitude. The native preflight now rejects
that case before collective native entry; the final test also verifies that
the communicator remains usable afterward.

All files are checksummed in [SHA256SUMS](SHA256SUMS); source/build-input hashes
are in [source-inputs.sha256](source-inputs.sha256). These are working-tree
receipts, not evidence that the recorded Git HEAD includes uncommitted changes.
