#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "Square dimensions and resource bounds are checked before QR indexing; scalar arithmetic is checked by the backend"
)]
//! Statically selected, column-pivoted QR kernels. Diagnostics are numerical
//! candidate checks; they do not establish an approximation certificate.
use crate::{Error, Limits, Result};
use quest_numerics::arithmetic::{
    ArithmeticError, Backend, Budget, BudgetedBackend, ExactConstant, PointBackend,
};
use std::cmp::Ordering;

/// A square-system solution admitted by the shared rank and residual checks.
#[derive(Clone, Debug)]
pub struct LinearSolution<T> {
    pub values: Vec<T>,
    pub rank: usize,
    /// Dimensionless infinity-norm backward error in the scaled system.
    pub residual: T,
    /// Relative threshold used for both rank and backward-error admission.
    pub relative_threshold: T,
}
/// Caller-selected kernel; no runtime backend selection or fallback occurs.
pub trait LinearSolver<P: PointBackend<Error = ArithmeticError>> {
    /// Solve a row-major square system.
    ///
    /// # Errors
    /// Rejects malformed, nonfinite, over-budget, rank-deficient, or numerically
    /// unresolved systems. Backend errors are retained.
    fn solve(
        &self,
        backend: &mut P,
        matrix: &[P::Scalar],
        rhs: &[P::Scalar],
        n: usize,
        limits: Limits,
    ) -> Result<LinearSolution<P::Scalar>>;
}
#[derive(Clone, Copy, Debug, Default)]
pub struct PivotedQr;
#[derive(Clone, Copy, Debug, Default)]
pub struct MpHouseholder;

struct System<T> {
    matrix: Vec<T>,
    rhs: Vec<T>,
    matrix_scale: T,
    rhs_scale: T,
    threshold: T,
    workspace_bytes: usize,
}

