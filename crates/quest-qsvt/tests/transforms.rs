use faer::mat;
use googletest::prelude::*;
use quest_circuit::{NumericalOperator, OracleFragment, QuantumRegionBuilder};
use quest_qsp::{ControlSequence, PhaseSequence, WxSymmetric};
use quest_qsvt::{
    Complex64, EncodingBuilder, Left, LogicalSpace, NumericalPolicy, ProjectedEncoding, Right,
    TransformBuilder, TransformContinuation,
};
use std::ops::{Add, Mul, Sub};

fn scalar_encoding(value: f64) -> quest_qsvt::Result<ProjectedEncoding> {
    let policy = NumericalPolicy::default();
    let remainder = value.mul_add(-value, 1.0).sqrt();
    let c = |v| Complex64::new(v, 0.0);
    let matrix = mat![[c(value), c(remainder)], [c(remainder), c(-value)]];
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    body.numerical(
        NumericalOperator::from_view(matrix.as_ref(), policy.matrix_policy())?,
        &[body.qubit(0)?],
        &[],
    )?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    EncodingBuilder::new()
        .oracle(oracle)
        .left(LogicalSpace::<Left>::coordinates(2, &[0], policy)?)
        .right(LogicalSpace::<Right>::coordinates(2, &[0], policy)?)
        .normalization(1.0)?
        .build()
}

#[gtest]
fn standard_degree_zero_and_one_keep_exact_readout_and_counts() -> Result<()> {
    for phases in [vec![0.2], vec![0.2, 0.2]] {
        let degree = phases.len().saturating_sub(1);
        let transform = TransformBuilder::new()
            .encoding(scalar_encoding(0.3)?)
            .standard(PhaseSequence::<WxSymmetric>::builder(phases).build()?)
            .build()?;
        let expected = if degree == 0 {
            0.2_f64.sin()
        } else {
            0.3 * 0.4_f64.sin()
        };
        let block = transform.materialize_block()?;
        expect_that!(
            block[(0, 0)].sub(Complex64::new(expected, 0.0)).norm(),
            lt(1e-13)
        );
        expect_that!(transform.query_counts().semantic, eq(degree));
        expect_that!(
            transform.query_counts().source_forward,
            eq(2usize.saturating_mul(degree))
        );
        expect_that!(transform.query_counts().source_adjoint, eq(0));
        expect_true!(transform.bridge().is_none());
        expect_true!(transform.continuation().is_none());
        expect_true!(matches!(
            transform.continuation_stage(),
            TransformContinuation::Direct
        ));
    }
    Ok(())
}

#[gtest]
fn odd_multiplication_retains_bridge_and_source_at_reduced_degree_zero() -> Result<()> {
    let controls = ControlSequence::builder()
        .angles(&[0.31], &[0.47])?
        .build()?;
    let expected = Complex64::from_polar(0.3 * 0.31_f64.sin(), 0.47);
    let transform = TransformBuilder::new()
        .encoding(scalar_encoding(0.3)?)
        .multiplication_odd(
            quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls),
        )
        .build()?;
    expect_true!(transform.bridge().is_some());
    expect_true!(transform.continuation().is_some());
    expect_true!(matches!(
        transform.continuation_stage(),
        TransformContinuation::Projected { .. }
    ));
    expect_that!(transform.query_counts().source_forward, eq(1));
    expect_that!(transform.query_counts().source_adjoint, eq(0));
    expect_that!(
        transform.materialize_block()?[(0, 0)].sub(expected).norm(),
        lt(1e-13)
    );
    Ok(())
}

