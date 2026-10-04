use googletest::{expect_true, gtest};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{
	Complex64, MatchingEncoding, NumericalPolicy, replay_transform::MatchingTransform,
};
#[gtest]
fn replay_rejects_nonfinite_amplitudes_generated_by_unitary_arithmetic() -> googletest::Result<()> {
	let matrix =
		SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![], SparseLimits::default())?;
	let encoding = MatchingEncoding::from_sparse(&matrix, NumericalPolicy::default())?;
	let sequence = PhaseSequence::<WxSymmetric>::builder(vec![0.0]).build()?;
	let transform = MatchingTransform::new(encoding, sequence, NumericalPolicy::default())?;
	let mut state = vec![Complex64::new(f64::MAX, 0.0); 4];
	expect_true!(
		transform
			.apply_reference(&mut state, false, NumericalPolicy::default())
			.is_err()
	);
	Ok(())
}

#[cfg(feature = "certification")]
#[gtest]
fn certified_schedule_uses_the_frozen_projector_payload() -> googletest::Result<()> {
	use quest_qsp::certification::CertificationBuilder;
	use quest_qsvt::{
		OperandLayout, TransformBuilder, materialize_program,
		reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence},
	};
	use std::ops::{Mul, Sub};
	let policy = NumericalPolicy::default();
	let matrix = SparseMatrix::from_triplets(
		1,
		1,
		SparseFormat::Csr,
		vec![(0, 0, Complex64::new(1.0, 0.0))],
		SparseLimits::default(),
	)?;
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	let bounds = SpectralBounds::new(
		1.0,
		1.0,
		SpectralEvidence::Analytic {
			description: "scalar identity".into(),
		},
	)?;
	let polynomial = ReciprocalPolynomial::geometric(&bounds, 1.0, 1e-3, 3, policy)?;
	let frozen = polynomial.synthesize(quest_qsp::Policy::default())?;
	let certificate = CertificationBuilder::new()
		.candidate(frozen)
		.policy(quest_qsp::certification::CertificationPolicy::default())?
		.certify()?
		.certify_projector_phases(quest_qsp::certification::CertificationPolicy::default())?;
	let oracle_width = encoding.num_qubits();
	let portable_certificate = CertificationBuilder::new()
		.candidate(certificate.source_candidate().clone())
		.policy(quest_qsp::certification::CertificationPolicy::default())?
		.certify()?
		.certify_projector_phases(quest_qsp::certification::CertificationPolicy::default())?;
	let portable = TransformBuilder::new()
		.encoding(encoding.projected_encoding(policy)?)
		.operands(OperandLayout::new(
			oracle_width
				.checked_add(1)
				.ok_or(quest_qsvt::Error::Budget("test width"))?,
			(0..oracle_width).collect(),
			oracle_width,
			None,
		)?)
		.certified_standard(portable_certificate)
		.build()?;
	let schedule = MatchingTransform::from_certified_projector(encoding, certificate, policy)?;
	expect_true!(schedule.projector_certificate().is_some());
	let unitary = materialize_program(portable.main(), policy)?;
	for col in 0..unitary.ncols() {
		let mut state = vec![Complex64::new(0.0, 0.0); unitary.nrows()];
		*state
			.get_mut(col)
			.ok_or(quest_qsvt::Error::Encoding("test state"))? = Complex64::new(1.0, 0.0);
		schedule.apply_reference(&mut state, false, policy)?;
		for (row, &value) in state.iter().enumerate() {
			expect_true!(value.sub(unitary[(row, col)]).norm() < 1e-12);
		}
	}
	let output = polynomial.evaluate(1.0)?;
	expect_true!(output.mul(2.0).sub(1.0).abs() < 1e-12);
	Ok(())
}

#[gtest]
fn reciprocal_admits_large_byte_budget_and_reports_contractivity() -> googletest::Result<()> {
	use quest_qsvt::reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence};
	let bounds = SpectralBounds::new(
		0.5,
		1.0,
		SpectralEvidence::Analytic {
			description: "test singular interval".into(),
		},
	)?;
	let polynomial = ReciprocalPolynomial::geometric(
		&bounds,
		1.0,
		1e-3,
		255,
		NumericalPolicy {
			max_bytes: usize::MAX,
		},
	)?;
	expect_true!(polynomial.global_magnitude_bound() < 1.0);
	expect_true!(polynomial.global_magnitude_bound() >= 0.5);
	expect_true!(matches!(
		ReciprocalPolynomial::geometric(&bounds, 1.0, 0.0, 255, NumericalPolicy::default()),
		Err(quest_qsvt::Error::Encoding(_))
	));
	Ok(())
}

