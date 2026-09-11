# MPI admission before CLI worker creation

`Workers` is an explicit selection (`Count(NonZeroUsize)` or `Auto`). The default
remains one. Automatic selection resolves `available_parallelism` on the local
executing rank or distributed root, and the report uses the actual pool size.

Distributed `Cli::run` now enters an MPI-owned application scope first. The
admitted `MpiRuntime` and world communicator exist before the root resource
factory can construct a pool. Every rank agrees on thread support and on pool
construction success before cold input or native work. Only root creates a pool;
nonroot contexts remain sequential. A dispatch closure reuses the existing
communicator without placing MPI lifetimes inside `Context` or initializing MPI
twice. Lexical ownership drops the pool before the communicator and runtime.
No library MPI runtime implementation was changed.

Application wall totals start before MPI initialization and pool creation. Their
durations also appear as `mpi_environment` and `worker_pool`. Reports distinguish
automatic versus explicit selection, preserve deterministic catalogue ordering,
and retain per-synthesis borrowed execution policies.

Focused evidence:

- Three worker tests pass: parser/default/invalid inputs, explicit local pool,
  automatic local pool with reported actual parallelism.
- The parser test also passes without default features.
- Two-rank internal regression uses a test-only Rayon spawn handler to check
  admitted MPI thread support at actual OS-worker creation. Root spawns two,
  peers spawn zero. A normal invalid worker-scope error is agreed collectively;
  the communicator remains usable afterward. No production fault hook is used.
- Two/four-rank embedded and overlap CLI regressions pass with `--workers auto`
  and `--workers 2`, respectively. Scientific results and native-dispatch reports
  remain correct. Runtime was 0.53 s for the focused MPI integration test.
- Strict all-target Clippy passed with MPI/Rayon and without default features; rustfmt checks cover touched
  source/tests. MPI tests require local process-manager sockets and were run
  with authorized sandbox escalation, the ABI-checked Homebrew MPICC and
  `MPICH_CC=/usr/bin/gcc`. All Cargo commands use two build jobs.

No commits or root-manifest edits were made.
