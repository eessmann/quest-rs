use faer::Mat;
use googletest::Result;
use googletest::prelude::*;
use num_complex::Complex64 as C;
use quest_circuit::*;

fn fixture() -> Mat<C> {
    Mat::from_fn(2, 2, |r, c| {
        C::new((r * 2 + c + 1) as f64, (r as f64) - 2.0 * c as f64)
    })
}

#[gtest]
fn logical_views_are_copied_including_transpose_and_conjugation() -> Result<()> {
    let a = fixture();
    let x = NumericalOperator::from_view(a.adjoint(), MatrixPolicy::default())?;
    for r in 0..2 {
        for c in 0..2 {
            expect_eq!(x.view()[(r, c)], a[(c, r)].conj());
        }
    }
    Ok(())
}

#[gtest]
fn faer_product_matches_independent_scalar_complex_fixture() -> Result<()> {
    let a = fixture();
    let b = Mat::from_fn(2, 2, |r, c| C::new((r + c + 2) as f64, (2 * r + c) as f64));
    let x = NumericalOperator::from_view(a.as_ref(), MatrixPolicy::default())?;
    let y = NumericalOperator::from_view(b.as_ref(), MatrixPolicy::default())?;
    let product = x.product(&y, MatrixPolicy::default())?;
    for r in 0..2 {
        for c in 0..2 {
            let expected = (0..2).map(|k| a[(r, k)] * b[(k, c)]).sum::<C>();
            expect_lt!((product.view()[(r, c)] - expected).norm(), 1e-12);
        }
    }
    Ok(())
}

#[gtest]
fn matrix_admission_checks_finiteness_shape_and_budget() -> Result<()> {
    let a = fixture();
    expect_true!(NumericalOperator::from_view(a.as_ref(), MatrixPolicy { max_bytes: 1 }).is_err());
    expect_true!(
        NumericalOperator::from_view(Mat::<C>::zeros(3, 3).as_ref(), MatrixPolicy::default())
            .is_err()
    );
    let bad = Mat::from_fn(2, 2, |_, _| C::new(f64::NAN, 0.0));
    expect_true!(NumericalOperator::from_view(bad.as_ref(), MatrixPolicy::default()).is_err());
    Ok(())
}

#[gtest]
fn padding_is_counted_and_noncontiguous_views_keep_logical_coordinates() -> Result<()> {
    let a = fixture();
    expect_true!(NumericalOperator::from_view(a.as_ref(), MatrixPolicy { max_bytes: 64 }).is_err());
    let padded = Mat::from_fn(7, 5, |r, c| C::new((10 * r + c) as f64, (r + c) as f64));
    let view = padded.submatrix(2, 1, 2, 2).reverse_rows().conjugate();
    let admitted = NumericalOperator::from_view(view, MatrixPolicy::default())?;
    expect_eq!(admitted.view()[(0, 0)], padded[(3, 1)].conj());
    expect_eq!(admitted.view()[(1, 1)], padded[(2, 2)].conj());
    let rows = [
        C::new(1.0, 2.0),
        C::new(3.0, 4.0),
        C::new(5.0, 6.0),
        C::new(7.0, 8.0),
    ];
    let row_view = faer::MatRef::from_row_major_slice(&rows, 2, 2);
    let row_admitted = NumericalOperator::from_view(row_view, MatrixPolicy::default())?;
    expect_eq!(row_admitted.view()[(0, 1)], rows[1]);
    expect_eq!(row_admitted.view()[(1, 0)], rows[2]);
    Ok(())
}

#[gtest]
fn u_realization_keeps_finite_large_phases_without_overflowing_their_sum() -> Result<()> {
    let matrix = BoundGate::U {
        theta: 0.5,
        phi: 1e308,
        lambda: 1e308,
    }
    .matrix(MatrixPolicy::default())?;
    expect_lt!(matrix.unitarity_residual(MatrixPolicy::default())?, 1e-14);
    let phase = C::new(1e308f64.cos(), 1e308f64.sin());
    expect_lt!(
        (matrix.view()[(1, 1)] - phase * phase * 0.25f64.cos()).norm(),
        1e-15
    );
    Ok(())
}
