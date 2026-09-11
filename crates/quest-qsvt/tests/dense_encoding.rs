use faer::mat;
use googletest::prelude::*;
use quest_qsvt::{Complex64, DenseEncodingBuilder, NumericalPolicy};
use std::ops::Sub;

#[gtest]
fn dense_dilation_preserves_rectangular_complex_block_and_normalization() -> Result<()> {
    let matrix = mat![
        [
            Complex64::new(0.2, 0.1),
            Complex64::new(-0.1, 0.05),
            Complex64::new(0.07, -0.03)
        ],
        [
            Complex64::new(0.0, 0.12),
            Complex64::new(0.14, -0.09),
            Complex64::new(-0.04, 0.11)
        ]
    ];
    let encoding = DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
        .normalization(2.0)?
        .build()?;
    expect_that!(encoding.left().logical_dimension(), eq(2));
    expect_that!(encoding.right().logical_dimension(), eq(3));
    expect_that!(encoding.normalization().get(), eq(2.0));
    let actual = encoding.logical_matrix()?;
    for row in 0..2 {
        for col in 0..3 {
            expect_that!(actual[(row, col)].sub(matrix[(row, col)]).norm(), lt(1e-13));
        }
    }
    Ok(())
}

#[gtest]
fn dense_dilation_rejects_insufficient_normalization_and_small_budget() -> Result<()> {
    let matrix = mat![[Complex64::new(2.0, 0.0)]];
    expect_true!(
        DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy::default())?
            .normalization(1.0)?
            .build()
            .is_err()
    );
    expect_true!(
        DenseEncodingBuilder::new(matrix.as_ref(), NumericalPolicy { max_bytes: 1 }).is_err()
    );
    Ok(())
}
