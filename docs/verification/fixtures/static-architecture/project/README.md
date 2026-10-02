# Matched project workloads

Run this fixture inside the current repository's project environment, with both
checkouts available and a fresh output directory. Get an exclusive measurement
window first: builds and trials run serially with two Cargo jobs.

```sh
devenv shell -- python3 docs/verification/fixtures/static-architecture/project/run.py \
  --repository . \
  --baseline-repository /path/to/immutable/baseline \
  --output /path/to/new/receipts
```

`--prepare-only` generates manifests and source/environment receipts without
building or timing. The output must be new or empty; the runner preserves prior
receipts and never changes either repository's manifests, locks or source. It
generates two standalone packages, selecting the historical quest-circuit name
when present and the canonical quest-compile entrypoint otherwise. Both compile
the same workload source and shared allocator, use release thin LTO, and inherit
the same installed QuEST environment and final-consumer runtime path policy.

The workloads adapt the existing [QSP pipeline](../../../../../crates/quest-qsp/benches/pipeline.rs),
[compiler pipeline](../../../../../crates/quest-compile/benches/compiler.rs), and
[native runtime](../../../../../crates/quest/benches/runtime.rs) benchmarks and
verify mathematical results before the measured loops:

- QSP completion and inverse NLFT at degrees 256 and 1024 use identical periodic
  complex coefficients, power-of-two scaling, response tolerance 1e-11 and
  contractivity margin 1e-12. A strict l1 envelope ensures contractivity. Direct
  coefficient sums at four unit-circle points independently check the response;
  grid and reconstruction/completion diagnostics are recorded.
- Compiler construction/admission and verify/lower/plan use a 64-iteration
  structured circuit. A second workload inserts 256 finite operations with 128
  exact angle captures and verifies its captures and plan.
- Native preparation and warm execution use a 10-qubit Bell state, 128 shared
  matrix/oracle alias pairs across two signed control profiles, and a final Rz
  phase. All 1024 amplitudes are compared with independent expected values before
  measurement. Preparation and zeroed execution are measured separately.

Three trials interleave baseline/current and reverse their order on the second
trial. CSV columns record iterations, elapsed nanoseconds, successful Rust
allocations/reallocations, peak additional live Rust bytes above the warmed
baseline, and output scalar counts. Native C++/QuEST allocations are excluded;
native details also report admitted environment bytes, which are a resource
model rather than measured heap usage. Trial details retain process peak RSS
(including loaded code/native allocations); this is not a native heap breakdown. QSP diagnostics are not independent
certification evidence.

Receipts include source hashes, actual resolved scratch lock copies, toolchain
and checkout identities, cold/unchanged/touched-source rebuild wall time and process RSS,
executable bytes/hashes, section sizes, runtime CSVs and correctness diagnostics.
The touched-source stage updates only the generated scratch main.rs timestamp;
its bytes remain identical, and both repository sources remain unchanged.
Darwin uses `time -l`/`size -m`; Linux uses `time -v`/SysV section output. Repository Cargo/devenv/toolchain inputs and fixture sources are hashed before
and after the run; changed inputs invalidate the measurements. Updated scratch
locks are excluded from this mutation check and retained separately. A failed
build or mathematical check aborts the run and preserves its receipts. Local
CPU results do not establish Linux, MPI or accelerator performance.
