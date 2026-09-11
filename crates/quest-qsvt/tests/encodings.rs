use faer::mat;
use googletest::prelude::*;
use quest_circuit::{Gate, OracleFragment, ProgramBuilder};
use quest_qsvt::ExplicitUnitaryPremise;
use quest_qsvt::{EncodingBuilder, Left, LogicalSpace, NumericalPolicy, Right};

#[gtest]
fn explicit_unitary_premise_keeps_large_coordinate_encodings_compact() -> Result<()> {
    let policy = NumericalPolicy { max_bytes: 16384 };
    let mut body = ProgramBuilder::new(30, 0)?;
    body.gate(Gate::X, &[body.qubit(29)?], &[])?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let encoding = EncodingBuilder::new()
        .oracle(oracle)
        .left(LogicalSpace::<Left>::coordinates(
            1usize << 30,
            &[7, 2],
            policy,
        )?)
        .right(LogicalSpace::<Right>::coordinates(
            1usize << 30,
            &[3],
            policy,
        )?)
        .normalization(2.0)?
        .policy(policy)
        .unitarity_assumption(ExplicitUnitaryPremise::new(
            "analytically unitary source gates",
        )?)
        .build()?;
    expect_true!(encoding.left().dense_isometry().is_none());
    expect_that!(
        encoding.left().storage_bytes()?,
        eq(size_of::<[usize; 2]>())
    );
    expect_true!(encoding.left().isometry_snapshot(policy).is_err());
    let controls = quest_qsp::ControlSequence::builder()
        .angles(&[0.2, 0.3], &[0.1, 0.4])?
        .build()?;
    let full = quest_qsvt::TransformBuilder::new()
        .encoding(encoding.clone())
        .hermitianized_full(controls.clone())
        .build()?;
    expect_that!(full.input().logical_dimension(), eq(3));
    let transform = quest_qsvt::TransformBuilder::new()
        .encoding(encoding)
        .multiplication_odd(controls)
        .build()?;
    expect_that!(transform.main().num_qubits(), eq(32));
    expect_true!(transform.bridge().is_some());
    Ok(())
}

#[gtest]
fn whole_oracle_admission_does_not_reuse_individual_gate_tolerances() -> Result<()> {
    let matrix = mat![
        [
            quest_qsvt::Complex64::new(1.0 + 4e-13, 0.0),
            quest_qsvt::Complex64::new(0.0, 0.0)
        ],
        [
            quest_qsvt::Complex64::new(0.0, 0.0),
            quest_qsvt::Complex64::new(1.0, 0.0)
        ]
    ];
    let numerical = quest_circuit::NumericalOperator::from_view(
        matrix.as_ref(),
        NumericalPolicy::default().matrix_policy(),
    )?;
    let mut body = ProgramBuilder::new(1, 0)?;
    for _ in 0..10 {
        body.numerical(numerical.clone(), &[body.qubit(0)?], &[])?;
    }
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let builder = EncodingBuilder::new()
        .oracle(oracle)
        .left(LogicalSpace::<Left>::coordinates(
            2,
            &[0],
            NumericalPolicy::default(),
        )?)
        .right(LogicalSpace::<Right>::coordinates(
            2,
            &[0],
            NumericalPolicy::default(),
        )?)
        .normalization(1.0)?;
    expect_true!(matches!(
        builder.build(),
        Err(quest_qsvt::Error::Residual {
            operation: "whole oracle unitarity",
            ..
        })
    ));
    Ok(())
}

#[gtest]
fn coordinate_spaces_keep_independent_dimensions_and_caller_order() -> Result<()> {
    let left = LogicalSpace::<Left>::coordinates(4, &[3, 0], NumericalPolicy::default())?;
    let right = LogicalSpace::<Right>::coordinates(4, &[1], NumericalPolicy::default())?;
    expect_that!(left.logical_dimension(), eq(2));
    expect_that!(right.logical_dimension(), eq(1));
    expect_that!(
        left.isometry_snapshot(NumericalPolicy::default())?[(3, 0)].re,
        eq(1.0)
    );
    expect_that!(
        left.isometry_snapshot(NumericalPolicy::default())?[(0, 1)].re,
        eq(1.0)
    );
    expect_true!(
        LogicalSpace::<Left>::coordinates(4, &[1, 1], NumericalPolicy::default()).is_err()
    );
    Ok(())
}

#[gtest]
fn complex_isometry_and_dense_projector_keep_conjugate_transpose() -> Result<()> {
    use quest_qsvt::Complex64;
    let basis = mat![[Complex64::new(0.0, 1.0)], [Complex64::new(0.0, 0.0)]];
    let space = LogicalSpace::<Left>::from_isometry(basis.as_ref(), NumericalPolicy::default())?;
    let projector = space.projector_matrix(NumericalPolicy::default())?;
    expect_that!(projector[(0, 0)], eq(Complex64::new(1.0, 0.0)));
    expect_that!(projector[(1, 1)], eq(Complex64::new(0.0, 0.0)));
    let rebuilt = LogicalSpace::<Right>::from_dense_projector(
        projector.as_ref(),
        basis.as_ref(),
        NumericalPolicy::default(),
    )?;
    expect_that!(
        rebuilt.isometry_snapshot(NumericalPolicy::default())?[(0, 0)],
        eq(Complex64::new(0.0, 1.0))
    );
    Ok(())
}

#[gtest]
fn encoding_builder_preserves_oracle_storage_and_logical_interfaces() -> Result<()> {
    let mut body = ProgramBuilder::new(1, 0)?;
    body.gate(Gate::H, &[body.qubit(0)?], &[])?;
    let oracle = OracleFragment::builder(body.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let encoding = EncodingBuilder::new()
        .oracle(oracle.clone())
        .left(LogicalSpace::<Left>::coordinates(
            2,
            &[0],
            NumericalPolicy::default(),
        )?)
        .right(LogicalSpace::<Right>::coordinates(
            2,
            &[1],
            NumericalPolicy::default(),
        )?)
        .normalization(2.0)?
        .build()?;
    expect_true!(encoding.oracle().shares_storage_with(&oracle));
    expect_that!(encoding.normalization().get(), eq(2.0));
    expect_that!(encoding.left().logical_dimension(), eq(1));
    expect_that!(encoding.right().logical_dimension(), eq(1));
    Ok(())
}
