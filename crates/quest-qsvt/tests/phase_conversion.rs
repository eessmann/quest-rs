use faer::{MatRef, mat};
use googletest::prelude::*;
use quest_qsp::{CanonicalWxImag, PhaseSequence, WxLaurent, WxSymmetric};
use quest_qsvt::{Complex64, DenseEncodingBuilder, NumericalPolicy, TransformBuilder};
use std::ops::{Mul, Sub};

fn check(actual: MatRef<'_, Complex64>, input: MatRef<'_, Complex64>, scale: f64, constant: bool) {
    expect_that!(actual.nrows(), eq(2));
    expect_that!(actual.ncols(), eq(2));
    for row in 0..2 {
        for col in 0..2 {
            let expected = if constant {
                Complex64::new(if row == col { scale } else { 0.0 }, 0.0)
            } else {
                input[(row, col)].mul(scale)
            };
            expect_that!(actual[(row, col)].sub(expected).norm(), lt(1e-13));
        }
    }
}
#[gtest]
fn huge_imported_phases_keep_full_complex_qsvt_blocks_and_finite_diagnostics() -> Result<()> {
    let input = mat![
        [Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.07)],
        [Complex64::new(0.1, -0.07), Complex64::new(-0.3, 0.0)]
    ];
    let encoding = DenseEncodingBuilder::new(input.as_ref(), NumericalPolicy::default())?
        .normalization(1.0)?
        .build()?;
    for huge in [1e20, -1e20, f64::MAX, -f64::MAX] {
        let (sin, cos) = huge.sin_cos();
        let canonical = TransformBuilder::new()
            .encoding(encoding.clone())
            .standard(PhaseSequence::<CanonicalWxImag>::builder(vec![huge]).build()?)
            .build()?;
        check(
            canonical.materialize_block()?.as_ref(),
            input.as_ref(),
            sin,
            true,
        );
        let laurent = TransformBuilder::new()
            .encoding(encoding.clone())
            .standard(PhaseSequence::<WxLaurent>::builder(vec![huge]).build()?)
            .build()?;
        check(
            laurent.materialize_block()?.as_ref(),
            input.as_ref(),
            cos,
            true,
        );
        let symmetric = TransformBuilder::new()
            .encoding(encoding.clone())
            .standard(PhaseSequence::<WxSymmetric>::builder(vec![huge, huge]).build()?)
            .build()?;
        check(
            symmetric.materialize_block()?.as_ref(),
            input.as_ref(),
            2.0 * sin * cos,
            false,
        );
        for transform in [canonical, laurent, symmetric] {
            let estimate = transform.evidence().phase_conversion_roundoff_estimate;
            expect_true!(estimate.is_finite());
            expect_that!(estimate, gt(0.0));
            expect_that!(estimate, lt(1e-13));
            expect_true!(transform.theorem_error_bound().is_none());
        }
    }
    Ok(())
}