#[gtest]
fn truncated_positive_binomial_reciprocal_bounds_a_small_spectral_gap() -> googletest::Result<()> {
	use quest_qsvt::reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence};
	use std::ops::{Add, Div, Mul, Sub};
	let bounds = SpectralBounds::new(
		0.03,
		1.0,
		SpectralEvidence::CallerPremise {
			description: "independent test interval".into(),
		},
	)?;
	let polynomial =
		ReciprocalPolynomial::geometric(&bounds, 1.0, 1e-4, 1023, NumericalPolicy::default())?;
	expect_true!(polynomial.target().stored_order() <= 1023);
	expect_true!(polynomial.global_magnitude_bound() < 1.0);
	for sample in 0..=200_u32 {
		let x = 0.03_f64.add(f64::from(sample).div(200.0).mul(0.97));
		for sign in [-1.0, 1.0] {
			let signal = x.mul(sign);
			// The explicit slack only covers this binary64 reference evaluation;
			// the polynomial coefficient and analytic bounds are interval-derived.
			expect_true!(
				polynomial
					.evaluate(signal)?
					.sub(polynomial.scale().div(signal))
					.abs()
					<= polynomial.error_bound().add(1e-12)
			);
		}
	}
	Ok(())
}

#[gtest]
fn reciprocal_residual_budget_keeps_response_and_execution_errors_explicit()
-> googletest::Result<()> {
	use quest_qsvt::reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence};
	let bounds = SpectralBounds::new(
		1.0,
		1.0,
		SpectralEvidence::CallerPremise {
			description: "scalar spectrum premise".into(),
		},
	)?;
	let polynomial =
		ReciprocalPolynomial::geometric(&bounds, 1.0, 1e-3, 3, NumericalPolicy::default())?;
	let bound = polynomial.relative_residual_bound(0.01, 0.02)?;
	expect_true!(bound >= 0.06);
	expect_true!(polynomial.admit_relative_residual(0.01, 0.02, 0.1).is_ok());
	expect_true!(
		polynomial
			.admit_relative_residual(0.01, 0.02, 0.01)
			.is_err()
	);
	expect_true!(polynomial.relative_residual_bound(f64::NAN, 0.0).is_err());
	expect_true!(polynomial.relative_residual_bound(0.0, -0.1).is_err());
	Ok(())
}

#[gtest]
fn reciprocal_storage_admission_includes_retained_spectral_provenance() -> googletest::Result<()> {
	use quest_qsvt::reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence};
	let bounds = SpectralBounds::new(
		1.0,
		1.0,
		SpectralEvidence::CallerPremise {
			description: "x".repeat(4096),
		},
	)?;
	expect_true!(
		ReciprocalPolynomial::geometric(&bounds, 1.0, 0.01, 3, NumericalPolicy { max_bytes: 1024 })
			.is_err()
	);
	Ok(())
}

#[gtest]
fn exported_matching_schedule_owns_only_phase_payload_and_scalar_header() -> googletest::Result<()>
{
	use googletest::{expect_that, matchers::eq};
	let matrix = SparseMatrix::from_triplets(
		1024,
		1024,
		SparseFormat::Csr,
		vec![(999, 1, Complex64::new(1.0, 0.0))],
		SparseLimits::default(),
	)?;
	let encoding = MatchingEncoding::from_sparse(&matrix, NumericalPolicy::default())?;
	let identity = encoding.source_identity();
	let transform = MatchingTransform::new(
		encoding,
		PhaseSequence::<WxSymmetric>::builder(vec![0.17, -0.2, 0.17]).build()?,
		NumericalPolicy::default(),
	)?;
	let schedule = transform.schedule(NumericalPolicy { max_bytes: 512 })?;
	expect_true!(schedule.retained_bytes()? < 512);
	expect_that!(schedule.header().source_identity, eq(identity));
	for adjoint in [false, true] {
		let mut owned = Vec::new();
		transform.visit_steps(adjoint, |step| {
			owned.push(step);
			Ok(())
		})?;
		let mut exported = Vec::new();
		schedule.visit_steps(adjoint, |step| {
			exported.push(step);
			Ok::<(), quest_qsvt::Error>(())
		})?;
		expect_that!(exported, eq(&owned));
	}
	Ok(())
}
