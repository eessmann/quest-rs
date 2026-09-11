use googletest::prelude::*;
use quest_qsvt_io::{
    Complex64, IoPolicy, QspInput, SparseFormat, SparseMatrix, read_qsp_json, write_qsp_json,
};

#[gtest]
fn canonical_phase_and_polynomial_json_round_trip() -> Result<()> {
    let text = r#"{"convention":"pyqsp-wx-symmetric","angles":[0.2,0.3,0.2]}"#;
    let input = read_qsp_json(text, IoPolicy::default())?;
    let round_trip = read_qsp_json(&write_qsp_json(&input)?, IoPolicy::default())?;
    let QspInput::Symmetric(phases) = round_trip else {
        return fail!("wrong convention");
    };
    expect_that!(phases.values(), eq([0.2, 0.3, 0.2].as_slice()));
    let input = read_qsp_json(
        r#"{"coefficients":[0.0,0.5],"basis":"Chebyshev","parameters":[],"minimum_order":2}"#,
        IoPolicy::default(),
    )?;
    let QspInput::Polynomial(polynomial) = input else {
        return fail!("wrong payload");
    };
    let converted = polynomial.to_chebyshev()?;
    expect_that!(converted.polynomial.coefficients().len(), eq(4));
    expect_that!(converted.polynomial.coefficients()[3].re, eq(0.5));
    Ok(())
}

#[gtest]
fn complex_frozen_control_components_round_trip_exactly() -> Result<()> {
    let legacy = read_qsp_json(r#"{"psi":[0.2,0.4],"phi":[-0.7,1.2]}"#, IoPolicy::default())?;
    let QspInput::GeneralizedAngles { controls, .. } = legacy else {
        return fail!("wrong generalized payload");
    };
    let original = controls.matrices().to_vec();
    let frozen = QspInput::GeneralizedMatrices(controls);
    let loaded = read_qsp_json(&write_qsp_json(&frozen)?, IoPolicy::default())?;
    let QspInput::GeneralizedMatrices(controls) = loaded else {
        return fail!("wrong frozen payload");
    };
    expect_that!(controls.matrices(), eq(original.as_slice()));
    expect_true!(read_qsp_json(r#"{"theta":[1.0],"lambda":[2.0]}"#, IoPolicy::default()).is_err());
    Ok(())
}

#[gtest]
fn sparse_densification_keeps_complex_duplicates_and_one_based_csc() -> Result<()> {
    let sparse = SparseMatrix::builder(2, 2)
        .format(SparseFormat::Csc)
        .one_based()
        .entries(
            vec![
                Complex64::new(1.0, 2.0),
                Complex64::new(-0.25, 0.5),
                Complex64::new(3.0, -1.0),
            ],
            vec![2, 2, 1],
            vec![1, 3, 4],
        )
        .build(IoPolicy::default())?;
    let dense = sparse.densify(IoPolicy::default())?;
    expect_that!(dense[(1, 0)], eq(Complex64::new(0.75, 2.5)));
    expect_that!(dense[(0, 1)], eq(Complex64::new(3.0, -1.0)));
    expect_that!(dense[(0, 0)], eq(Complex64::new(0.0, 0.0)));
    Ok(())
}
