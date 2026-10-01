use faer::mat;
use googletest::prelude::*;
use quest_qsp::ControlSequence;
use quest_qsvt::{
    Complex64, DenseEncodingBuilder, NumericalPolicy, OperandLayout, RouteResponse,
    TransformBuilder,
};

#[gtest]
fn imaginary_odd_polynomial_preserves_its_phase_on_both_hermitianized_blocks() -> Result<()> {
    let zero = Complex64::new(0.0, 0.0);
    let one = Complex64::new(1.0, 0.0);
    let imaginary = Complex64::new(0.0, 1.0);
    let physical = mat![[Complex64::new(0.2, 0.1)], [Complex64::new(0.3, -0.2)]];
    let encoding = DenseEncodingBuilder::new(physical.as_ref(), NumericalPolicy::default())?
        .normalization(1.0)?
        .build()?;
    // diag(i,1) diag(z,1) I has upper-left polynomial i z, hence i T1(H).
    let controls = ControlSequence::builder()
        .matrices(vec![
            [[imaginary, zero], [zero, one]],
            [[one, zero], [zero, one]],
        ])
        .build()?;
    let transform = TransformBuilder::new()
        .encoding(encoding)
        .hermitianized_full(RouteResponse::imported(controls))
        .operands(OperandLayout::new(4, vec![2, 1], 3, Some(0))?)
        .build()?;
    let actual = transform.materialize_block()?;
    expect_eq!(actual.nrows(), 3);
    expect_eq!(actual.ncols(), 3);
    let expected = mat![
        [zero, zero, Complex64::new(-0.1, 0.2)],
        [zero, zero, Complex64::new(0.2, 0.3)],
        [Complex64::new(0.1, 0.2), Complex64::new(-0.2, 0.3), zero],
    ];
    for row in 0..3 {
        for col in 0..3 {
            expect_that!(
                std::ops::Sub::sub(actual[(row, col)], expected[(row, col)]).norm(),
                lt(1e-12)
            );
        }
    }
    Ok(())
}
