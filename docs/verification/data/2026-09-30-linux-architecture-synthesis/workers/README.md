# Linux worker and process acceptance

These receipts were collected from the authorized isolated Bazzite x86_64
checkout at `/path/to/validation/architecture-synthesis/source`.
The final source is bound by the parent [826-file validation manifest](../validation-source-manifest.json),
SHA256 `59f8829a64737a6ea9a68c91ca8ebeaa5fb182b4bfc179988ada3de80798a184`.
[synthesis-final-overlay.sha256](synthesis-final-overlay.sha256) records the eight
files changed by this worker/synthesis follow-up; all eight verified after the
final run. Original export/archive manifests were preserved.

## Final accepted run

[run-worker-isolated.sh](run-worker-isolated.sh) records the exact environment
and command sequence. It uses the pinned project environment through
`devenv --no-tui shell --`, `CARGO_BUILD_JOBS=6`, serial Rust tests, and a separate
`CARGO_TARGET_DIR` ending in `/worker-target`. Other workspace validation ran
concurrently, but could not replace this run's worker executable. No further
test run was needed after these commands completed.

| Stage | Tests | Result and log |
|---|---:|---|
| Worker, `synthesis,zx,mitm` | 40: 28 engine units and 12 actual-process integrations | [exit 0](worker-isolated-engines-final.exit), [log](worker-isolated-engines-final.log) |
| Client | 13: direct 1, process ownership 1, process contracts 9, forged MITM responses 2 | [exit 0](worker-isolated-client-final.exit), [log](worker-isolated-client-final.log) |
| Compiler worker contract | 6 | [exit 0](worker-isolated-compiler-final.exit), [log](worker-isolated-compiler-final.log) |
| Client/worker strict all-target Clippy, all three engines enabled | — | [exit 0](worker-isolated-clippy-final.exit), [log](worker-isolated-clippy-final.log) |
| Release quarter-pi process regression | 1, repeated from the worker suite above | [exit 0](worker-isolated-release-phase-final.exit), [log](worker-isolated-release-phase-final.log) |
| Feature-enabled native worker build | — | [exit 0](worker-isolated-native-build-final.exit), [log](worker-isolated-native-build-final.log) |
| Actual native QuEST state-vector/density comparison through the process worker | 1 logical test; parent and child each print the same test result | [exit 0](worker-isolated-native-final.exit), [log](worker-isolated-native-final.log) |

Each stage also has a corresponding `.command` file; the script supplies the
common environment. [worker-isolated-final.done](worker-isolated-final.done)
records completion. Counts are listed separately to avoid counting the repeated
release test or native child twice. MITM has engine-unit and forged-response
coverage here; no real-process MITM integration test is claimed.

The 12 actual-process worker integrations comprise two finite-circuit tests,
five structured-program tests, three synthesis tests, and two ZX tests. The
native stage explicitly sets `QUEST_TUTORIAL_WORKER` to the isolated debug
worker; it therefore does not take the fixture's missing-environment early
return. It compares both state-vector and density-matrix outputs for signed
controlled Ry synthesis at 1e-12.

## Failures retained and resolved

[worker-red.log](worker-red.log) records the Linux-only stale builder API compile
failure. The finite fixture now uses `QuantumRegionBuilder` and explicit pass
traits. [worker-compiler.log](worker-compiler.log) records the stale direct
client import; its replacement uses the public optimizer reexport without a
new dependency. Intermediate compile/lint attempts remain alongside the final
logs rather than being overwritten.

[worker-engines2.log](worker-engines2.log) records a genuine 30-second debug
timeout for Rz(pi/4), epsilon 1e-12, seed 1234.
[worker-release-phase.log](worker-release-phase.log) independently reproduces
the same timeout in release mode. Exact coefficient-ball and radial pruning,
with [independent mathematical review and local acceptance](../synthesis/README.md),
resolved it. The final debug process suite passes under the original cap, and
the unchanged release regression completes in 0.21 seconds. This is a specific
regression observation, not a general performance claim.

The shared-target `worker-engines-final` attempt is **not accepted evidence**:
a concurrent default-feature workspace build replaced its worker executable,
causing capability-disabled responses during otherwise enabled tests. See
[worker-shared-target-interference.txt](worker-shared-target-interference.txt).
The complete isolated matrix above supersedes that attempt and the other
shared-target `*-final` runs.

## Process prerequisites and limits

[worker-prerequisites.log](worker-prerequisites.log) confirms `/usr/bin/prlimit`,
`/bin/sh` utilities and the existing `/usr/bin/python3` fixture dependency.
An independent prlimit probe reports 524,288 KiB address space and 30 CPU
seconds; the 700 MiB allocation emits `MemoryError` and exits 1. This avoids
mistaking a missing interpreter for successful memory enforcement.

The client uses per-process address-space/CPU limits, bounded streams, wall-time
limits and owned process-group cleanup. It does not use or validate seccomp or
aggregate cgroup enforcement. These receipts establish local Linux CPU process
and native execution; they do not claim Linux MPI, GPU, or cluster validation.

[SHA256SUMS](SHA256SUMS) binds the collected logs, command files, exit files,
scripts and this explanation. Source hashes are separately verifiable from the
repository root using `sha256sum -c .../synthesis-final-overlay.sha256`.
