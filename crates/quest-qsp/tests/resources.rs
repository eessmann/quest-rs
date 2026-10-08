#![allow(
	clippy::panic_in_result_fn,
	reason = "Regression tests assert independently observable resource and numerical behavior"
)]
use quest_numerics::ExecutionPolicy;
use quest_numerics::{OperationLimits, OperationResources};
use quest_qsp::{Complex64, FftBackend, ForwardNlftWorkspace, InverseNlftWorkspace};
use std::ops::Sub;

#[test]
fn retained_plans_reuse_allocations_and_results_hold_their_ownership()
-> Result<(), Box<dyn std::error::Error>> {
	let resources = OperationResources::default();
	let mut forward = ForwardNlftWorkspace::new(
		FftBackend::Scalar,
		resources.clone(),
		ExecutionPolicy::Sequential,
	);
	let gamma = [
		Complex64::new(0.1, 0.0),
		Complex64::new(-0.2, 0.0),
		Complex64::new(0.05, 0.0),
	];
	let first = forward.forward(&gamma)?;
	let initial = resources.report();
	assert!(initial.live_planner_bytes_estimate > 0);
	let second = forward.forward(&gamma)?;
	assert_eq!(
		resources
			.report()
			.live_bytes
			.checked_sub(initial.live_bytes)
			.ok_or("test overflow")?,
		96
	);
	assert_eq!(
		resources.report().live_planner_bytes_estimate,
		initial.live_planner_bytes_estimate
	);
	assert_eq!(first.target, second.target);
	let mut inverse = InverseNlftWorkspace::new(
		FftBackend::Scalar,
		resources.clone(),
		ExecutionPolicy::Sequential,
	);
	let reconstructed = inverse.inverse(&first.conjugate_complement, &first.target)?;
	for (a, b) in reconstructed.iter().zip(gamma) {
		assert!(a.sub(b).norm() < 1e-12);
	}
	drop((forward, inverse));
	assert_eq!(resources.report().live_planner_bytes_estimate, 0);
	assert_eq!(resources.report().live_bytes, 240);
	drop((first, second, reconstructed));
	assert_eq!(resources.report().live_bytes, 0);
	Ok(())
}

#[test]
fn separate_forward_inverse_stages_share_a_cumulative_limit()
-> Result<(), Box<dyn std::error::Error>> {
	let resources = OperationResources::default();
	let gamma = [Complex64::new(0.1, 0.0), Complex64::new(0.2, 0.0)];
	let mut forward = ForwardNlftWorkspace::new(
		FftBackend::Scalar,
		resources.clone(),
		ExecutionPolicy::Sequential,
	);
	let pair = forward.forward(&gamma)?;
	let work = resources.report().work_units;
	let mut limits = OperationLimits::default();
	limits.resources.max_work_units = work;
	let limited = OperationResources::from_limits(limits);
	let mut forward = ForwardNlftWorkspace::new(
		FftBackend::Scalar,
		limited.clone(),
		ExecutionPolicy::Sequential,
	);
	let produced = forward.forward(&gamma)?;
	let mut inverse = InverseNlftWorkspace::new(
		FftBackend::Scalar,
		limited.clone(),
		ExecutionPolicy::Sequential,
	);
	assert!(
		inverse
			.inverse(&produced.conjugate_complement, &produced.target)
			.is_err()
	);
	assert_eq!(limited.report().work_units, work);
	assert_eq!(pair.target, produced.target);
	Ok(())
}

#[test]
fn production_pipeline_keeps_one_ledger_through_completion_and_synthesis()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Laurent, Limits, Polynomial};
	use quest_qsp::SynthesisBuilder;
	let polynomial = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	let admitted = SynthesisBuilder::new()
		.unit_circle_response(&polynomial)?
		.admit()?;
	let before = admitted.resource_report();
	let completed = admitted.complete()?;
	let after_completion = completed.resource_report();
	assert!(after_completion.work_units > before.work_units);
	let candidate = completed.synthesize()?;
	let after_synthesis = candidate.resource_report();
	assert!(after_synthesis.work_units > after_completion.work_units);
	assert_eq!(after_synthesis.live_planner_bytes_estimate, 0);
	assert!(after_synthesis.live_bytes > 0);
	Ok(())
}

#[test]
fn physical_inverse_charges_preparation_and_recovers_the_same_reflections()
-> Result<(), Box<dyn std::error::Error>> {
	let resources = OperationResources::default();
	let gamma = [
		Complex64::new(0.1, 0.02),
		Complex64::new(-0.2, -0.01),
		Complex64::new(0.03, -0.02),
	];
	let mut forward = ForwardNlftWorkspace::new(
		FftBackend::Scalar,
		resources.clone(),
		ExecutionPolicy::Sequential,
	);
	let pair = forward.forward(&gamma)?;
	let physical: Vec<_> = pair
		.conjugate_complement
		.iter()
		.rev()
		.map(Complex64::conj)
		.collect();
	let mut inverse =
		InverseNlftWorkspace::new(FftBackend::Scalar, resources, ExecutionPolicy::Sequential);
	let recovered = inverse.inverse_physical(&physical, &pair.target)?;
	for (a, b) in recovered.iter().zip(gamma) {
		assert!(a.sub(b).norm() < 1e-12);
	}
	Ok(())
}

