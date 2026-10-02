# All-feature builds and QSP completion/inverse changes

This record covers the Linux host validation and staged measurements for the
algorithm-specific completion, reflections-only inverse root and shared
convolution changes. Baseline source is
`d19de2a0fd605b46bbc57e87fee36ba811a7d2b2`. The
[approved plan](../superpowers/plans/2026-10-02-all-features-qsp.md) defines the
scope. Reconstructible [stage patches](data/2026-10-02-qsp-optimization/snapshots/manifest.json)
apply independently to that revision; their reconstruction was checked against
each immutable source snapshot.

## Resulting behavior

- Completion follows `Policy.algorithm`. Default inverse NLFT retains the
  complement only; RHW retains its Weiss ratio and provenance. The opaque result
  reports its algorithm, and `weiss_ratio()` now returns `Option<&WeissRatio<M>>`.
  Synthesis dispatches on the private payload. Offline synthesis uses the same
  distinction. Binary64 completion charges four FFTs for NLFT and five for RHW,
  plus residual checks and all retries.
- The inverse root recovers reflections without constructing unused transfer
  polynomials. Both recursive children still produce transfers. Singletons
  retain pivot/reflection validation. Leaf normalization shares one control
  helper instead of allocating a one-element control vector.
- Inverse groups share two lazily prepared right spectra. The original product
  order `[0, 1, 1, 0]` uses six forward and four inverse FFTs. A separate numerics
  workspace leaves ordinary convolution storage unchanged. Offline zero and
  direct-product paths retain their original behavior. Four compact coefficient
  windows finish before arithmetic combines them, and two windows become the
  outputs in place. Midpoints are released before transfer reconstruction.

