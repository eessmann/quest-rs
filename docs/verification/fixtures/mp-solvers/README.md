# Complete arbitrary-precision workloads

Run this fixture inside the repository's pinned `devenv shell`, after correctness
checks and without competing builds. Supply the immutable pre-port source archive
from the Dashu evidence directory, extracted into a separate scratch directory.

```sh
devenv shell -- python3 docs/verification/fixtures/mp-solvers/run.py \
  --baseline /path/to/extracted/pre-port-source \
  --repository . --output /path/to/new/receipts
```

Identical source builds against the frozen Astro-based static architecture and
the current Dashu architecture. The fixture exercises complete MP Remez and
offline QSP solves, split Newton contraction and exact circuit synthesis. Every
workload checks its numerical or exact result; Remez requests the same uniform
bound and minimax gap, and QSP checks the actual binary64 export's certificate.
Three trials interleave the two binaries at 128 and 256 bits. These are matched
accuracy comparisons, not claims that every intermediate represented value is
identical across libraries.

Timings include the correctness assertions and exact reconstruction in each
iteration. They are allocator-instrumented: atomic counting adds overhead to
allocation paths in both implementations. The allocator counts Rust allocations/reallocations and peak additional live
bytes over a warmed starting point; it excludes allocator metadata and stack
storage. Both compared implementations are pure Rust. Process RSS, release build
time, executable size and hashes are separate receipts. Source hashes before and
after the run must agree. The project-level fixture under
`../static-architecture/project` additionally measures compiler, binary64 QSP and
native execution, including clean/unchanged/touched-source build cost.

Allocation counts are totals over the stated iteration count. Peaks omit internal
reallocation overlap and native allocations. Exact synthesis's precision label
configures its limit; both settings execute the same exact integer workload.

`decimal_reference.py OUTPUT.json` regenerates external 210-digit CPython
decimal/libmpdec witnesses for exp/ln/sqrt. Frozen fixtures are consumed by the
Rust tests without a Python dependency; the exact rational series oracle remains
separate. Historical Rug primitive results are not whole-solver measurements.
