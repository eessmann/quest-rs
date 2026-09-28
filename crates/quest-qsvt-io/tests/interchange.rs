use googletest::prelude::*;
use quest_qsvt_io::{
    Complex64, GeneralizedAngleInput, IoPolicy, QspInput, SparseFormat, SparseMatrix,
    read_qsp_execution_json, read_qsp_json, write_qsp_execution_json, write_qsp_json,
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
    let QspInput::GeneralizedAngles(angles) = legacy else {
        return fail!("wrong generalized payload");
    };
    let original = angles.controls().matrices().to_vec();
    let frozen = QspInput::GeneralizedMatrices(angles.controls().clone());
    let loaded = read_qsp_json(&write_qsp_json(&frozen)?, IoPolicy::default())?;
    let QspInput::GeneralizedMatrices(controls) = loaded else {
        return fail!("wrong frozen payload");
    };
    expect_that!(controls.matrices(), eq(original.as_slice()));
    expect_true!(read_qsp_json(r#"{"theta":[1.0],"lambda":[2.0]}"#, IoPolicy::default()).is_err());
    Ok(())
}

#[gtest]
fn imported_angles_keep_source_and_exact_frozen_execution_words() -> Result<()> {
    let source = r#"{"psi":[0.2,-0.4],"phi":[-0.7,1.2]}"#;
    let admitted = read_qsp_json(source, IoPolicy::default())?;
    let QspInput::GeneralizedAngles(original) = &admitted else {
        return fail!("wrong payload");
    };
    let matrix_bits = original
        .controls()
        .matrices()
        .iter()
        .flat_map(|matrix| {
            matrix
                .iter()
                .flat_map(|row| row.iter().flat_map(|z| [z.re.to_bits(), z.im.to_bits()]))
        })
        .collect::<Vec<_>>();
    let source_json = write_qsp_json(&admitted)?;
    expect_false!(source_json.contains("controls"));
    expect_true!(read_qsp_execution_json(&source_json, IoPolicy::default()).is_err());
    let execution_json = write_qsp_execution_json(&admitted)?;
    expect_true!(execution_json.contains("control_words"));
    expect_true!(execution_json.contains("psi_words"));
    expect_true!(execution_json.contains("phi_words"));
    let QspInput::GeneralizedAngles(roundtrip) =
        read_qsp_execution_json(&execution_json, IoPolicy::default())?
    else {
        return fail!("source provenance lost");
    };
    expect_that!(roundtrip.psi(), eq([0.2, -0.4].as_slice()));
    expect_that!(roundtrip.phi(), eq([-0.7, 1.2].as_slice()));
    let roundtrip_bits = roundtrip
        .controls()
        .matrices()
        .iter()
        .flat_map(|matrix| {
            matrix
                .iter()
                .flat_map(|row| row.iter().flat_map(|z| [z.re.to_bits(), z.im.to_bits()]))
        })
        .collect::<Vec<_>>();
    expect_true!(roundtrip_bits == matrix_bits);
    Ok(())
}

#[gtest]
fn checked_angle_constructor_has_one_admitted_execution_authority() -> Result<()> {
    expect_true!(GeneralizedAngleInput::new(vec![0.1], vec![]).is_err());
    expect_true!(GeneralizedAngleInput::new(vec![f64::NAN], vec![0.2]).is_err());
    let payload = GeneralizedAngleInput::new(vec![0.2], vec![-0.7])?;
    expect_that!(payload.controls().matrices().len(), eq(1));
    expect_that!(payload.psi(), eq([0.2].as_slice()));
    Ok(())
}

#[gtest]
fn frozen_execution_retains_signed_zero_source_angle_bits() -> Result<()> {
    let input = QspInput::GeneralizedAngles(GeneralizedAngleInput::new(vec![-0.0], vec![-0.0])?);
    let encoded = write_qsp_execution_json(&input)?;
    let QspInput::GeneralizedAngles(decoded) =
        read_qsp_execution_json(&encoded, IoPolicy::default())?
    else {
        return fail!("source provenance lost");
    };
    expect_that!(decoded.psi()[0].to_bits(), eq((-0.0_f64).to_bits()));
    expect_that!(decoded.phi()[0].to_bits(), eq((-0.0_f64).to_bits()));
    Ok(())
}

#[gtest]
fn frozen_execution_rejects_competing_matrix_and_angle_authorities() -> Result<()> {
    let input = QspInput::GeneralizedAngles(GeneralizedAngleInput::new(vec![0.2], vec![-0.7])?);
    let mut encoded: serde_json::Value = serde_json::from_str(&write_qsp_execution_json(&input)?)?;
    encoded["controls"] = serde_json::json!([]);
    expect_true!(read_qsp_execution_json(&encoded.to_string(), IoPolicy::default()).is_err());
    encoded.as_object_mut().expect("object").remove("controls");
    encoded["psi"] = serde_json::json!([0.2]);
    expect_true!(read_qsp_execution_json(&encoded.to_string(), IoPolicy::default()).is_err());
    Ok(())
}

#[gtest]
fn execution_matrix_words_preserve_subnormal_and_signed_zero_bits() -> Result<()> {
    let tiny = f64::from_bits(1);
    let controls = quest_qsp::ControlSequence::builder()
        .matrices(vec![[
            [Complex64::new(1.0, -0.0), Complex64::new(tiny, -0.0)],
            [Complex64::new(-tiny, 0.0), Complex64::new(1.0, 0.0)],
        ]])
        .build()?;
    let expected = controls.matrices().to_vec();
    let input = QspInput::GeneralizedMatrices(controls);
    let encoded = write_qsp_execution_json(&input)?;
    let parsed: serde_json::Value = serde_json::from_str(&encoded)?;
    expect_true!(parsed.get("control_words").is_some());
    expect_true!(parsed.get("controls").is_none());
    let QspInput::GeneralizedMatrices(decoded) =
        read_qsp_execution_json(&encoded, IoPolicy::default())?
    else {
        return fail!("wrong payload");
    };
    for (left, right) in expected[0]
        .iter()
        .flatten()
        .zip(decoded.matrices()[0].iter().flatten())
    {
        expect_that!(left.re.to_bits(), eq(right.re.to_bits()));
        expect_that!(left.im.to_bits(), eq(right.im.to_bits()));
    }
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

#[gtest]
fn sparse_builder_budget_counts_pointer_storage() {
    let policy = IoPolicy {
        max_bytes: 48,
        ..IoPolicy::default()
    };
    let sparse = SparseMatrix::builder(2, 2)
        .entries(vec![Complex64::new(1.0, 0.0)], vec![0], vec![0, 1, 1])
        .build(policy);
    expect_true!(matches!(sparse, Err(quest_qsvt_io::Error::Budget(_))));
}

#[gtest]
fn zero_entry_sparse_budget_has_exact_pointer_boundary() {
    const BUDGET: usize = 1616;
    let build = |max_bytes| {
        SparseMatrix::builder(100, 1)
            .entries(vec![], vec![], vec![0; 101])
            .build(IoPolicy {
                max_bytes,
                ..IoPolicy::default()
            })
    };
    expect_true!(build(BUDGET).is_ok());
    expect_true!(matches!(
        build(BUDGET.saturating_sub(1)),
        Err(quest_qsvt_io::Error::Budget(_))
    ));
}

#[gtest]
fn sparse_builder_rejects_retained_spare_capacity() {
    let data = Vec::with_capacity(1_000_000);
    let indices = Vec::with_capacity(1_000_000);
    let sparse = SparseMatrix::builder(1, 1)
        .entries(data, indices, vec![0, 0])
        .build(IoPolicy {
            max_bytes: 32,
            ..IoPolicy::default()
        });
    expect_true!(matches!(sparse, Err(quest_qsvt_io::Error::Budget(_))));
}
