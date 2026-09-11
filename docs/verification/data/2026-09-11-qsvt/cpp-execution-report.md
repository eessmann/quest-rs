# Fresh pinned C++ native execution comparison

Completed a new public-API adapter at `benchmarks/reference/cpp_execution.cpp` and
tracked standalone build recipe at `benchmarks/reference/CMakeLists.txt`. The
adjacent README contains the reproduction command and measured table. The build
compiled all 37 initial target steps from the clean external C++ source at
7fe7f740579b03c52a8cf48be6a31268b029c19f, using the installed QuEST target under
`/var/home/erich/Projects/opt/quest`. No existing QSVT archives were reused; only
third-party dependency packages and a hash-verified CPM bootstrap were reused.
The external checkout and native installation were not modified. A second empty
build directory successfully configured directly from the tracked CMake recipe.

The adapter explicitly forces local CPU and single-thread native execution. It
constructs the block encoding for 0.3+0.4i with normalization 1 and Wx phases
[0.1,0.2,0.2,0.1], prepares the public standard-transform circuit, derives exact
coordinate projections from public layout metadata, and executes it. Public
Hadamard overlap uses input=reference=[1]. Both public paths are checked before
timing. Every timing iteration checks operation results; invalid execution does
not become a fast benchmark sample.

The matching Rust benchmark output was read directly from
`rust-qsvt-benchmark.log`. Comparison in `cpp-rust-execution-comparison.json`:

- Full complex retained amplitude difference 1.7554167342883506e-16.
- Retained mass difference 4.37069000783219e-19.
- Full complex Hadamard overlap difference 3.1401849173675503e-16.
- Hadamard total success mass identical 0.5000076859045433.

This is differential numerical evidence for one local fixture, not an enclosure
certificate, high-degree execution test, distributed run, or GPU validation.

Five separate C++ process runs, each averaging 1000 repeats, give median durations
in microseconds: construction 22.97, combined lowering/native preparation 36.28,
execution with coordinate projections and three mass reads 51.39, direct native
conditioning 1.301, public Hadamard observation 83.62. Parent's Rust Criterion
estimates are construction 9.274, admission 1.864, native preparation 12.681,
execution 139.45, consuming conditioning 1.229. C++ has no matching separate public
standard-transform admission stage; benchmark methods, ownership turnover and
preflight contracts differ. In this tiny fixture Rust repeated execution is
slower. No broad performance claim is made.

Artifacts: configure-command/configure/build/rebuild/final-build logs; full five
run JSONL; final result JSON; comparison JSON; compiler, dependency resolution,
source status, native-library and adapter hashes in provenance JSON. Initial
build warnings from upstream sources/Eigen remain visible and were not silenced.
Native dynamic resolution is verified against the selected QuEST shared library;
MPI/CUDA libraries are linked dependencies but their execution modes are disabled.
