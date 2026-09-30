use faer::{Mat, MatRef, mat};
use googletest::prelude::*;
use quest_qsp::{ControlSequence, PhaseSequence, WxLaurent, WxSymmetric};
use quest_qsvt::{
    Complex64, DenseEncodingBuilder, NumericalPolicy, OperandLayout, TransformBuilder,
};
use std::ops::{Add, Div, Mul, Sub};

fn product(left: MatRef<'_, Complex64>, right: MatRef<'_, Complex64>) -> Mat<Complex64> {
    Mat::from_fn(left.nrows(), right.ncols(), |row, col| {
        (0..left.ncols()).fold(Complex64::new(0.0, 0.0), |total, k| {
            total.add(left[(row, k)].mul(right[(k, col)]))
        })
    })
}
fn adjoint(matrix: MatRef<'_, Complex64>) -> Mat<Complex64> {
    Mat::from_fn(matrix.ncols(), matrix.nrows(), |row, col| {
        matrix[(col, row)].conj()
    })
}
fn fixture() -> Mat<Complex64> {
    mat![
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
    ]
}
fn expect_matrix(actual: MatRef<'_, Complex64>, expected: MatRef<'_, Complex64>) {
    expect_that!(actual.nrows(), eq(expected.nrows()));
    expect_that!(actual.ncols(), eq(expected.ncols()));
    for row in 0..expected.nrows() {
        for col in 0..expected.ncols() {
            expect_that!(
                actual[(row, col)].sub(expected[(row, col)]).norm(),
                lt(3e-12)
            );
        }
    }
}
fn control_coefficients(sequence: &ControlSequence) -> Vec<Complex64> {
    let mut matrix = vec![[[Complex64::new(0.0, 0.0); 2]; 2]];
    if let Some((last, earlier)) = sequence.matrices().split_last() {
        if let Some(first) = matrix.first_mut() {
            *first = *last;
        }
        for factor in earlier.iter().rev() {
            let mut next = vec![[[Complex64::new(0.0, 0.0); 2]; 2]; matrix.len().saturating_add(1)];
            for (degree, coefficient) in matrix.iter().enumerate() {
                for (row, control_row) in factor.iter().enumerate() {
                    for (branch, &control) in control_row.iter().enumerate() {
                        for col in 0..2 {
                            let value = coefficient
                                .get(branch)
                                .and_then(|r| r.get(col))
                                .copied()
                                .unwrap_or_default();
                            let output_degree = degree.saturating_add(usize::from(branch == 0));
                            if let Some(output) = next
                                .get_mut(output_degree)
                                .and_then(|m| m.get_mut(row))
                                .and_then(|r| r.get_mut(col))
                            {
                                *output = output.add(control.mul(value));
                            }
                        }
                    }
                }
            }
            matrix = next;
        }
    }
    matrix
        .iter()
        .map(|m| {
            m.first()
                .and_then(|r| r.first())
                .copied()
                .unwrap_or_default()
        })
        .collect()
}
fn chebyshev(coefficients: &[Complex64], matrix: MatRef<'_, Complex64>) -> Mat<Complex64> {
    let size = matrix.nrows();
    let mut before = Mat::from_fn(size, size, |row, col| {
        Complex64::new(if row == col { 1.0 } else { 0.0 }, 0.0)
    });
    let mut current = Mat::from_fn(size, size, |row, col| matrix[(row, col)]);
    let mut result = Mat::<Complex64>::zeros(size, size);
    for (degree, &coefficient) in coefficients.iter().enumerate() {
        if degree > 1 {
            let multiplied = product(matrix, current.as_ref());
            let next = Mat::from_fn(size, size, |row, col| {
                multiplied[(row, col)].mul(2.0).sub(before[(row, col)])
            });
            before = current;
            current = next;
        }
        let term = if degree == 0 {
            before.as_ref()
        } else {
            current.as_ref()
        };
        for row in 0..size {
            for col in 0..size {
                result[(row, col)] = result[(row, col)].add(term[(row, col)].mul(coefficient));
            }
        }
    }
    result
}

