# Linux architecture and synthesis validation

This is the Linux follow-up to the [architecture overhaul](2026-09-29-architecture-synthesis.md).
Validation uses an isolated source export on the Bazzite workstation; the existing
Linux checkout, primary macOS Cargo/devenv edits, and C++ working tree are preserved.
The default project environment remains CPU/OpenMP. MPI uses a separate pinned
QuEST installation and Cargo target directory. GPU execution is not covered here.

## Source and environment

The initial export contains 825 files from `codex/circuit-synthesis`, based on
`7148ccaa47e9e39506d62ba1e441d657040b83f8`. Its manifest SHA-256 is
`35ae2cd5c86d9352124f2be3b557d79a0fd2be9d53cda0608235e99f503fefaa`.
All exported files were verified after transfer. The [final manifest](data/2026-09-30-linux-architecture-synthesis/validation-source-manifest.json)
contains 826 files and 16 explicit source/configuration corrections; all final
files were verified remotely before acceptance. Its SHA-256 is
`59f8829a64737a6ea9a68c91ca8ebeaa5fb182b4bfc179988ada3de80798a184`.
The [correction patch](data/2026-09-30-linux-architecture-synthesis/linux-fixes.patch)
retains the changes against the original export.

The host runs x86_64 Linux on an AMD Ryzen 9 7950X (16 cores / 32 logical CPUs),
with 32 GB installed memory. The project lock selects Rust 1.100.0-nightly
(`6bb1652a0`, 2026-09-22), QuEST 4.3.0, and the native GCC 15.3.0 wrapper.
Clang 21.1.8 remains available explicitly for binding generation. Existing fish
startup output reports a missing `direnv`; validation commands run through explicit
Bash and project `devenv shell`. No login configuration or installed system profile
was changed.

The remote export is under
`/path/to/validation/architecture-synthesis/source`.
Receipts retain exact commands, exit statuses and logs. Long numerical test timings
were observed during validation with other activity on the host; they are not
isolated performance measurements or cross-platform speedup claims.

## QSP and native CLI

[QSP receipts](data/2026-09-30-linux-architecture-synthesis/qsp/acceptance.json)
record 94 all-feature tests and nine doctests passing. All three ordinarily ignored
large fixtures were explicitly run in release and passed:

- degree-8192 outward FFT certification at 256-bit precision;
- degree-8105 offline RHW in one 128-bit attempt, grid 65,536, with response upper
  bound `4.19907433604448073e-17`;
- degree-8105 serial versus 1/2/4-worker exact-value reproducibility in both
  real-parity Wx and generalized unit-circle response conventions, followed by
  independent frozen-export certification.

Offline RHW synthesis took 68.72 s and certification 44.18 s on this run. These
values include the run's concurrent-host conditions. The reproducibility check is
within one Linux host, not a claim of bit identity between macOS and Linux.
Inverse NLFT remains supported for generalized QSP as well as the Wx route.
Native CLI offline-synthesis/Rayon tests passed 25/25; QSP all-feature/all-target
and native CLI/IO strict Clippy both passed.

## MPI

[MPI receipts and reproduction](data/2026-09-30-linux-architecture-synthesis/mpi/README.md)
retain the separate pinned QuEST MPI/SUBCOMM build, installed consumer check,
normal two-rank launcher, independent native/MPICC ABI witnesses, seven collective
tests, and strict native MPI lint. The initial shell selected bare `clang++`
instead of its configured GCC wrapper; the resulting ABI witness could not load
`libstdc++.so.6`. The receipts preserve this failure and the compiler-selection
probes. No synthetic topology or loader-path override is needed on Linux.

## Portability and optimizer corrections

Linux-only facade and worker tests still used removed builder names, direct
transitive client imports, or the old concrete error payload. Their migration
preserves the original forged-response and exact-budget assertions. Protocol
fixtures now declare their direct development dependency; client access uses the
public facade export.

The new rotation-generator boundary initially wrapped process failures twice.
Beam search therefore treated a recoverable candidate decline as a fatal error
instead of continuing to another generator. The conversion now retains the
process error's scheduling category, while direct synthesis cancellation and
certificate rejection keep their distinct generator categories. A portable
pass-level regression and the original Linux tests cover these distinctions.
The full all-feature facade suite passed 249 tests, and strict compiler/facade
Clippy passed.

The project environment now explicitly selects its native stdenv and disables
devenv's automatic Rust Clang-linker selection. Native builds use the GCC wrapper
on Linux and the Clang wrapper on Darwin. Clang for binding generation remains
explicit. This avoids restoring compiler variables through a shell hook.
Direct, facade, wrapped and renamed native consumers passed their CPU/OpenMP
numerical and deployment checks on both platforms with this configuration.

