# Actual-capacity admission for persisted matching loads

The persisted loader now checks actual record and reverse-directory capacities
before bucket processing or distributed routing. Independent review and focused
tests passed on 2026-10-06. The API, file format, matching semantics and native
QuEST remain unchanged.

Previously, a requested record count could fit the loader budget while a larger
returned vector allocation was only considered later under a separate, larger
replay budget. The new admission also checks common raw limits and the initial
metadata/work allowance before opening files. A rank-local failure is agreed
before peers enter the next stage.

The [method and resource model](../research/persisted-matching-loading.md) describe
the simultaneous manifest, chunk, record, reverse and scalar/control allowances.
Adjacent IO guards check actual manifest input before reading, receipt and
String storage before filling, and typed/disk chunks before conversion. Returned
allocations are inspected immediately; an oversized allocation can temporarily
exist before rejection. This is not a hard allocator quota.

HDF5 metadata/cache, returned header/owner metadata, filesystem/native/MPI
internals, caller-owned inputs, allocator overhead and process RSS have separate
scope. The fixed stack/control allowance is a model, not a measured high-water
mark. Existing returned-source accounting remains distinct from transient load
storage. A caller must reserve its declared loading envelope while earlier
register and prepared-child owners remain live.

## Independent evidence

The [seven-file source manifest](data/2026-10-06-persisted-matching-loading/source.json),
SHA-256 `bf8191b4217472da04972b32566ed236783546c226dbd85bb7f7e43b8119043c`,
has content aggregate `a0e5437a9f67decf6d33418aded51a26a390d2c0f0a94900dbe89ca4e7f4d401`.
All seven files stayed unchanged through independent checks. The
[focused receipt](data/2026-10-06-persisted-matching-loading/focused.json), SHA-256
`5f01d4ea2a39c3d05a0823f8c8f6b9ad376c596453aed761a0576aafb3a938d2`, records scopes
and private log hashes. These exclude dependencies and do not attest a full build.

Independent checks passed:

- Two loader tests, including nine bounded MPI fault jobs at one and two ranks.
  Real oversized allocations reject before later bucket/routing work. Early
  budget/work and raw-limit mismatch cases stop before manifest IO. The register
  and accounting remain unchanged, and a prior prepared child remains usable.
- All 51 IO unit/integration tests and two doctests, including five isolated
  actual-capacity cases.
- Persisted replay/restart and bridge test parents at 1/2/4/8 ranks and split
  communicators. The same resources reload from four producer ranks on two or
  eight ranks; the incompatible three-rank layout rejects. Whole-unitary and
  standalone-adjoint comparisons remain intact.
- Strict Clippy for selected runtime targets and the IO library/tests, plus
  formatting for all seven changed Rust files.

The first independent broader library/test Clippy attempt encountered the
concurrently unfinished persisted-inverse consumer test and failed there. That
attempt is preserved separately. The implementation owner had passed the full
scoped library/test check before the new target appeared; this stage makes no
current workspace-wide Clippy claim.

Development evidence also preserves genuine before-fix failures, test-build
errors and an incorrectly shared publication/load fixture budget. Historical IO
failure probes used 32,768 typed/disk elements; the final probes use 1,024 elements
and each stays below 1 MiB. Runtime limits were unchanged. These real bounded
allocations do not demonstrate OS exhaustion or actual large-count transport.

With matching native QuEST and MPI configured:

```sh
cargo test -p quest-rs --features qsvt-io,mpi --lib qsvt::persisted_matching::loading::capacity_tests -- --test-threads=1
cargo test -p quest-qsvt-io
cargo test -p quest-rs --features qsvt-io,mpi --test matching_persisted --test matching_persisted_preparation
```

The [earlier consuming bridge](2026-10-06-persisted-matching-preparation.md) retains
its historical source and evidence. The new correction supports the separately
declared 1 MiB loader/external reservation in the planned weighted-inverse
consumer. That consumer must still pass actual admission and its own review and
execution gates. No integrated inverse, multi-host capacity or scientific
accuracy result follows from these loader tests. Broad checkpoint 07 remains
historical; the next full workspace check is pending concurrent stage completion.