#[gtest]
fn rectangular_complex_generalized_routes_match_independent_matrix_polynomials() -> Result<()> {
    let physical = fixture();
    let alpha = 1.7;
    let block = Mat::from_fn(2, 3, |row, col| physical[(row, col)].div(alpha));
    let encoding = DenseEncodingBuilder::new(physical.as_ref(), NumericalPolicy::default())?
        .normalization(alpha)?
        .build()?;
    let controls = ControlSequence::builder()
        .angles(&[0.31, -0.22, 0.14, 0.53], &[0.47, 0.91, -0.34, -0.28])?
        .build()?;
    let coefficients = control_coefficients(&controls);
    let hermitian = Mat::from_fn(5, 5, |row, col| {
        if row < 2 && col >= 2 {
            block[(row, col.saturating_sub(2))]
        } else if row >= 2 && col < 2 {
            block[(col, row.saturating_sub(2))].conj()
        } else {
            Complex64::new(0.0, 0.0)
        }
    });
    let expected = chebyshev(&coefficients, hermitian.as_ref());
    let layout = OperandLayout::new(5, vec![4, 1, 3], 2, Some(0))?;
    let full = TransformBuilder::new()
        .encoding(encoding.clone())
        .hermitianized_full(
            quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(controls.clone()),
        )
        .operands(layout.clone())
        .build()?;
    expect_matrix(full.materialize_block()?.as_ref(), expected.as_ref());
    let even = TransformBuilder::new()
        .encoding(encoding.clone())
        .hermitianized_even(
            quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(controls.clone())
                .even_component(),
        )
        .operands(layout.clone())
        .build()?;
    expect_matrix(
        even.materialize_block()?.as_ref(),
        expected.as_ref().submatrix(2, 2, 3, 3),
    );
    let odd = TransformBuilder::new()
        .encoding(encoding.clone())
        .hermitianized_odd(
            quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(controls.clone())
                .odd_component(),
        )
        .operands(layout.clone())
        .build()?;
    expect_matrix(
        odd.materialize_block()?.as_ref(),
        expected.as_ref().submatrix(0, 2, 2, 3),
    );
    let gram = product(adjoint(block.as_ref()).as_ref(), block.as_ref());
    let reduced = chebyshev(&coefficients, gram.as_ref());
    let even = TransformBuilder::new()
        .encoding(encoding.clone())
        .multiplication_even(
            quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls.clone()),
        )
        .operands(layout.clone())
        .build()?;
    expect_matrix(even.materialize_block()?.as_ref(), reduced.as_ref());
    let odd = TransformBuilder::new()
        .encoding(encoding)
        .multiplication_odd(
            quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls),
        )
        .operands(layout)
        .build()?;
    expect_matrix(
        odd.materialize_block()?.as_ref(),
        product(block.as_ref(), reduced.as_ref()).as_ref(),
    );
    expect_that!(odd.normalization().get(), eq(alpha));
    expect_that!(odd.query_counts().source_forward, eq(4));
    expect_that!(odd.query_counts().source_adjoint, eq(3));
    Ok(())
}

fn wx_response(x: f64, phases: &[f64]) -> Complex64 {
    let mut response = Mat::from_fn(2, 2, |row, col| {
        Complex64::new(if row == col { 1.0 } else { 0.0 }, 0.0)
    });
    let sine = (-x).mul_add(x, 1.0).sqrt();
    let signal = mat![
        [Complex64::new(x, 0.0), Complex64::new(0.0, sine)],
        [Complex64::new(0.0, sine), Complex64::new(x, 0.0)]
    ];
    for (index, &phase) in phases.iter().enumerate() {
        if index > 0 {
            response = product(response.as_ref(), signal.as_ref());
        }
        for row in 0..2 {
            for col in 0..2 {
                response[(row, col)] = response[(row, col)].mul(Complex64::from_polar(
                    1.0,
                    if col == 0 { phase } else { -phase },
                ));
            }
        }
    }
    response[(0, 0)]
}
#[gtest]
fn standard_degree_three_preserves_wx_conventions_on_rectangular_complex_inputs() -> Result<()> {
    let block = fixture();
    let encoding = DenseEncodingBuilder::new(block.as_ref(), NumericalPolicy::default())?
        .normalization(1.0)?
        .build()?;
    let symmetric = [0.1, 0.2, 0.2, 0.1];
    let laurent = [0.1, -0.2, 0.3, 0.4];
    let first = TransformBuilder::new()
        .encoding(encoding.clone())
        .standard(PhaseSequence::<WxSymmetric>::builder(symmetric.to_vec()).build()?)
        .build()?;
    let second = TransformBuilder::new()
        .encoding(encoding)
        .standard(PhaseSequence::<WxLaurent>::builder(laurent.to_vec()).build()?)
        .operands(OperandLayout::new(4, vec![2, 0, 3], 1, None)?)
        .build()?;
    let gram = product(adjoint(block.as_ref()).as_ref(), block.as_ref());
    let cubic = product(block.as_ref(), gram.as_ref());
    for (phases, transform, imaginary) in [(symmetric, first, true), (laurent, second, false)] {
        let readout = |x| {
            let value = wx_response(x, &phases);
            if imaginary { value.im } else { value.re }
        };
        let third = readout(1.0).sub(2.0 * readout(0.5)) / 0.75;
        let linear = readout(1.0) - third;
        let expected = Mat::from_fn(2, 3, |row, col| {
            block[(row, col)]
                .mul(linear)
                .add(cubic[(row, col)].mul(third))
        });
        expect_matrix(transform.materialize_block()?.as_ref(), expected.as_ref());
        expect_that!(transform.query_counts().semantic, eq(3));
        expect_that!(transform.query_counts().source_forward, eq(4));
        expect_that!(transform.query_counts().source_adjoint, eq(2));
        expect_that!(transform.query_counts().retained_oracle_calls, eq(6));
    }
    Ok(())
}

