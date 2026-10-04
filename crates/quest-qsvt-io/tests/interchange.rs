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
	expect_that!(converted.polynomial().coefficients().len(), eq(4));
	expect_that!(converted.polynomial().coefficients()[3].re, eq(0.5));
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
fn interchange_rejects_duplicate_known_fields_before_payload_admission() {
	for text in [
		r#"{"coefficients":[0.2],"coefficients":[0.3]}"#,
		r#"{"coefficients":[0.2],"basis":"Monomial","basis":"Chebyshev"}"#,
		r#"{"coefficients":[0.2],"minimum_order":0,"minimum_order":1}"#,
		r#"{"coefficients":[0.2],"parameters":[],"parameters":[]}"#,
		r#"{"convention":"pyqsp-wx-symmetric","angles":[0.2],"angles":[0.3]}"#,
		r#"{"convention":"pyqsp-wx-symmetric","convention":"pyqsp-wx-laurent","angles":[0.2]}"#,
		r#"{"psi":[0.2],"psi":[0.3],"phi":[0.4]}"#,
		r#"{"psi":[0.2],"phi":[0.3],"phi":[0.4]}"#,
	] {
		expect_true!(read_qsp_json(text, IoPolicy::default()).is_err(), "{text}");
	}
}

#[gtest]
fn interchange_rejects_competing_payload_families_and_orphan_angle_words() {
	for text in [
		r#"{"convention":"pyqsp-wx-symmetric","angles":[0.2],"coefficients":[0.3]}"#,
		r#"{"convention":"pyqsp-wx-symmetric","angles":[0.2],"psi":[0.3],"phi":[0.4]}"#,
		r#"{"coefficients":[0.2],"psi":[0.3],"phi":[0.4]}"#,
		r#"{"coefficients":[0.2],"psi_words":[0],"phi_words":[0]}"#,
		r#"{"psi_words":[0],"phi_words":[0]}"#,
		r#"{"convention":"gqsp-matrix-upper-left-v1","controls":[[[1,0],[0,1]]],"coefficients":[0.2]}"#,
	] {
		expect_true!(read_qsp_json(text, IoPolicy::default()).is_err(), "{text}");
	}
}

#[gtest]
fn interchange_known_null_fields_cannot_hide_competing_payloads() {
	for text in [
		r#"{"convention":"pyqsp-wx-symmetric","angles":[0.2],"coefficients":null}"#,
		r#"{"coefficients":[0.2],"psi_words":null,"phi_words":null}"#,
		r#"{"coefficients":[0.2],"basis":null}"#,
	] {
		expect_true!(read_qsp_json(text, IoPolicy::default()).is_err(), "{text}");
	}
}

#[gtest]
fn interchange_rejects_overdeep_or_unrepresentable_unknown_metadata() {
	let nested = format!(
		"{{\"convention\":\"pyqsp-wx-symmetric\",\"angles\":[0.2],\"metadata\":{}0{}}}",
		"[".repeat(130),
		"]".repeat(130),
	);
	expect_true!(read_qsp_json(&nested, IoPolicy::default()).is_err());
	expect_true!(
		read_qsp_json(
			r#"{"convention":"pyqsp-wx-symmetric","angles":[0.2],"metadata":1e999}"#,
			IoPolicy::default(),
		)
		.is_err()
	);
}

#[gtest]
fn polynomial_metadata_defaults_parameters_and_support_survive_interchange() -> Result<()> {
	for source in [
		r#"{"coefficients":[0.2,[0.1,-0.0]],"metadata":{"label":"quoted \"name\"","tags":["μ",null]}}"#,
		r#"{"coefficients":[0.2],"basis":"Laguerre","parameters":[0.5],"minimum_order":2,"label":"L"}"#,
		r#"{"coefficients":[[0.2,0.1]],"basis":"Jacobi","parameters":[0.3,0.5]}"#,
		r#"{"coefficients":[0.2,0.1],"basis":"Laurent","minimum_order":-1}"#,
	] {
		let input = read_qsp_json(source, IoPolicy::default())?;
		let original: serde_json::Value = serde_json::from_str(source)?;
		let restored: serde_json::Value = serde_json::from_str(&write_qsp_json(&input)?)?;
		expect_that!(restored, eq(&original));
	}
	let QspInput::Polynomial(defaulted) =
		read_qsp_json(r#"{"coefficients":[0.2,0.1]}"#, IoPolicy::default())?
	else {
		return fail!("wrong polynomial payload");
	};
	expect_that!(
		defaulted.to_chebyshev()?.polynomial().coefficients(),
		eq([Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.0)].as_slice())
	);
	Ok(())
}