The recurrence, gauge, response conventions, refinement and independent
certification remain those of the existing implementations. The supplied
[Laneve paper](https://arxiv.org/abs/2503.03026) and
[inverse-NLFT paper, section 4.2](https://arxiv.org/abs/2505.12615) were references
for the retained mathematical structure. No numerical tolerance was relaxed.

## Environments

The host is x86_64 Linux on an AMD Ryzen 9 7950X, with 16 cores and 32 hardware
threads. All timing comparisons use the system Rust toolchain on this host.

The system lane uses installed QuEST 4.3.0 at
`<home>/Projects/opt/quest`, Fedora MPICH 4.2.2 with absolute
`MPICC=/usr/lib64/mpich/bin/mpicc`, its matching launcher, and automatically
discovered serial HDF5 1.14.6. A clean `env -i` removes inherited Nix and loader
overrides. Rust is `1.101.0-nightly`, commit `c36f14571` (2026-10-01), with GCC
16.2.1 and Clang 22.1.8. The installed QuEST includes CPU/OpenMP, MPI/SUBCOMM,
CUDA and cuQuantum; acceptance here exercises CPU/OpenMP and MPI.

The second lane enters `devenv shell --clean -- bash ...`. Linux uses one pinned
MPICH 5.0.1 installation for QuEST, Rust MPI discovery and the launcher. QuEST
enables CPU/OpenMP and MPI/SUBCOMM; accelerator backends are disabled. The
installed QuEST output is
`/nix/store/331j44fm1cmrlglzm3lcji64j48c95pg-quest-4.3.0`.
Serial HDF5 is selected explicitly. Rust is `1.100.0-nightly`, commit
`6bb1652a020e80cef79332741d89e996d71933c9` (2026-09-22), with GCC 15.3.0 and
Clang 21.1.8. Both Cargo invocations actually select cargo-nextest 0.9.143 from
the Cargo home, although the Nix shell also supplies 0.9.144 on PATH. Exact
compiler, wrapper, launcher and library paths are retained with the receipts.

Each final lane starts with an empty, separate Cargo target directory. Cargo
build jobs are limited to two, Nextest to four threads, and OpenMP to two.
The local Cargo target configuration uses `-C target-cpu=native`. These settings
are local test conditions, not new portable defaults. Build-time ABI witnesses
compare the MPICC witness with QuEST's actual loaded MPI library.

QuEST's Nix install check explicitly initializes CPU execution and independently
loads its shared library. The independent loader caught a missing MPICH runtime
path that a directly linked MPI consumer had masked; the package now owns the
required RUNPATH. Existing installed-consumer checks also run CPU/OpenMP state
and density examples through direct, facade, wrapped and renamed dependencies.

## Verification

Both Linux lanes passed the commands below. Final production-source inventories
are unchanged throughout their last runs. The release tests preceded only the
two unrelated compiler-style repairs described below; their QSP/numerics sources
remain byte-identical.

| Command/check | System | Clean devenv |
| --- | --- | --- |
| `cargo build --workspace --all-features --locked` | exit 0 | exit 0 |
| `cargo nextest run --workspace --all-features` | 1,120 passed, 3 skipped; exit 0 | 1,120 passed, 3 skipped; exit 0 |
| `cargo test --doc --workspace --all-features --locked` | 65 passed, 1 ignored; exit 0 | 65 passed, 1 ignored; exit 0 |
| `cargo nextest run -p quest-qsp --all-features --release --run-ignored only` | 3 passed, 118 filtered; exit 0 | 3 passed, 118 filtered; exit 0 |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0 | exit 0 |
| `cargo fmt --all -- --check` | exit 0 | exit 0 |
| `cargo run --locked -p xtask -- generate-quest-bindings --check` | exit 0 | exit 0 |
| `cargo run --locked -p xtask -- check-native-consumers --backends cpu,omp --work-dir ...` | execution and complete loader closure pass; exit 0 | execution and complete loader closure pass; exit 0 |
| Explicit MPICH launch of registered MPI cases | 2 and 4 ranks, every rank passes; exit 0 | 2 and 4 ranks, every rank passes; exit 0 |

Workspace Nextest registered 167 binaries. Its three skipped QSP fixtures were
all subsequently executed by the release command: degree-8192 interval-FFT
certification, degree-8105 offline synthesis/export/certification, and degree-8105
serial/1/2/4-worker canonical/generalized bit preservation. The one ignored
doctest remains an intentionally non-executed documentation example.

Explicit MPI runs use `mpi_collective_preflight_rejects_mismatch_on_every_rank`
on two ranks and `mpi_subgroups_and_excluded_ranks_preserve_contexts` on four,
with `QUEST_SYS_MPI_CASE` selecting their inner bodies. Captured outputs show
one passing test per rank. The tests assert actual communicator sizes and
collective behavior; registration alone is not used as execution evidence.

Focused QSP checks passed 126 tests/doctests with all features, 38 without
defaults, and 40 offline-only library tests (one ignored scale fixture).
Strict package Clippy passed all three combinations. Shared numerics passed 82
tests plus four doctests with all features, and 75 plus four in the scalar
configuration. Eight fixture-controller/summary tests and a standalone nested
allocator probe passed. Independent source review found no outstanding defects.

Commands, separate environment paths, source inventories, complete failures,
test logs, consumer evidence and reviews are retained in the
[evidence bundle](data/2026-10-02-qsp-optimization/README.md). System final receipts
use `acceptance4-system-*`; Nix final receipts use `acceptance3-devenv-*`, with
the previously successful release, binding, consumer and rank receipts linked
by the acceptance summary. Both nightly compilers still emit their existing
`generic_const_exprs` solver-fallback diagnostic; the strict Clippy commands
return zero without suppressing it.

The regression coverage includes both response conventions, algorithm identity
and ratio provenance, artifact roundtrips, completion and precision retries,
resource boundaries, and independent certificates. Exact recurrence oracles
compare binary64 component bits and offline values, configured precision and
signed zeros. Inputs cover singleton, odd/even, sparse/dense, offset and
near-boundary cases. Shared sessions cover padding, reset, invalid inputs,
failure order and scalar/SIMD/pool execution. Pointer and capacity tests confirm
the compact output buffers are reused. Minimal-feature builds exercise the
changed gates separately.

## Failures retained and resolved

1. The baseline system Nextest run failed an existing `!Send` UI snapshot when
   the newer nightly changed how it printed `std::Vec`. The production ownership
   contract was unchanged. A compile-time trait-ambiguity assertion now verifies
   the same concrete `PreparedProgram<'static>` is not Send, without depending
   on standard-library source layout. An intentionally Send control fails on
   both toolchains; the other seven UI cases still use their existing harness.
2. Binding freshness in both MPI environments exposed one conditional native
   declaration absent from the existing common inventory. The 747 common entries
   and generated Rust/CXX remain byte-identical. A small MPI/SUBCOMM sidecar
   tracks the reviewed initializer, normalizes only its inventory MPI_Comm type,
   and still rejects removed, renamed or newly unreviewed declarations.
3. Reusing target directories populated from an immutable baseline exposed stale
   Cargo dependency variants: QSP could not import the new numerics export.
   Dependency records and failures are retained. Entirely fresh final directories
   resolve the import without changing source. Benchmark variants likewise use
   independent build directories.
4. Fixture review found timeout cleanup could leave a child alive and trial
   preflight did not compare with saved build identities. Both were repaired and
   tested before the official campaign. Old observer builds and the concurrent
   smoke run are retained as preparation evidence, excluded from measurements.
5. Strict all-target/all-feature Clippy on the system nightly reported the
   pre-existing `inconsistent_struct_constructor` lint in the MPI CLI decoder.
   The constructor now lists its already evaluated local values in definition
   order. No decoding operations or expressions moved. Both full workspace
   builds, tests, doctests and strict lint checks were repeated after this edit;
   the release QSP source is byte-identical to its successful scale-test source.
   The Nix nightly additionally rejected an existing test module preceding
   production items. Moving that module to the end of its file preserves its
   exact bytes and all remaining production bytes; both final lint checks use
   the relocation without lint suppressions.

## Measurement method

The [fixture](fixtures/qsp-optimization/README.md) compares baseline, A, A+B and
A+B+C with three interleaved trials. It uses scalar binary64 degrees 256/1024 and
offline degrees 16/256 at fixed 128/256-bit computation and certification.
Completion and end-to-end synthesis are observed separately, including Rust
allocation counts, additional peak live requested bytes, accepted grids and
charged work. Instrumentation exists only in copied benchmark sources.

Binary64 end-to-end starts from an admitted target; offline end-to-end includes
admission, export and independent certification. Checks and allocation counters
are timed consistently across variants. Peaks exclude allocator metadata,
stacks, native allocations and transient realloc overlap. Process RSS is recorded
separately. All workload exports and grids must match across variants and trials;
failed attempts remain failures. Baseline binary64 completion undercounted an
FFT, so corrected work charges are not literal instruction-count comparisons.

No speedup threshold is an acceptance condition. These measurements do not
establish an explanation for the historical Dashu regression. Darwin configuration
was evaluated but not built or run; additional accelerator and host coverage is
outside this record.

## Observed results

All 12 trial processes succeeded, with 72 successful workload records. Export
fingerprints and accepted grids match exactly for each workload across all
variants and trials. Saved build identities still match after execution. No
compiler, Cargo/Nextest or MPI worker processes were present at campaign entry
or exit, and all validation agents were idle throughout the trials.

The following are median end-to-end milliseconds per invocation; the
[complete tables](data/2026-10-02-qsp-optimization/results.md) also show completion,
allocations, peak live storage and charged work. The
[machine-readable summary](data/2026-10-02-qsp-optimization/summary.json) preserves
minimum and maximum observations from all three trials.

| Workload | Baseline | A | A+B | A+B+C |
| --- | ---: | ---: | ---: | ---: |
| Binary64 degree 256 | 1.1068 | 1.0342 | 0.9932 | 0.8940 |
| Binary64 degree 1024 | 5.7221 | 5.3019 | 4.9341 | 4.7485 |
| Offline degree 16, 128 bits | 66.6235 | 62.3050 | 62.7254 | 61.3309 |
| Offline degree 16, 256 bits | 106.5147 | 100.1135 | 100.6219 | 101.1278 |
| Offline degree 256, 128 bits | 2579.3103 | 2521.8988 | 2548.5965 | 2494.0102 |
| Offline degree 256, 256 bits | 4181.8110 | 4152.5806 | 4097.1972 | 4080.3451 |

A alone reduced observed completion medians by 23.1%/8.9% for the two binary64
degrees and 12.6–19.1% for the offline cases. Completion allocations and peak
live storage decrease consistently. A+B and A+B+C perform the same completion
work as A; differences between their completion timings are not evidence of
further completion optimizations.

The complete change reduced observed end-to-end medians by 19.2% and 17.0% for
binary64 and 2.4–7.9% for the offline cases. Intermediate offline timings are not
monotonic: B or C can be slower in individual comparisons. Three instrumented
trials do not establish a portable speedup or statistical significance for those
small differences.

Binary64 end-to-end allocations decreased from 8,731 to 7,454 at degree 256 and
34,141 to 29,026 at degree 1024; peak additional live storage fell from 248,856
to 231,104 bytes and 1,002,664 to 930,272 bytes. Offline completion memory fell,
but the measured whole-solve peak remained unchanged in every offline case.
Offline whole-solve allocation counts and synthesis work charges decreased;
its separate certification still contributes to duration and measured storage.
These are observations of the instrumented fixture, including allocator-counter
and correctness-check overhead, rather than uninstrumented application timings.
