use faer::{MatRef, mat};
use googletest::prelude::*;
use quest_qsp::{PhaseSequence, WxImaginaryU00, WxLaurent, WxSymmetric};
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
			.standard(PhaseSequence::<WxImaginaryU00>::builder(vec![huge]).build()?)
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
		}
	}
	Ok(())
}

#[cfg(feature = "certification")]
#[gtest]
fn certified_converted_phases_bind_to_rectangular_transform_and_analysis() -> Result<()> {
	use quest_polynomial::{Chebyshev, Limits, Polynomial};
	use quest_qsp::{
		SynthesisBuilder,
		certification::{CertificationBuilder, CertificationPolicy},
	};
	use quest_qsvt::analysis::{Assumption, BlockEncodingBound, StandardPremises};
	let target = Polynomial::new(
		Chebyshev,
		vec![Complex64::new(0.0, 0.0), Complex64::new(0.25, 0.0)],
		Limits::default(),
	)?;
	let candidate = SynthesisBuilder::new()
		.real_parity_wx(&target)?
		.admit()?
		.complete()?
		.synthesize()?;
	let policy = CertificationPolicy::default();
	let certified = CertificationBuilder::new()
		.candidate(candidate)
		.policy(policy)?
		.certify()?;
	let projector = certified.certify_projector_phases(policy)?;
	let readout = projector.readout_phase().to_bits();
	let response_error = projector.response_bound().upper_f64();
	let input = mat![[Complex64::new(0.2, 0.1), Complex64::new(-0.1, 0.05)]];
	let encoding = DenseEncodingBuilder::new(input.as_ref(), NumericalPolicy::default())?
		.normalization(1.0)?
		.build()?;
	let transform = TransformBuilder::new()
		.encoding(encoding)
		.certified_standard(projector)
		.build()?;
	let evidence = transform.projector_certificate().or_fail()?;
	expect_eq!(evidence.readout_phase().to_bits(), readout);
	let actual = transform.materialize_block()?;
	expect_eq!(actual.nrows(), 1);
	expect_eq!(actual.ncols(), 2);
	for (actual, expected) in actual
		.as_ref()
		.row(0)
		.iter()
		.zip(input.as_ref().row(0).iter())
	{
		expect_that!(
			(*actual).sub((*expected).mul(0.25)).norm(),
			le(response_error.max(1e-13))
		);
	}
	let premise = Assumption::stated(
		"Exact projected encoding and completion hypotheses supplied for this fixture",
	)?;
	let premises = StandardPremises::for_transform(&transform)?
		.assume_projected_unitary_subspaces(premise.clone())
		.certified_actual_phase_response()?
		.assume_parity_and_completion(premise.clone())
		.build();
	expect_eq!(premises.assumptions().len(), 2);
	let bound = BlockEncodingBound::builder()
		.normalization(1.0)?
		.absolute_error(0.0)?
		.assume_contract(premise)
		.build()?;
	let report = transform
		.analysis()
		.encoding_bound(bound)?
		.standard_premises(premises)?
		.build()?;
	expect_true!(report.total().or_fail()?.upper() >= response_error);
	Ok(())
}