#[gtest]
fn complex_isometry_and_dense_projector_routes_preserve_ordered_logical_bases() -> Result<()> {
    use quest_circuit::{Gate, OracleFragment, QuantumRegionBuilder};
    use quest_qsvt::{EncodingBuilder, Left, LogicalSpace, Right};
    let policy = NumericalPolicy::default();
    let scale = std::f64::consts::FRAC_1_SQRT_2;
    let zero = Complex64::new(0.0, 0.0);
    let left_basis = mat![
        [Complex64::new(scale, 0.0), zero],
        [zero, Complex64::new(scale, 0.0)],
        [Complex64::new(0.0, scale), zero],
        [zero, Complex64::new(scale, 0.0)]
    ];
    let right_basis = mat![
        [Complex64::new(scale, 0.0)],
        [Complex64::new(0.0, scale)],
        [zero],
        [zero]
    ];
    let left_projector = product(left_basis.as_ref(), adjoint(left_basis.as_ref()).as_ref());
    let left = LogicalSpace::<Left>::from_dense_projector(
        left_projector.as_ref(),
        left_basis.as_ref(),
        policy,
    )?;
    let right = LogicalSpace::<Right>::from_isometry(right_basis.as_ref(), policy)?;
    let mut body = QuantumRegionBuilder::new(2, 0)?;
    body.gate(Gate::H, &[body.qubit(0)?], &[])?;
    body.gate(Gate::S, &[body.qubit(1)?], &[])?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let source = quest_qsvt::materialize_oracle(&oracle, policy)?;
    let block = product(
        adjoint(left_basis.as_ref()).as_ref(),
        product(source.as_ref(), right_basis.as_ref()).as_ref(),
    );
    let encoding = EncodingBuilder::new()
        .oracle(oracle.clone())
        .left(left)
        .right(right)
        .normalization(1.0)?
        .build()?;
    let transform = TransformBuilder::new()
        .encoding(encoding.clone())
        .standard(PhaseSequence::<WxSymmetric>::builder(vec![0.2, 0.2]).build()?)
        .build()?;
    let expected = Mat::from_fn(2, 1, |row, col| block[(row, col)].mul(0.4_f64.sin()));
    expect_matrix(transform.materialize_block()?.as_ref(), expected.as_ref());
    for instruction in transform.main().instructions() {
        if let quest_circuit::Operation::Oracle { fragment, .. } = instruction.operation() {
            expect_true!(fragment.shares_storage_with(&oracle));
        }
    }
    let controls = ControlSequence::builder()
        .angles(&[0.3, -0.2], &[0.4, 0.1])?
        .build()?;
    let coefficients = control_coefficients(&controls);
    let gram = product(adjoint(block.as_ref()).as_ref(), block.as_ref());
    let expected = product(
        block.as_ref(),
        chebyshev(&coefficients, gram.as_ref()).as_ref(),
    );
    let transform = TransformBuilder::new()
        .encoding(encoding)
        .multiplication_odd(
            quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls),
        )
        .build()?;
    expect_matrix(transform.materialize_block()?.as_ref(), expected.as_ref());
    Ok(())
}