#[gtest]
fn frozen_controls_remain_authority_when_source_angles_differ() -> Result<()> {
	let source = r#"{"convention":"gqsp-matrix-upper-left-v1","controls":[[[1,0],[0,1]]],"psi":[0.2],"phi":[0.3]}"#;
	let input = read_qsp_json(source, IoPolicy::default())?;
	let frozen = write_qsp_execution_json(&input)?;
	let QspInput::GeneralizedAngles(loaded) =
		read_qsp_execution_json(&frozen, IoPolicy::default())?
	else {
		return fail!("source provenance lost");
	};
	expect_that!(loaded.psi(), eq([0.2].as_slice()));
	expect_that!(loaded.phi(), eq([0.3].as_slice()));
	expect_that!(
		loaded.controls().matrices(),
		eq([[
			[Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)],
			[Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0)],
		]]
		.as_slice())
	);
	Ok(())
}

#[gtest]
fn frozen_payload_duplicate_fields_are_rejected_for_both_readers() -> Result<()> {
	let angles = QspInput::GeneralizedAngles(GeneralizedAngleInput::new(vec![0.2], vec![0.3])?);
	let encoded = write_qsp_execution_json(&angles)?;
	let value: serde_json::Value = serde_json::from_str(&encoded)?;
	for key in ["control_words", "psi_words", "phi_words"] {
		let component = value
			.get(key)
			.ok_or_else(|| std::io::Error::other("missing frozen field"))?;
		let duplicate = format!(
			"{{\"{key}\":{component},{}",
			encoded.trim_start_matches('{')
		);
		expect_true!(
			read_qsp_json(&duplicate, IoPolicy::default()).is_err(),
			"{key}"
		);
		expect_true!(
			read_qsp_execution_json(&duplicate, IoPolicy::default()).is_err(),
			"{key}"
		);
	}
	let matrices = r#"{"convention":"gqsp-matrix-upper-left-v1","controls":[[[1,0],[0,1]]],"controls":[[[1,0],[0,1]]]}"#;
	expect_true!(read_qsp_json(matrices, IoPolicy::default()).is_err());
	Ok(())
}

#[gtest]
fn typed_interchange_retains_shape_support_and_count_admission() {
	for source in [
		r#"{"coefficients":[[0.2]]}"#,
		r#"{"coefficients":[[0.2,0.3,0.4]]}"#,
		r#"{"coefficients":[0.2],"minimum_order":-1}"#,
		r#"{"coefficients":[0.2],"minimum_order":2147483648}"#,
		r#"{"coefficients":[0.2],"basis":"Jacobi","parameters":[0.3]}"#,
		r#"{"convention":"gqsp-matrix-upper-left-v1","controls":[[[1,0]]] }"#,
		r#"{"convention":"gqsp-matrix-words-v1","control_words":[[[[4607182418800017408,0],[0,0]],[[0,0],[4607182418800017408,0]]]],"psi_words":[18442240474082181120],"phi_words":[0]}"#,
	] {
		expect_true!(
			read_qsp_json(source, IoPolicy::default()).is_err(),
			"{source}"
		);
	}
	let policy = IoPolicy {
		max_coefficients: 1,
		..IoPolicy::default()
	};
	expect_true!(matches!(
		read_qsp_json(r#"{"coefficients":[0.2,0.3]}"#, policy),
		Err(quest_qsvt_io::Error::Budget(_))
	));
	expect_true!(matches!(
		read_qsp_json(r#"{"coefficients":[0.2],"minimum_order":1}"#, policy),
		Err(quest_qsvt_io::Error::Budget(_))
	));
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

#[gtest]
fn sparse_io_exposes_canonical_numerical_storage() -> Result<()> {
	let sparse = SparseMatrix::builder(1, 2)
		.entries(
			vec![Complex64::new(1.0, 0.0), Complex64::new(2.0, 0.0)],
			vec![1, 1],
			vec![0, 2],
		)
		.build(IoPolicy::default())?;
	expect_that!(sparse.as_numerics().nnz(), eq(1));
	expect_that!(
		sparse.as_numerics().data(),
		eq(&[Complex64::new(3.0, 0.0)][..])
	);
	Ok(())
}