fn coefficients(controls: &ControlSequence) -> Vec<Complex64> {
    let zero = Complex64::new(0.0, 0.0);
    let mut polynomial = vec![[[zero; 2]; 2]];
    if let Some((last, earlier)) = controls.matrices().split_last() {
        if let Some(first) = polynomial.first_mut() {
            *first = *last;
        }
        for control in earlier.iter().rev() {
            let mut next = vec![[[zero; 2]; 2]; polynomial.len().saturating_add(1)];
            for (degree, matrix) in polynomial.iter().enumerate() {
                for row in 0..2 {
                    for col in 0..2 {
                        let upper = control
                            .get(row)
                            .and_then(|r| r.first())
                            .copied()
                            .unwrap_or_default()
                            .mul(
                                matrix
                                    .first()
                                    .and_then(|r| r.get(col))
                                    .copied()
                                    .unwrap_or_default(),
                            );
                        let lower = control
                            .get(row)
                            .and_then(|r| r.get(1))
                            .copied()
                            .unwrap_or_default()
                            .mul(
                                matrix
                                    .get(1)
                                    .and_then(|r| r.get(col))
                                    .copied()
                                    .unwrap_or_default(),
                            );
                        if let Some(entry) = next
                            .get_mut(degree.saturating_add(1))
                            .and_then(|m| m.get_mut(row))
                            .and_then(|r| r.get_mut(col))
                        {
                            *entry = entry.add(upper);
                        }
                        if let Some(entry) = next
                            .get_mut(degree)
                            .and_then(|m| m.get_mut(row))
                            .and_then(|r| r.get_mut(col))
                        {
                            *entry = entry.add(lower);
                        }
                    }
                }
            }
            polynomial = next;
        }
    }
    polynomial
        .iter()
        .map(|matrix| {
            matrix
                .first()
                .and_then(|row| row.first())
                .copied()
                .unwrap_or_default()
        })
        .collect()
}
fn evaluate(coefficients: &[Complex64], x: f64, parity: Option<usize>) -> Complex64 {
    let mut previous = 1.0;
    let mut current = x;
    let mut value = Complex64::new(0.0, 0.0);
    for (index, coefficient) in coefficients.iter().enumerate() {
        let t = match index {
            0 => 1.0,
            1 => x,
            _ => {
                let next = (2.0 * x).mul_add(current, -previous);
                previous = current;
                current = next;
                next
            }
        };
        if parity.is_none_or(|parity| index % 2 == parity) {
            value = value.add(coefficient.mul(t));
        }
    }
    value
}

#[gtest]
fn generalized_routes_preserve_complex_control_polynomials_and_query_counts() -> Result<()> {
    for count in [1usize, 2, 4] {
        let psi = [0.31, -0.22, 0.14, 0.53]
            .into_iter()
            .take(count)
            .collect::<Vec<_>>();
        let phi = [0.47, 0.91, -0.34, -0.28]
            .into_iter()
            .take(count)
            .collect::<Vec<_>>();
        let controls = ControlSequence::builder().angles(&psi, &phi)?.build()?;
        let polynomial = coefficients(&controls);
        let degree = count.saturating_sub(1);
        let direct = TransformBuilder::new()
            .encoding(scalar_encoding(0.3)?)
            .direct(
                quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                    controls.clone(),
                ),
            )
            .build()?;
        expect_that!(
            direct.materialize_block()?[(0, 0)]
                .sub(evaluate(&polynomial, 0.3, None))
                .norm(),
            lt(1e-13)
        );
        expect_that!(direct.query_counts().source_forward, eq(degree));
        expect_that!(direct.query_counts().source_adjoint, eq(0));
        let even = TransformBuilder::new()
            .encoding(scalar_encoding(0.3)?)
            .hermitianized_even(
                quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                    controls.clone(),
                )
                .even_component(),
            )
            .build()?;
        let odd = TransformBuilder::new()
            .encoding(scalar_encoding(0.3)?)
            .hermitianized_odd(
                quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                    controls.clone(),
                )
                .odd_component(),
            )
            .build()?;
        expect_that!(
            even.materialize_block()?[(0, 0)]
                .sub(evaluate(&polynomial, 0.3, Some(0)))
                .norm(),
            lt(1e-13)
        );
        expect_that!(
            odd.materialize_block()?[(0, 0)]
                .sub(evaluate(&polynomial, 0.3, Some(1)))
                .norm(),
            lt(1e-13)
        );
        expect_that!(odd.query_counts().source_forward, eq(degree));
        expect_that!(odd.query_counts().source_adjoint, eq(degree));
        let full = TransformBuilder::new()
            .encoding(scalar_encoding(0.3)?)
            .hermitianized_full(
                quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                    controls.clone(),
                ),
            )
            .build()?;
        let full_block = full.materialize_block()?;
        expect_that!(
            full_block[(0, 0)]
                .sub(evaluate(&polynomial, 0.3, Some(0)))
                .norm(),
            lt(1e-13)
        );
        expect_that!(
            full_block[(0, 1)]
                .sub(evaluate(&polynomial, 0.3, Some(1)))
                .norm(),
            lt(1e-13)
        );
        let reduced_even = TransformBuilder::new()
            .encoding(scalar_encoding(0.3)?)
            .multiplication_even(
                quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls.clone()),
            )
            .build()?;
        let reduced_odd = TransformBuilder::new()
            .encoding(scalar_encoding(0.3)?)
            .multiplication_odd(
                quest_qsvt::RouteResponse::<quest_qsvt::GramArgument>::imported(controls),
            )
            .build()?;
        let reduced = evaluate(&polynomial, 0.09, None);
        expect_that!(
            reduced_even.materialize_block()?[(0, 0)]
                .sub(reduced)
                .norm(),
            lt(1e-13)
        );
        expect_that!(
            reduced_odd.materialize_block()?[(0, 0)]
                .sub(reduced.mul(0.3))
                .norm(),
            lt(1e-13)
        );
        expect_that!(
            reduced_odd.query_counts().source_forward,
            eq(degree.saturating_add(1))
        );
        expect_that!(reduced_odd.query_counts().source_adjoint, eq(degree));
    }
    Ok(())
}

