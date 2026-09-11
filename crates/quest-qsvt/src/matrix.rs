use crate::{Complex64, Error, NumericalPolicy, Result};
use faer::{Accum, Mat, MatRef, Par, mat::AsMatRef, traits::Conjugate};

pub fn allocate(
    rows: usize,
    cols: usize,
    policy: NumericalPolicy,
    value: impl FnMut(usize, usize) -> Complex64,
) -> Result<Mat<Complex64>> {
    policy.check(rows, cols, 1)?;
    let mut output = Mat::new();
    output
        .try_reserve(rows, cols)
        .map_err(|_| Error::Budget("matrix allocation"))?;
    output.resize_with(rows, cols, value);
    Ok(output)
}
pub fn snapshot<T: Conjugate<Canonical = Complex64>>(
    view: impl AsMatRef<T = T, Rows = usize, Cols = usize>,
    policy: NumericalPolicy,
) -> Result<Mat<Complex64>> {
    let view = view.as_mat_ref();
    let canonical = view.canonical();
    let conjugated = faer::Conj::get::<T>() == faer::Conj::Yes;
    let logical = |row, col| {
        let value = canonical[(row, col)];
        if conjugated { value.conj() } else { value }
    };
    for col in 0..view.ncols() {
        for row in 0..view.nrows() {
            let value = logical(row, col);
            if !value.re.is_finite() || !value.im.is_finite() {
                return Err(Error::NonFinite);
            }
        }
    }
    allocate(view.nrows(), view.ncols(), policy, logical)
}
pub fn multiply<T: Conjugate<Canonical = Complex64>, U: Conjugate<Canonical = Complex64>>(
    left: impl AsMatRef<T = T, Rows = usize, Cols = usize>,
    right: impl AsMatRef<T = U, Rows = usize, Cols = usize>,
    policy: NumericalPolicy,
) -> Result<Mat<Complex64>> {
    let left = left.as_mat_ref();
    let right = right.as_mat_ref();
    if left.ncols() != right.nrows() {
        return Err(Error::Space("matrix product dimensions"));
    }
    let mut output = allocate(left.nrows(), right.ncols(), policy, |_, _| {
        Complex64::new(0.0, 0.0)
    })?;
    for col in 0..right.ncols() {
        faer::linalg::matmul::matmul(
            output.as_mut().submatrix_mut(0, col, left.nrows(), 1),
            Accum::Replace,
            left,
            right.submatrix(0, col, right.nrows(), 1),
            Complex64::new(1.0, 0.0),
            Par::Seq,
        );
    }
    Ok(output)
}
pub fn difference(left: MatRef<'_, Complex64>, right: MatRef<'_, Complex64>) -> f64 {
    let mut total = 0.0_f64;
    for col in 0..left.ncols() {
        for row in 0..left.nrows() {
            let a = left[(row, col)];
            let b = right[(row, col)];
            total = total.hypot(a.re - b.re).hypot(a.im - b.im);
        }
    }
    total
}
