# All-feature build validation and QSP optimization

User approved this plan in chat on 2026-10-02. Baseline: d19de2a0fd605b46bbc57e87fee36ba811a7d2b2.

Implement algorithm-specific completion (A), reflections-only inverse root (B), and shared transforms/buffers (C). Verify full workspace against system-installed QuEST/MPICH/serial HDF5 and clean Linux devenv with matched MPICH and MPI+SUBCOMM QuEST. Keep Darwin config unchanged. Preserve math, exact same-backend arithmetic ordering, resource accounting, failure order, independent certification, and no production fallback to offline.

## Tasks and ownership

1. Native setup: Linux Nix MPICH supplied consistently to QuEST/CMake/rsmpi/launcher. CPU-only installed consumer check. All-feature devenv test entrypoint, setup docs, actual dual-environment validation.
2. A: complete[_with] selects Policy.algorithm. Private tagged completion payload; weiss_ratio -> Option<&WeissRatio<M>>, algorithm accessor; synthesis dispatches on payload. Offline equivalent. NLFT removes ratio work. Charge four FFTs NLFT/five RHW, plus residual/retries. Test API, accuracy, budgets; migrate docs/tests.
3. Shared numerics: separate SharedConvolutionWorkspace + scoped session of two immutable RHS inputs; work_for(index), product(left,index) borrowed output. Lazy session-local spectra; reset between sessions. Preserve FFT backend/padding/sign/scaling/multiply order and finite error order. Exact storage/scratch/work admission; ordinary workspace unchanged.
4. B: only root requests reflections-only; both children produce transfer polynomials; skip final four products/allocations and unused n=1 normalization. Shared scalar normalization helpers.
5. C integration: use RHS schedule [0,1,1,0] for midpoint/reconstruction. Copy four compact windows and combine only after all products succeed. Mirror offline zero/direct/FFT paths and precision accounting. Cache buffers by size without escaping session borrows or retaining invalid spectra.
6. Evidence/review: baseline, A, A+B, A+B+C immutable sources; matched release degree256/1024 binary64 and degree16/256 offline128/256, three interleaved trials. Completion and end-to-end timing, allocations/peak, grid/work/correctness, retained failures. No speedup assumption.

## Required verification in each environment

cargo build --workspace --all-features --locked
cargo nextest run --workspace --all-features
cargo test --doc --workspace --all-features --locked
cargo nextest run -p quest-qsp --all-features --release --run-ignored only

Also formatting, strict all-feature Clippy, binding freshness, installed native-consumer checks, actual 2/4-rank MPI tests. Focused scalar/optional-feature regressions. Record complete commands, source/toolchain/dependency identity, counts, failures and exits. Do not disable tests/features or loosen numeric tolerances. Run measurements without competing builds.
