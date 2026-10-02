# Static Remez linear kernels

Owned changes: `crates/quest-polynomial/src/remez/linear.rs` and `tests/linear_kernels.rs` only. Parent owns Remez orchestration and public request integration.

- `LinearSolver<P>::solve(&self, backend, row_major_matrix, rhs, n, limits)` returns `LinearSolution { values, rank, residual, relative_threshold }`.
- `PivotedQr` uses sequential faer column-pivoted QR for any compatible f64 point backend, including metered wrappers. Opaque factorization is explicitly charged with a conservative cubic work model before execution.
- `MpHouseholder` uses generic backend arithmetic for scaled column norms, column pivoting, cancellation-avoiding Householder reflectors, triangular solve, and inverse permutation. No normal equations, erased backend, runtime precision choice, or fallback.
- Both kernels validate all inputs before mathematical shortcuts, admit shape/storage, meter local and enclosing work limits, and use epsilon * 64 * dimension for relative rank and backward-residual checks. Rank deficiency and unresolved residuals fail explicitly.
- Matrix and RHS normalize independently. Four monotone fourth-root factors rescale the result without an overflowing/underflowing intermediate scale ratio; returned coefficients are mapped back for the shared residual check. A nonzero result underflowing to zero is rejected.
- The residual is a dimensionless infinity-norm backward error. These are candidate diagnostics; the independent original-target approximation certificate remains the parent orchestration's authority.

Validation: `devenv shell -- cargo test -p quest-polynomial --test linear_kernels` passed **6 tests**, exit 0 (`linear-kernels.log`). Tests exercise degree-two alternation systems, extreme common input scales, precision-sensitive MP rank, malformed/nonfinite inputs, local and enclosing budgets, and nonzero output underflow. Scoped rustfmt completed. Library integration is currently compiling with the expected generic-const-expression/next-solver toolchain warning. Clippy and whole-polynomial integration gates remain parent-coordinated.

## Final faer storage correction

Independent native review identified that the generic scalar allowance alone did not cover faer row padding and scratch: a 1x1 system admitted with 256 bytes already needs three 64-byte padded matrices, two permutation arrays, and factorization scratch in addition to the scaled system. The new 1x1/256-byte test failed before the fix (`linear-storage-red.log`).

F64 admission now adds to the existing conservative generic system/residual/output allowance: `temp_mat_scratch::<f64>(n,n)`, `(n,1)`, and `(1,n)` layout sizes; `2*n*sizeof(usize)` permutation storage; and the maximum of the public factor and solve scratch layouts. Layouts are checked before faer allocation. The factor MemBuffer is explicitly dropped before solve scratch allocation, so maximum rather than sum is sound; scratch allocation uses `try_new`. The generic allowance remains conservative, including small Vec collection capacities, rather than assuming each Result collection allocates exactly its length. Owned faer matrices use the same padding rule as these public temporary-matrix layouts in pinned faer 0.24.4.

This models application-owned heap buffers and the opaque kernel's published workspace contract. It does not claim an allocator-enforced process RSS bound or account OS/allocator metadata. The numerical factorization, pivoting, rank and residual contracts are unchanged.

`devenv shell -- env CARGO_BUILD_JOBS=2 cargo test -p quest-polynomial --test linear_kernels`: seven passed (`linear-storage-green.log`). Strict library and focused-test Clippy exited zero (`linear-storage-clippy.log`); the preexisting nightly generic_const_exprs/next-solver compatibility warning remains. No whole-workspace gate or timing was run by this correction.
