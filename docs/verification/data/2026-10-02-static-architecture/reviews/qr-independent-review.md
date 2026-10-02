# Independent QR mathematics and resource review

Reviewer: native/polynomial owner; the QR implementation belongs to the compiler owner. Read-only review of current `remez/linear.rs`, its six analytic kernel tests, and the pinned faer 0.24.4 factor/solve/storage source. No builds or production edits were performed during the numerical measurement window.

## Mathematical assessment

No numerical defect was found in the reviewed pivoting, scaling, reflector, rank or residual logic. This is a scoped mathematical review, not exhaustive validation of all condition numbers or a rigorous backward-error certificate.

- Both kernels scale the complete finite matrix and RHS independently, retain those scales, and use a relative rank threshold of `64*n*epsilon`. Column norms use a maximum-component scale before squaring, avoiding raw extreme-exponent squares.
- The generic reflector divides the selected column tail by its norm, then adds the sign-matched unit vector. Its leading component has magnitude at least one, avoiding cancellation. The signed diagonal is consistent with that reflector, trailing columns and RHS receive the same reflection, and pivot permutations map solved coordinates back to original columns.
- Faer's factorization uses one-row Householder coefficient blocks and its column-pivoting solve API with the same factor matrix as Q basis and R. Faer's solve applies Q transpose, triangular substitution and the inverse column permutation; this matches the generic kernel mathematically. Near-threshold floating rank decisions need not be bitwise identical.
- The final scale ratio is split into four fourth-root factors. For positive finite binary64 scales these factors remain representable; multiplication progresses toward the final scale instead of forming a potentially unrepresentable direct ratio. A nonzero result that becomes zero is rejected. The returned coefficients are mapped back into the scaled system before a normalized infinity-norm backward-residual check, avoiding large raw products. These diagnostics are candidate admission checks; approximation proof remains separate.
- The existing tests independently exercise a pivoted alternation system, extreme common scales, selected-precision rank resolution, nonfinite/shape/rank rejection, nested work/resource limits, and unrepresentable solution rejection. This review did not run new tests.

## Original resource finding (resolved below)

The shared scalar storage heuristic undercounts faer's known small-matrix padding and workspace. At `n=1`, it admits `(1*1+1)*16*8 = 256` bytes. Faer pads an owned f64 matrix's row capacity to a multiple of eight using 64-byte alignment. The three matrices therefore retain 192 bytes, the two permutation vectors retain 16, the scaled System vectors retain 16, and factorization scratch retains 128: at least 352 bytes before solve scratch and returned-value temporaries.

The public `faer::linalg::temp_mat_scratch::<f64>(rows,cols)` API computes the same padded matrix layout as owned Mat. `StackReq::{size_bytes,align_bytes,unaligned_bytes_required}` expose its storage requirement. Exact factor/solve requirements are available from `qr::col_pivoting::factor::qr_in_place_scratch::<usize,f64>(n,n,1,Par::Seq,Spec::default())` and `qr::col_pivoting::solve::solve_in_place_scratch::<usize,f64>(n,1,1,Par::Seq)`.

Admit a checked sum of known live array storage, the three padded Mat layouts, and actual public workspace requirements before allocating. Explicitly dropping the factor buffer before allocating solve scratch permits a maximum of the workspace requirements; retaining both requires their sum. The parent and compiler owner own this correction and focused verification after the quiet window. No claim is made about allocator metadata or unknown internal library allocations.

## Resolution reviewed

The compiler owner added `faer_workspace` admission using the three public padded Mat layouts, permutation arrays, and the maximum of exact factor/solve workspace layouts. It retains the conservative generic allowance for scaled System/output/residual vectors (including small Vec capacity growth). The check runs before faer allocation. The factor buffer is explicitly dropped before a fallible solve-buffer allocation, so the maximum workspace model matches its lifetime. This resolves the known padding/workspace undercount without altering QR mathematics or selected backend contracts.

The focused red receipt (`linear-storage-red.log`) reproduced admission undercount. The green receipt (`linear-storage-green.log`) records seven passing kernel tests, including tight n=1 storage rejection; `linear-storage-clippy.log` passes strict library/test Clippy. The reviewer read the final correction and receipts; the parent and numerical owner own final integrated and refreshed performance verification.