#[gtest]
fn direct_route_rejects_full_oracle_nonhermiticity_and_projector_mismatch() -> Result<()> {
    let policy = NumericalPolicy::default();
    let matrix = mat![
        [Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)],
        [Complex64::new(0.0, 0.0), Complex64::new(0.0, 1.0)]
    ];
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    body.numerical(
        NumericalOperator::from_view(matrix.as_ref(), policy.matrix_policy())?,
        &[body.qubit(0)?],
        &[],
    )?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let encoding = EncodingBuilder::new()
        .oracle(oracle)
        .left(LogicalSpace::<Left>::coordinates(2, &[0], policy)?)
        .right(LogicalSpace::<Right>::coordinates(2, &[0], policy)?)
        .normalization(1.0)?
        .build()?;
    // The projected scalar is exactly Hermitian; the complete oracle is not.
    expect_that!(
        encoding.logical_matrix()?[(0, 0)],
        eq(Complex64::new(1.0, 0.0))
    );
    let controls = ControlSequence::builder()
        .angles(&[0.2, 0.3], &[0.1, 0.4])?
        .build()?;
    expect_true!(matches!(
        TransformBuilder::new()
            .encoding(encoding)
            .direct(
                quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(
                    controls.clone()
                )
            )
            .build(),
        Err(quest_qsvt::Error::Residual {
            operation: "full oracle Hermiticity",
            ..
        })
    ));
    let original = scalar_encoding(0.3)?;
    let mismatch = EncodingBuilder::new()
        .oracle(original.oracle().clone())
        .left(LogicalSpace::<Left>::coordinates(2, &[1], policy)?)
        .right(LogicalSpace::<Right>::coordinates(2, &[0], policy)?)
        .normalization(1.0)?
        .build()?;
    expect_true!(
        TransformBuilder::new()
            .encoding(mismatch)
            .direct(quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(controls))
            .build()
            .is_err()
    );
    Ok(())
}

#[gtest]
fn explicit_idle_high_qubits_preserve_block_and_fix_every_projection_to_zero() -> Result<()> {
    let encoding = scalar_encoding(0.3)?;
    let controls = ControlSequence::builder()
        .angles(&[0.2, -0.3], &[0.4, 0.1])?
        .build()?;
    let ordinary = TransformBuilder::new()
        .encoding(encoding.clone())
        .hermitianized_full(
            quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(controls.clone()),
        )
        .build()?;
    let layout =
        quest_qsvt::OperandLayout::new(3, vec![0], 2, Some(1))?.with_idle_high_qubits(2)?;
    expect_eq!(layout.num_qubits(), 5);
    expect_eq!(layout.num_idle_high_qubits(), 2);
    expect_eq!(layout.source(), &[0]);
    let extended = TransformBuilder::new()
        .encoding(encoding)
        .operands(layout)
        .hermitianized_full(
            quest_qsvt::RouteResponse::<quest_qsvt::HermitianArgument>::imported(controls),
        )
        .build()?;
    expect_eq!(extended.query_counts(), ordinary.query_counts());
    let expected = ordinary.materialize_block()?;
    let actual = extended.materialize_block()?;
    for r in 0..expected.nrows() {
        for c in 0..expected.ncols() {
            expect_that!(actual[(r, c)].sub(expected[(r, c)]).norm(), lt(1e-12));
        }
    }
    for projection in [extended.input(), extended.output()] {
        for qubit in [3, 4] {
            expect_true!(
                projection
                    .controls()
                    .iter()
                    .any(|c| c.qubit == qubit && !c.value)
            );
        }
        let basis = projection.materialize_isometry(5, NumericalPolicy::default())?;
        for row in 8..32 {
            for col in 0..basis.ncols() {
                expect_eq!(basis[(row, col)], Complex64::new(0.0, 0.0));
            }
        }
    }
    expect_true!(
        quest_qsvt::OperandLayout::canonical(1, false)?
            .with_idle_high_qubits(usize::MAX)
            .is_err()
    );
    Ok(())
}