fn magnitude<P: PointBackend<Error = ArithmeticError>>(
    backend: &mut P,
    x: &P::Scalar,
    zero: &P::Scalar,
) -> Result<P::Scalar> {
    Ok(if backend.compare(x, zero)? == Ordering::Less {
        backend.neg(x.clone())?
    } else {
        x.clone()
    })
}
fn maximum<P: PointBackend<Error = ArithmeticError>>(
    backend: &P,
    a: P::Scalar,
    b: P::Scalar,
) -> Result<P::Scalar> {
    Ok(if backend.compare(&a, &b)? == Ordering::Less {
        b
    } else {
        a
    })
}
fn admit<P: PointBackend<Error = ArithmeticError>>(
    backend: &mut P,
    matrix: &[P::Scalar],
    rhs: &[P::Scalar],
    n: usize,
    limits: Limits,
) -> Result<System<P::Scalar>> {
    let square = n.checked_mul(n).ok_or(Error::Budget("QR shape overflow"))?;
    if matrix.len() != square {
        return Err(Error::Shape {
            expected: square,
            actual: matrix.len(),
        });
    }
    if rhs.len() != n {
        return Err(Error::Shape {
            expected: n,
            actual: rhs.len(),
        });
    }
    if n == 0 {
        return Err(Error::NotEstablished("empty linear system"));
    }
    if n > limits.max_coefficients || i32::try_from(n).is_err() {
        return Err(Error::Budget("QR dimension"));
    }
    let mut scalar_bytes = backend.working_scalar_bytes().max(size_of::<P::Scalar>());
    // Validate the complete input before zero, rank, and scaling shortcuts.
    for x in matrix.iter().chain(rhs) {
        backend.compare(x, x)?;
        scalar_bytes = scalar_bytes.max(backend.storage_bytes(x)?);
    }
    let bytes = square
        .checked_add(n)
        .and_then(|x| x.checked_mul(16))
        .and_then(|x| x.checked_mul(scalar_bytes.max(size_of::<usize>())))
        .ok_or(Error::Budget("QR storage overflow"))?;
    if bytes > limits.max_bytes || isize::try_from(bytes).is_err() {
        return Err(Error::Budget("QR storage"));
    }
    let zero = backend.point(0.0)?;
    let one = backend.point(1.0)?;
    let mut matrix_scale = zero.clone();
    let mut rhs_scale = zero.clone();
    for x in matrix {
        let a = magnitude(backend, x, &zero)?;
        matrix_scale = maximum(backend, matrix_scale, a)?;
    }
    for x in rhs {
        let a = magnitude(backend, x, &zero)?;
        rhs_scale = maximum(backend, rhs_scale, a)?;
    }
    if backend.compare(&matrix_scale, &zero)? == Ordering::Equal {
        return Err(Error::NotEstablished("rank-deficient linear system"));
    }
    if backend.compare(&rhs_scale, &zero)? == Ordering::Equal {
        rhs_scale = one;
    }
    let factor = i64::try_from(n)
        .ok()
        .and_then(|x| x.checked_mul(64))
        .ok_or(Error::Budget("QR threshold"))?;
    let factor = backend.constant(&ExactConstant::Integer(factor))?;
    let epsilon = backend.epsilon()?;
    let threshold = backend.mul(epsilon, factor)?;
    let matrix = matrix
        .iter()
        .map(|x| {
            backend
                .div(x.clone(), matrix_scale.clone())
                .map_err(Error::from)
        })
        .collect::<Result<Vec<_>>>()?;
    let rhs = rhs
        .iter()
        .map(|x| {
            backend
                .div(x.clone(), rhs_scale.clone())
                .map_err(Error::from)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(System {
        matrix,
        rhs,
        matrix_scale,
        rhs_scale,
        threshold,
        workspace_bytes: bytes,
    })
}

fn faer_workspace(
    n: usize,
    system_bytes: usize,
    limits: Limits,
) -> Result<(faer::dyn_stack::StackReq, faer::dyn_stack::StackReq)> {
    use faer::{
        Par,
        linalg::qr::col_pivoting::{factor, solve},
    };
    let factor =
        factor::qr_in_place_scratch::<usize, f64>(n, n, 1, Par::Seq, faer::Spec::default());
    let solve = solve::solve_in_place_scratch::<usize, f64>(n, 1, 1, Par::Seq);
    let bytes = |req: faer::dyn_stack::StackReq| {
        req.layout()
            .map(|layout| layout.size())
            .map_err(|_| Error::Budget("faer layout overflow"))
    };
    // Public layouts use the same padded row capacity as owned faer matrices.
    // The generic allowance covers scaled inputs and residual/output vectors;
    // these are additional simultaneously retained faer allocations.
    let mut total = system_bytes;
    for (rows, columns) in [(n, n), (n, 1), (1, n)] {
        total = total
            .checked_add(bytes(faer::linalg::temp_mat_scratch::<f64>(rows, columns))?)
            .ok_or(Error::Budget("faer storage overflow"))?;
    }
    let permutations = n
        .checked_mul(2 * size_of::<usize>())
        .ok_or(Error::Budget("faer permutation storage"))?;
    total = total
        .checked_add(permutations)
        .and_then(|value| value.checked_add(bytes(factor).ok()?.max(bytes(solve).ok()?)))
        .ok_or(Error::Budget("faer scratch storage"))?;
    if total > limits.max_bytes || isize::try_from(total).is_err() {
        return Err(Error::Budget("faer storage"));
    }
    Ok((factor, solve))
}

/// Norm of a column tail using its largest component as the scale. Squaring
/// raw MP or binary64 inputs is deliberately avoided.
fn column_norm<P: PointBackend<Error = ArithmeticError>>(
    backend: &mut P,
    matrix: &[P::Scalar],
    n: usize,
    start: usize,
    column: usize,
    zero: &P::Scalar,
) -> Result<P::Scalar> {
    let mut scale = zero.clone();
    for row in start..n {
        let a = magnitude(backend, &matrix[row * n + column], zero)?;
        scale = maximum(backend, scale, a)?;
    }
    if backend.compare(&scale, zero)? == Ordering::Equal {
        return Ok(zero.clone());
    }
    let mut sum = zero.clone();
    for row in start..n {
        let x = backend.div(matrix[row * n + column].clone(), scale.clone())?;
        let term = backend.mul(x.clone(), x)?;
        sum = backend.add(sum, term)?;
    }
    let root = backend.sqrt(sum)?;
    Ok(backend.mul(scale, root)?)
}

fn finish<P: PointBackend<Error = ArithmeticError>>(
    backend: &mut P,
    system: &System<P::Scalar>,
    values: Vec<P::Scalar>,
    n: usize,
) -> Result<LinearSolution<P::Scalar>> {
    let zero = backend.point(0.0)?;
    let one = backend.point(1.0)?;
    // Split the scale ratio into four monotone factors. Forming b_scale / a_scale
    // directly can overflow or underflow although x * b_scale / a_scale fits.
    // Fourth roots keep the factor representable over the complete exponent
    // range; intermediate products move monotonically toward the final value.
    let numerator = backend.sqrt(system.rhs_scale.clone())?;
    let numerator = backend.sqrt(numerator)?;
    let denominator = backend.sqrt(system.matrix_scale.clone())?;
    let denominator = backend.sqrt(denominator)?;
    let ratio = backend.div(numerator.clone(), denominator.clone())?;
    let output = values
        .into_iter()
        .map(|mut x| {
            let nonzero = backend.compare(&x, &zero)? != Ordering::Equal;
            for _ in 0..4 {
                x = backend.mul(x, ratio.clone())?;
            }
            if nonzero && backend.compare(&x, &zero)? == Ordering::Equal {
                return Err(Error::NotEstablished("QR solution scaling underflow"));
            }
            Ok(x)
        })
        .collect::<Result<Vec<_>>>()?;
    // Check the coefficients actually returned, including rescaling rounding
    // and subnormal loss, against the scaled input system.
    let inverse = backend.div(denominator, numerator)?;
    let values = output
        .iter()
        .map(|value| {
            let mut x = value.clone();
            for _ in 0..4 {
                x = backend.mul(x, inverse.clone())?;
            }
            Ok(x)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut xscale = one;
    for x in &values {
        let a = magnitude(backend, x, &zero)?;
        xscale = maximum(backend, xscale, a)?;
    }
    let normalized = values
        .iter()
        .map(|x| backend.div(x.clone(), xscale.clone()).map_err(Error::from))
        .collect::<Result<Vec<_>>>()?;
    let mut anorm = zero.clone();
    let mut bnorm = zero.clone();
    let mut residual = zero.clone();
    let mut xnorm = zero.clone();
    for x in &normalized {
        let a = magnitude(backend, x, &zero)?;
        xnorm = maximum(backend, xnorm, a)?;
    }
    for row in 0..n {
        let mut sum = zero.clone();
        let mut rownorm = zero.clone();
        for column in 0..n {
            let a = &system.matrix[row * n + column];
            let term = backend.mul(a.clone(), normalized[column].clone())?;
            sum = backend.add(sum, term)?;
            let a = magnitude(backend, a, &zero)?;
            rownorm = backend.add(rownorm, a)?;
        }
        anorm = maximum(backend, anorm, rownorm)?;
        let b = backend.div(system.rhs[row].clone(), xscale.clone())?;
        let babs = magnitude(backend, &b, &zero)?;
        bnorm = maximum(backend, bnorm, babs)?;
        let difference = backend.sub(sum, b)?;
        let difference = magnitude(backend, &difference, &zero)?;
        residual = maximum(backend, residual, difference)?;
    }
    let denominator = backend.mul(anorm, xnorm)?;
    let denominator = backend.add(denominator, bnorm)?;
    if backend.compare(&denominator, &zero)? != Ordering::Equal {
        residual = backend.div(residual, denominator)?;
    }
    if backend.compare(&residual, &system.threshold)? == Ordering::Greater {
        return Err(Error::NotEstablished("QR backward residual"));
    }
    Ok(LinearSolution {
        values: output,
        rank: n,
        residual,
        relative_threshold: system.threshold.clone(),
    })
}

impl<P: PointBackend<Scalar = f64, Error = ArithmeticError>> LinearSolver<P> for PivotedQr {
    fn solve(
        &self,
        backend: &mut P,
        input: &[f64],
        rhs: &[f64],
        n: usize,
        limits: Limits,
    ) -> Result<LinearSolution<f64>> {
        use faer::{
            Mat, Par,
            dyn_stack::{MemBuffer, MemStack},
            linalg::qr::col_pivoting::{factor, solve},
        };
        let budget = Budget::new(limits.max_work);
        let backend = &mut BudgetedBackend::new(backend, &budget);
        let system = admit(backend, input, rhs, n, limits)?;
        let (factor_scratch, solve_scratch) = faer_workspace(n, system.workspace_bytes, limits)?;
        // Faer does not call our backend. Admit and debit a conservative cubic
        // operation model before entering its opaque factorization kernel.
        let work = n
            .checked_mul(n)
            .and_then(|x| x.checked_mul(n))
            .and_then(|x| x.checked_mul(16))
            .ok_or(Error::Budget("QR work overflow"))?;
        if work > limits.max_work {
            return Err(Error::Budget("QR work"));
        }
        backend.charge(work)?;
        let par = Par::Seq;
        let mut matrix = Mat::from_fn(n, n, |row, column| system.matrix[row * n + column]);
        let mut rhs = Mat::from_fn(n, 1, |row, _| system.rhs[row]);
        let mut forward = vec![0_usize; n];
        let mut backward = vec![0_usize; n];
        let mut coefficients = Mat::zeros(1, n);
        let mut scratch = MemBuffer::try_new(factor_scratch)
            .map_err(|_| Error::Budget("faer factor allocation"))?;
        let (_, permutation) = factor::qr_in_place(
            matrix.as_mut(),
            coefficients.as_mut(),
            &mut forward,
            &mut backward,
            par,
            MemStack::new(&mut scratch),
            faer::Spec::default(),
        );
        let scale = matrix.get(0, 0).abs();
        let threshold = backend.mul(scale, system.threshold)?;
        for i in 0..n {
            if backend.compare(&matrix.get(i, i).abs(), &threshold)? != Ordering::Greater {
                return Err(Error::NotEstablished("rank-deficient linear system"));
            }
        }
        drop(scratch);
        let mut scratch = MemBuffer::try_new(solve_scratch)
            .map_err(|_| Error::Budget("faer solve allocation"))?;
        solve::solve_in_place(
            matrix.as_ref(),
            coefficients.as_ref(),
            matrix.as_ref(),
            permutation,
            rhs.as_mut(),
            par,
            MemStack::new(&mut scratch),
        );
        let values = (0..n).map(|row| *rhs.get(row, 0)).collect();
        finish(backend, &system, values, n)
    }
}

impl<P: PointBackend<Error = ArithmeticError>> LinearSolver<P> for MpHouseholder {
    fn solve(
        &self,
        backend: &mut P,
        input: &[P::Scalar],
        rhs: &[P::Scalar],
        n: usize,
        limits: Limits,
    ) -> Result<LinearSolution<P::Scalar>> {
        let budget = Budget::new(limits.max_work);
        let backend = &mut BudgetedBackend::new(backend, &budget);
        let system = admit(backend, input, rhs, n, limits)?;
        let zero = backend.point(0.0)?;
        let one = backend.point(1.0)?;
        let two = backend.point(2.0)?;
        let mut matrix = system.matrix.clone();
        let mut rhs = system.rhs.clone();
        let mut permutation: Vec<_> = (0..n).collect();
        let mut leading = zero.clone();
        for step in 0..n {
            let mut pivot = step;
            let mut norm = column_norm(backend, &matrix, n, step, step, &zero)?;
            for column in step + 1..n {
                let candidate = column_norm(backend, &matrix, n, step, column, &zero)?;
                if backend.compare(&candidate, &norm)? == Ordering::Greater {
                    norm = candidate;
                    pivot = column;
                }
            }
            if step == 0 {
                leading = norm.clone();
            }
            let threshold = backend.mul(leading.clone(), system.threshold.clone())?;
            if backend.compare(&norm, &threshold)? != Ordering::Greater {
                return Err(Error::NotEstablished("rank-deficient linear system"));
            }
            if pivot != step {
                for row in 0..n {
                    matrix.swap(row * n + step, row * n + pivot);
                }
                permutation.swap(step, pivot);
            }
            // Divide by the norm before forming v: |v_i| <= 2, including the
            // cancellation-avoiding signed leading component.
            let negative = backend.compare(&matrix[step * n + step], &zero)? == Ordering::Less;
            let mut vector = Vec::with_capacity(n - step);
            for row in step..n {
                vector.push(backend.div(matrix[row * n + step].clone(), norm.clone())?);
            }
            vector[0] = if negative {
                backend.sub(vector[0].clone(), one.clone())?
            } else {
                backend.add(vector[0].clone(), one.clone())?
            };
            let mut squared = zero.clone();
            for value in &vector {
                let term = backend.mul(value.clone(), value.clone())?;
                squared = backend.add(squared, term)?;
            }
            let tau = backend.div(two.clone(), squared)?;
            for column in step + 1..n {
                let mut dot = zero.clone();
                for row in step..n {
                    let term = backend
                        .mul(vector[row - step].clone(), matrix[row * n + column].clone())?;
                    dot = backend.add(dot, term)?;
                }
                let weight = backend.mul(tau.clone(), dot)?;
                for row in step..n {
                    let term = backend.mul(weight.clone(), vector[row - step].clone())?;
                    matrix[row * n + column] =
                        backend.sub(matrix[row * n + column].clone(), term)?;
                }
            }
            let mut dot = zero.clone();
            for row in step..n {
                let term = backend.mul(vector[row - step].clone(), rhs[row].clone())?;
                dot = backend.add(dot, term)?;
            }
            let weight = backend.mul(tau, dot)?;
            for row in step..n {
                let term = backend.mul(weight.clone(), vector[row - step].clone())?;
                rhs[row] = backend.sub(rhs[row].clone(), term)?;
            }
            matrix[step * n + step] = if negative { norm } else { backend.neg(norm)? };
            for row in step + 1..n {
                matrix[row * n + step] = zero.clone();
            }
        }
        let mut pivoted = vec![zero.clone(); n];
        for row in (0..n).rev() {
            let mut value = rhs[row].clone();
            for column in row + 1..n {
                let term =
                    backend.mul(matrix[row * n + column].clone(), pivoted[column].clone())?;
                value = backend.sub(value, term)?;
            }
            pivoted[row] = backend.div(value, matrix[row * n + row].clone())?;
        }
        let mut values = vec![zero; n];
        for (column, value) in permutation.into_iter().zip(pivoted) {
            values[column] = value;
        }
        finish(backend, &system, values, n)
    }
}
