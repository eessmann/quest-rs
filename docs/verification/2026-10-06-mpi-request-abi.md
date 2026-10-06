# MPI request ABI admission

The MPI ABI checks now compare `MPI_Request` size and alignment at build time
and runtime. This small change is verified locally on Linux with the system
MPICH toolchain and the installed QuEST 4.3.0 package. It adds no restricted
matching-state API, changes no native QuEST source, and does not certify a native
storage or message-partition protocol.

The [machine-readable summary](data/2026-10-06-mpi-request-abi/summary.json)
records the four changed ABI source hashes, exact verification commands, exit
statuses and raw-log hashes. The local artifacts remain under
`target/mpi-request-abi/`; their workstation-specific environment paths are not
copied into this record.

## Checked boundary

The native C++ witness uses the installed QuEST CMake target. An independent C
witness uses the MPI recipe selected for rsmpi. Both report request size and
alignment, with the existing library identity and ABI signature checks retained.
The runtime Rust and C++ witnesses contain eleven fields, adding request size
and alignment at positions 9 and 10. Runtime admission validates these fields
before MPI initialization or rsmpi initialization-state queries.

Production admission and the mismatch tests share the private safe Rust helper
`validate_rsmpi_layout`. The helper retains the existing documented read of the
FFI thread-support constant. This refactoring removes duplicate unsafe reads
from the tests; it does not claim that the FFI crate is entirely unsafe-free.

The compiled negative-control regression retains the real MPI headers and loaded
library while replacing only the request type seen by the witness. Independent
review compiled and ran all six C/C++ cases: a 16-byte, alignment-1 baseline; a
32-byte, alignment-1 size mutation; and a 16-byte, alignment-8 alignment mutation.
Both languages distinguished each mutation. Runtime tests independently mutate
request size and alignment and require rejection before initialization; the
existing status-alignment regression also passes through the shared helper.

## Saved local verification

All eight stages in the saved verification driver returned status zero:

| Stage | Scope | Result |
| --- | --- | --- |
| Nextest with MPI | `quest-build`, `quest-sys`, MPI feature | 111 passed, zero skipped |
| Default Nextest | `quest-sys` | 41 passed, zero skipped |
| Doctests | `quest-sys`, MPI feature | 5 passed, zero ignored |
| Workspace check | All workspace crates and features | Passed |
| Strict Clippy | `quest-build`, `quest-sys`, all targets, MPI feature | Passed with `-D warnings` |
| Binding freshness | `xtask generate-quest-bindings --check` | Passed |
| Formatting | `cargo fmt --all -- --check` | Passed |
| Whitespace | `git diff --check` | Passed |

Cargo verification used `--locked --offline`. Workspace checking and binding
generation retain the existing `quest-polynomial` Rust trait-solver compatibility
warning; the scoped strict Clippy stage passed. These are focused ABI package
tests and workspace compilation, not a new full-workspace test-suite result.

## Unrun environments and retained limits

The frozen source snapshot
`9adbef476110e8425bacee15882c006464a6d95b6a54ccb2269621e29af68233`
predates this ABI change. Its Cirrus GNU/Cray results do **not** validate the
current delta. The subsequent [current-source Cirrus smoke campaign](2026-10-06-torc-cirrus.md#direct-mpi-runtime-follow-up)
does: both compiler lanes passed eight selected ABI/discovery tests, including
the compiled request mutations without taking the skip branch, plus actual
one-/two-node native MPI consumers and lifecycle tests. macOS and other MPI
implementations remain unrun. The original independent negative-control observations are preserved in the
summary; their separate reviewer executions have tool-transcript evidence rather
than an additional saved raw log.

Request layout compatibility does not prove native allocation bounds, message
partition semantics, or whole-node memory enforcement. Restricted-state
admission and strict capacity remain open.