#[test]
fn escaped_control_sequence_retains_only_its_shared_control_allocation()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Laurent, Limits, Polynomial};
	use quest_qsp::SynthesisBuilder;
	let polynomial = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	let admitted = SynthesisBuilder::new()
		.unit_circle_response(&polynomial)?
		.admit()?;
	let observer = admitted.clone();
	let baseline = observer.resource_report().live_bytes;
	let candidate = admitted.complete()?.synthesize()?;
	let sequence = candidate.control_sequence();
	drop(candidate);
	assert_eq!(
		observer.resource_report().live_bytes,
		baseline.checked_add(128).ok_or("test overflow")?
	);
	drop(sequence);
	assert_eq!(observer.resource_report().live_bytes, baseline);
	Ok(())
}

#[cfg(feature = "offline-synthesis")]
#[test]
fn offline_export_reports_computation_work_and_live_export_buffers()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Laurent, Limits, Polynomial};
	use quest_qsp::offline::OfflineBuilder;
	let polynomial = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	let solved = OfflineBuilder::new()
		.unit_circle_response(&polynomial)?
		.policy(quest_qsp::offline::OfflinePolicy::default())?
		.solve()?;
	let report = solved.certified().candidate().resource_report();
	assert_eq!(report.live_bytes, 224);
	assert!(report.peak_bytes > report.live_bytes);
	assert_eq!(
		report.work_units,
		solved
			.report()
			.attempts()
			.iter()
			.map(quest_qsp::offline::OfflineAttempt::work_units)
			.sum::<usize>()
	);
	Ok(())
}

#[cfg(feature = "artifact")]
#[test]
fn loaded_export_retains_buffers_and_independent_fft_policy()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Laurent, Limits, Polynomial};
	use quest_qsp::artifact::{
		ArtifactLimits, LoadPolicy, LoadedCompiled, export_compiled, load_compiled,
	};
	use quest_qsp::{Policy, SynthesisBuilder};
	let polynomial = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	let mut policy = Policy::default();
	policy.limits.shapes.max_coefficients = 5;
	policy.limits.shapes.max_fft_len = 8;
	let candidate = SynthesisBuilder::new()
		.policy(policy)
		.unit_circle_response(&polynomial)?
		.admit()?
		.complete()?
		.synthesize()?;
	let encoded = export_compiled(&candidate, ArtifactLimits::default())?;
	let LoadedCompiled::UnitCircleResponse(loaded) =
		load_compiled(&encoded, LoadPolicy::default())?
	else {
		return Err("unexpected mode".into());
	};
	assert!(loaded.resource_report().live_bytes >= 224);
	assert!(loaded.resource_report().work_units > 0);
	let roundtrip: serde_json::Value =
		serde_json::from_slice(&export_compiled(&loaded, ArtifactLimits::default())?)?;
	assert_eq!(roundtrip["payload"]["policy"]["max_fft_len"], 8);
	Ok(())
}

#[test]
fn completion_sample_buffers_use_the_completion_gate() -> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Laurent, Limits, Polynomial};
	use quest_qsp::{Policy, SynthesisBuilder};
	let polynomial = Polynomial::new(
		Laurent::new(0),
		vec![Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.0)],
		Limits::default(),
	)?;
	let mut policy = Policy::default();
	policy.limits.shapes.max_coefficients = 2;
	policy.limits.shapes.max_fft_len = 4;
	policy.limits.shapes.max_completion_grid = 32;
	let completed = SynthesisBuilder::new()
		.policy(policy)
		.unit_circle_response(&polynomial)?
		.admit()?
		.complete()?;
	assert_eq!(completed.completion_grid(), 32);
	Ok(())
}

#[test]
fn synthesis_builder_reuses_the_source_ledger_and_its_limits()
-> Result<(), Box<dyn std::error::Error>> {
	use quest_polynomial::{Laurent, Polynomial};
	use quest_qsp::SynthesisBuilder;
	let resources = OperationResources::default();
	let polynomial = Polynomial::new_with_resources(
		Laurent::new(0),
		vec![Complex64::new(0.2, 0.0), Complex64::new(0.1, 0.0)],
		resources.clone(),
	)?;
	let before = resources.report();
	let admitted = SynthesisBuilder::new()
		.resources(resources.clone())
		.unit_circle_response(&polynomial)?
		.admit()?;
	assert!(resources.report().work_units > before.work_units);
	assert_eq!(resources.report(), admitted.resource_report());
	assert_eq!(resources.report().live_bytes, 96);
	Ok(())
}
