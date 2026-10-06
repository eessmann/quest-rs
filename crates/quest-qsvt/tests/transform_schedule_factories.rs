#![allow(
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Small frozen phase fixtures assert complete copied payloads"
)]
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::replay_transform::TransformSchedule;
use quest_qsvt::{NumericalPolicy, ReplayEncoding, StandardConvention, TensorShiftEncoding};
#[test]
fn descriptor_factory_retains_converted_payload_and_rejects_capacity() -> quest_qsvt::Result<()> {
	let p = NumericalPolicy::default();
	let descriptor = TensorShiftEncoding::new(1, vec![], p)?.descriptor()?;
	let values = vec![0.1, 0.2, 0.1];
	let sequence = PhaseSequence::<WxSymmetric>::builder(values.clone()).build()?;
	let converted = WxSymmetric::projector_phases(&sequence);
	let schedule = TransformSchedule::from_phase_sequence(descriptor.clone(), sequence, p)?;
	assert_eq!(schedule.values(), converted.values());
	assert_eq!(schedule.descriptor(), &descriptor);
	assert_eq!(
		schedule.conversion_roundoff_estimate(),
		converted.roundoff_estimate()
	);
	assert!(schedule.projector_response_bound().is_none());
	let mut large = Vec::with_capacity(16384);
	large.extend(values);
	assert!(
		TransformSchedule::from_phase_sequence(
			descriptor,
			PhaseSequence::<WxSymmetric>::builder(large).build()?,
			NumericalPolicy { max_bytes: 4096 }
		)
		.is_err()
	);
	Ok(())
}

#[cfg(feature = "certification")]
#[test]
fn descriptor_factory_uses_exact_certified_angles_and_attestation()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
	use quest_qsvt::reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence};
	let p = NumericalPolicy::default();
	let descriptor = TensorShiftEncoding::new(1, vec![], p)?.descriptor()?;
	let spectrum = SpectralBounds::new(
		1.,
		1.,
		SpectralEvidence::Analytic {
			description: "identity spectrum".into(),
		},
	)?;
	let polynomial = ReciprocalPolynomial::geometric(&spectrum, 1., 1e-3, 3, p)?;
	let candidate = polynomial.synthesize(quest_qsp::Policy::default())?;
	let certificate = CertificationBuilder::new()
		.candidate(candidate)
		.policy(CertificationPolicy::default())?
		.certify()?
		.certify_projector_phases(CertificationPolicy::default())?;
	let schedule =
		TransformSchedule::from_certified_projector(descriptor.clone(), &certificate, p)?;
	assert_eq!(schedule.values(), certificate.values());
	assert_eq!(schedule.readout_phase(), certificate.readout_phase());
	assert_eq!(
		schedule.projector_response_bound(),
		Some(certificate.response_bound().upper_f64())
	);
	assert!(
		TransformSchedule::from_certified_projector(
			descriptor,
			&certificate,
			NumericalPolicy { max_bytes: 1 }
		)
		.is_err()
	);
	Ok(())
}
