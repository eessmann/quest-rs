# Successful QSVT native dispatch reporting

`MassObservation::native_dispatches()` and
`OverlapObservation::native_dispatches()` return `NativeDispatchReport` with
checked `total`, `circuit`, `projection`, `readout`, and `state_management`
counts. These are successful-run native API calls per process/rank, independent
of semantic queries and oriented source applications. Collective wrappers carry
the same report; CLI JSON places it at `mass.native_dispatches` with an explicit
scope and exclusions.

Admission counts the actual fixed lowered dispatch schedule, including retained
oracle calls memoized by shared body and inherited negative-control count.
Adjoints reverse order and angles without changing these dispatch counts. Gate
decomposition accounts for identity, U, square-root X, scalar phases and signed
phase toggles. Numerical state-vector matrices cost one native API dispatch.
Projection counts use the admitted representation, so optimized coordinate cubes
and dense/diagonal projections are distinguished. Hadamard branch selection,
scratch clone/add operations, probability queries and readout gates are included.
Memo scratch reserves 256 modeled bytes per admitted oracle control profile and
is dropped before native preparation. No runtime/global counters were added.

The scope excludes setup, run admission/fingerprint queries, MPI agreement,
caller initialization, snapshots, and later conditioning. Errors publish no
complete count. Native API dispatches do not claim to count backend kernels or
MPI messages. Existing runtime failure-prefix semantics and consuming stages
remain intact.

Independent hand counts and evidence:

- A shared oracle with U, controlled global phase, Sx and a complex numerical
  matrix on nonsorted operands checks negative-control toggles. Its standard
  transform has 77 circuit calls, 82 total embedded calls and 96 total overlap
  calls; native outputs agree with the independent complex matrix reference.
- Shared forward/adjoint pure regression separately checks 40 circuit calls,
  optional-bridge totals and overflow rejection.
- Dense imported CLI fixture passes with 16 embedded and 30 overlap calls.
- The same 16/30 per-rank counts pass root-synthesis CLI runs at two and four
  ranks. Initial sandbox run could not open MPICH process-manager sockets;
  authorized unsandboxed rerun passed in 0.51 s.
- Facade library/tests and MPI CLI all-target strict Clippy passed with
  `-D warnings`; touched-file rustfmt checks passed. All commands used
  `CARGO_BUILD_JOBS=2` and the installed QuEST at
  `/var/home/erich/Projects/opt/quest`. MPI used the ABI-checked Homebrew MPICC
  with `MPICH_CC=/usr/bin/gcc`; CLI used the local serial HDF5 installation.

No commit was created. No claim of partial-error call counts or per-kernel
profiling is made.