## Synthesis search correction

The real process test for `Rz(pi/4)`, seed 1234 and epsilon `1e-12`, exceeded its
unchanged 30-second timeout in both debug and release. This was a search defect,
not a reason to equate the full-phase rotation with T or loosen its tolerance.
The weighted lattice sphere contained very large branches whose points could
not satisfy both physical and algebraic-conjugate unit-disk constraints.

The enumerator now intersects the original sphere with a necessary exact
coefficient-norm ball. A second necessary radial bound uses the actual target
and square-root enclosure widths; exact Cauchy–Schwarz bounds prune a partial
branch only when its remaining coordinates cannot reach the feasible radial
region. Strict inequalities retain boundary points. An exhaustive small-grid
oracle compares all feasible points in the original sphere against the new
enumerator; independent certification still checks every published candidate.

The regression also checks full scalar phase, requested epsilon, repeated
sequence/work determinism, and failure at one fewer work unit. The modeled grid
scratch reservation is kept separate from candidate working storage while the
grid remains live. These are explicit admission models, not a universal allocator
or measured-heap guarantee.

## Workspace acceptance and measurements

[Workspace receipts](data/2026-09-30-linux-architecture-synthesis/workspace/acceptance.json)
select the corrected-source final run explicitly. Earlier failed commands remain
available as diagnostic evidence.

| Check | Observed result |
| --- | --- |
| Locked workspace build | Passed |
| Workspace nextest, no fail-fast | 838 passed, one ignored fixture, 147 binaries |
| Workspace doctests | 49 passed, one ignored |
| Workspace all-target strict Clippy | Passed with `-D warnings` |
| Formatting | Passed |
| Binding freshness | Passed, no generated changes |
| CPU/OpenMP native consumers | Direct, facade, wrapped and renamed consumers passed; state-vector/density numerical checks and complete ELF RUNPATH/dependency closure |
| Book | mdBook 0.5.4 build passed using the pinned temporary Nix package |
| All-feature facade | 249 passed, no skips; strict compiler/facade lint passed |
| Separate MPI/SUBCOMM | Seven collective tests, normal launcher, matching ABI witnesses and strict lint passed |
| Explicit large QSP fixtures | All three passed in release |

The live-grid reservation and pruning also passed 62 mathematical/synthesis tests
and one doctest on Darwin, with strict synthesis lint. The [independent review](data/2026-09-30-linux-architecture-synthesis/synthesis/grid-pruning-review.md)
records the proof obligations, seven-angle exhaustive oracle and remaining model
limits. The original worker timeout was not increased.

[Refreshed macOS performance receipts](../../benchmarks/architecture/2026-09-30-portability/summary.json)
bind 21 process measurements to the corrected source. Reused preparation reduced
the recorded execution-plus-preparation time by about 5–9%, with zero amplitude
difference in all nine comparisons. At epsilon `1e-12`, full-phase `Rz(pi/7)`
synthesis plus acceptance had median 580.62 ms and independent replay 0.665 ms;
all three runs produced 297 gates, 122 T/T-dagger gates and 1,251,191 logical work
units. These are modest local measurements, not a cross-platform performance
comparison. Earlier measurements remain unchanged as historical receipts.

## Optional worker execution

[Worker receipts](data/2026-09-30-linux-architecture-synthesis/workers/README.md)
record the final run in a separate Cargo target directory. All seven stages
exited successfully:

- 40 worker tests: 28 unit tests and 12 actual process tests, with synthesis,
  ZX and MITM features enabled;
- 13 client tests, including process limits/cleanup and forged parent evidence;
- six compiler worker-contract tests;
- strict worker/client all-target Clippy;
- the targeted release phase regression, passing in 0.21 s under the unchanged
  30-second process limit;
- the enabled worker binary build and one native execution test using that
  explicit binary for state-vector and density-matrix comparisons.

Counts are reported by stage rather than added into a claim of unique coverage.
The native parent/child output represents one logical test. Actual subprocess
coverage includes synthesis and ZX; MITM has engine-unit and parent-fixture
coverage. The memory-limit prerequisite was also witnessed with an allocation
that failed under the effective address-space limit. Process limits apply per
process with group cleanup; this is not a cgroup or seccomp sandbox claim.

A shared-target trial was invalidated when a simultaneous default workspace
build replaced the enabled worker executable; its failures are preserved and it
is not used as final worker evidence. The successful run uses its own target
directory. All final acceptance commands are complete. GPU execution remains
unvalidated for this overhaul.
