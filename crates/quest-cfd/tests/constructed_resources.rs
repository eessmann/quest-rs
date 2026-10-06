#![allow(
	clippy::panic_in_result_fn,
	reason = "Bounded resource admission and independent circuit-count assertions"
)]
use quest_cfd::{
	CfdError,
	constructed_resources::{ConstructedResourceLimits, constructed_history_resources},
	history::HistorySystem,
};
use quest_numerics::{Complex64, SparseFormat, SparseLimits, SparseMatrix};
fn history(zero: bool) -> Result<HistorySystem, CfdError> {
	history_order(zero, 1)
}
fn history_order(zero: bool, order: usize) -> Result<HistorySystem, CfdError> {
	let generator =
		SparseMatrix::from_triplets(2, 2, SparseFormat::Csr, vec![], SparseLimits::default())?;
	HistorySystem::assemble(
		&generator,
		&[
			Complex64::new(if zero { 0. } else { 1. }, 0.),
			Complex64::new(0., 0.),
		],
		0.1,
		1,
		order,
		SparseLimits::default(),
	)
}
#[test]
fn temporal_dg2_constructs_three_nodes_and_padded_rhs() -> Result<(), CfdError> {
	let h = history_order(false, 2)?;
	let r = constructed_history_resources(&h, ConstructedResourceLimits::default(), None, None)?;
	assert_eq!(r.history_dimension, 6);
	assert_eq!(r.history_nonzeros, 16);
	let e = r.encoding.ok_or(CfdError::Assembly("DG2 source missing"))?;
	assert_eq!(e.descriptor.rows, 6);
	assert_eq!(e.descriptor.data_qubits, 3);
	assert!(e.count.forward.gates > 22);
	let rhs = r
		.rhs_preparation
		.ok_or(CfdError::Assembly("DG2 RHS missing"))?;
	assert_eq!(rhs.padded_dimension, 8);
	assert_eq!(rhs.qubits, 3);
	Ok(())
}
#[test]
fn actual_temporal_matching_has_three_auxiliaries_including_signal() -> Result<(), CfdError> {
	let h = history(false)?;
	let r = constructed_history_resources(&h, ConstructedResourceLimits::default(), None, None)?;
	assert_eq!(r.status, "constructed-resources-counted");
	let e = r
		.encoding
		.ok_or(CfdError::Assembly("missing encoding costs"))?;
	// DG1 zero dynamics: two diagonal and two off-diagonal temporal blocks,
	// two colors, two color H gates, two defaults,16 edge corrections,2 transpositions.
	assert_eq!(e.descriptor.data_qubits, 2);
	assert_eq!(e.descriptor.encoding_qubits, 4);
	assert_eq!(e.descriptor.inverse_qubits, 5);
	assert_eq!(e.descriptor.auxiliary_qubits_including_signal, 3);
	assert_eq!(e.count.forward.gates, 22);
	assert_eq!(e.count.adjoint.gates, 22);
	assert!(e.descriptor.encoding_error_bound.is_none());
	assert!(r.rhs_preparation.is_some());
	assert!(!r.quantum_execution);
	Ok(())
}
#[test]
fn zero_rhs_skips_quantum_construction_with_zero_work_budgets() -> Result<(), CfdError> {
	let h = history(true)?;
	let r = constructed_history_resources(
		&h,
		ConstructedResourceLimits {
			max_bytes: h.retained_bytes()?,
			max_matching_work: 0,
			max_count_work: 0,
			..ConstructedResourceLimits::default()
		},
		None,
		None,
	)?;
	assert_eq!(r.status, "zero-rhs-no-circuit");
	assert!(r.encoding.is_none() && r.rhs_preparation.is_none() && r.inverse.is_none());
	Ok(())
}
#[test]
fn shared_count_rejection_retains_source_and_does_not_claim_complete_pair() -> Result<(), CfdError>
{
	let r = constructed_history_resources(
		&history(false)?,
		ConstructedResourceLimits {
			max_count_work: 22,
			..ConstructedResourceLimits::default()
		},
		None,
		None,
	)?;
	assert_eq!(r.status, "constructed-resources-partial");
	let e = r
		.encoding
		.ok_or(CfdError::Assembly("missing compiled source"))?;
	assert_eq!(e.count.forward.gates, 22);
	assert_eq!(e.count.adjoint.gates, 0);
	assert!(e.count.rejection.is_some());
	assert_eq!(r.count_work, 22);
	Ok(())
}
#[test]
fn source_compile_and_live_overlap_limits_reject_before_counting() -> Result<(), CfdError> {
	let h = history(false)?;
	assert!(
		constructed_history_resources(
			&h,
			ConstructedResourceLimits {
				max_matching_work: 1,
				..ConstructedResourceLimits::default()
			},
			None,
			None
		)
		.is_err()
	);
	assert!(
		constructed_history_resources(
			&h,
			ConstructedResourceLimits {
				max_bytes: h.retained_bytes()?,
				..ConstructedResourceLimits::default()
			},
			None,
			None
		)
		.is_err()
	);
	Ok(())
}
#[test]
fn actual_inverse_schedule_and_real_diagonal_recipe_charge_repeated_attempts()
-> Result<(), CfdError> {
	use quest_cfd::{
		constructed_resources::{InverseResourceRequest, ObservationResourceRequest},
		probability_observation::{DiagonalRange, SamplingRequest},
	};
	let h = history(false)?;
	let recipe = |index: usize| Ok((index < 2, if index == 0 { 1. } else { 0. }));
	let observation = ObservationResourceRequest {
		sampling: SamplingRequest {
			range: DiagonalRange::new(0., 1.)?,
			absolute_error: 0.5,
			failure_probability: 0.1,
			joint_success_lower_bound: 0.01,
			success_bound_provenance: "external joint inverse/sector probability hypothesis; not simulator evidence",
			max_selected_shots: 1000,
			max_attempted_shots: 100_000,
			max_provenance_bytes: 1024,
			systematic_bias_bound: None,
		},
		description: "population of first logical basis coordinate, selected in first temporal block",
		retained_bytes: 0,
		scratch_bytes: 0,
		selection_query_work: 2,
		observable_query_work: 1,
		max_validation_work: 2000,
		query: &recipe,
	};
	let r = constructed_history_resources(
		&h,
		ConstructedResourceLimits::default(),
		Some(InverseResourceRequest {
			approximation_tolerance: 0.1,
			max_degree: 255,
			..InverseResourceRequest::default()
		}),
		Some(observation),
	)?;
	assert_eq!(r.status, "constructed-resources-counted", "{r:?}");
	let inv = r
		.inverse
		.ok_or(CfdError::Assembly("missing inverse cost"))?;
	assert_eq!(inv.oracle_calls_forward, 2 * inv.degree);
	assert_eq!(inv.oracle_calls_adjoint, 2 * inv.degree);
	assert_eq!(inv.count.forward.gates, inv.count.adjoint.gates);
	assert!(
		inv.projector_response_bound.is_none() && inv.execution_amplitude_error_bound.is_none()
	);
	let sampling = r
		.sampling
		.ok_or(CfdError::Assembly("missing sampling cost"))?;
	assert_eq!(sampling.validation_queries, 8);
	assert_eq!(sampling.validation_work, 8 * (2 + 1 + 128));
	assert_eq!(
		sampling.attempted_inverse_primitives,
		sampling.attempted_shots
			* u64::try_from(inv.count.forward.gates)
				.map_err(|_| CfdError::InvalidInput("test count"))?
	);
	assert_eq!(
		sampling.classical_attempt_selection_work,
		sampling.attempted_shots * 2
	);
	assert_eq!(
		sampling.classical_selected_observable_work,
		sampling.selected_shots
	);
	assert!(sampling.total_error_bound.is_none());
	assert!(!sampling.quantum_measurements_executed);
	Ok(())
}
#[test]
fn unavailable_inverse_plan_does_not_erase_constructed_encoding() -> Result<(), CfdError> {
	use quest_cfd::constructed_resources::InverseResourceRequest;
	let r = constructed_history_resources(
		&history(false)?,
		ConstructedResourceLimits::default(),
		Some(InverseResourceRequest {
			max_degree: 1,
			..InverseResourceRequest::default()
		}),
		None,
	)?;
	assert_eq!(r.status, "constructed-resources-partial");
	assert!(r.inverse.is_none() && r.inverse_rejection.is_some());
	assert!(r.encoding.is_some() && r.rhs_preparation.is_some());
	Ok(())
}
#[test]
fn rejected_reciprocal_retains_the_attempted_stage_memory_envelope() -> Result<(), CfdError> {
	use quest_cfd::constructed_resources::InverseResourceRequest;
	let generator =
		SparseMatrix::from_triplets(1, 1, SparseFormat::Csr, vec![], SparseLimits::default())?;
	let h = HistorySystem::assemble(
		&generator,
		&[Complex64::new(1., 0.)],
		0.1,
		4,
		1,
		SparseLimits::default(),
	)?;
	let limits = ConstructedResourceLimits::default();
	let r = constructed_history_resources(
		&h,
		limits,
		Some(InverseResourceRequest {
			max_degree: 1,
			..InverseResourceRequest::default()
		}),
		None,
	)?;
	assert_eq!(r.status, "constructed-resources-partial");
	assert!(r.encoding.is_some() && r.rhs_preparation.is_some() && r.inverse.is_none());
	assert!(
		r.inverse_rejection
			.as_deref()
			.is_some_and(|s| s.contains("contractivity"))
	);
	// Independent allocator review measured a 2,070,672-byte temporary in
	// this failed attempt, despite the old receipt claiming only 11,752 bytes.
	// Without peak telemetry the complete admitted stage envelope is retained.
	assert_eq!(r.managed_peak_byte_envelope, limits.max_bytes);
	assert!(r.timings.synthesis_seconds > 0.);
	Ok(())
}
#[test]
fn inverse_descriptor_binds_actual_nonhermitian_adjoint() -> Result<(), CfdError> {
	use quest_qsvt::{MatchingEncoding, NumericalPolicy, ReplayEncoding};
	let h = history(false)?;
	let direct =
		MatchingEncoding::from_sparse(h.operator(), NumericalPolicy::default())?.descriptor()?;
	let source = MatchingEncoding::from_sparse(
		&h.operator().adjoint(SparseLimits::default())?,
		NumericalPolicy::default(),
	)?;
	let adjoint = source.descriptor()?;

	assert_ne!(direct.source_identity, adjoint.source_identity);
	assert_ne!(direct.construction_identity, adjoint.construction_identity);
	assert_ne!(direct, adjoint);
	let left = adjoint
		.left
		.logical_space::<quest_qsvt::Left>(source.num_qubits(), NumericalPolicy::default())?;
	let right = adjoint
		.right
		.logical_space::<quest_qsvt::Right>(source.num_qubits(), NumericalPolicy::default())?;
	// Only this N=4 fixture allocates a 16-amplitude whole-U reference. Extract
	// the block independently from original H entries, rather than an adjoint CSR.
	assert_eq!(h.operator().rows(), 4);
	assert_eq!(source.num_qubits(), 4);
	for col in 0..4 {
		let mut state = vec![Complex64::new(0., 0.); 16];
		let coordinate = right
			.coordinate_at(col)
			.ok_or(CfdError::Assembly("right logical coordinate"))?;
		*state
			.get_mut(coordinate)
			.ok_or(CfdError::Assembly("right state coordinate"))? = Complex64::new(1., 0.);
		source.apply_reference(&mut state, false, NumericalPolicy::default())?;
		for row in 0..4 {
			let expected = h
				.operator()
				.entries()
				.find(|(r, c, _)| *r == col && *c == row)
				.map_or(Complex64::new(0., 0.), |(_, _, value)| value.conj())
				/ source.normalization().get();
			let actual = state
				.get(
					left.coordinate_at(row)
						.ok_or(CfdError::Assembly("left logical coordinate"))?,
				)
				.ok_or(CfdError::Assembly("left state coordinate"))?;
			assert!((*actual - expected).norm() < 1e-12);
		}
	}
	let r = constructed_history_resources(&h, ConstructedResourceLimits::default(), None, None)?;
	assert_eq!(
		r.encoding
			.ok_or(CfdError::Assembly("descriptor absent"))?
			.descriptor
			.source_identity,
		adjoint.source_identity
	);
	Ok(())
}
#[test]
fn sampling_admission_precedes_callbacks_and_repeat_digest_is_checked() -> Result<(), CfdError> {
	use quest_cfd::{
		constructed_resources::{InverseResourceRequest, ObservationResourceRequest},
		probability_observation::{DiagonalRange, SamplingRequest},
	};
	let h = history(false)?;
	let calls = std::cell::Cell::new(0usize);
	let recipe = |_index: usize| {
		let prior = calls.get();
		calls.set(prior + 1);
		Ok((true, if prior < 4 { 0. } else { 1. }))
	};
	let sampling = SamplingRequest {
		range: DiagonalRange::new(0., 1.)?,
		absolute_error: 0.5,
		failure_probability: 0.1,
		joint_success_lower_bound: 0.1,
		success_bound_provenance: "external probability hypothesis",
		max_selected_shots: 1000,
		max_attempted_shots: 100_000,
		max_provenance_bytes: 1024,
		systematic_bias_bound: None,
	};
	let request = |limit| ObservationResourceRequest {
		sampling,
		description: "bounded changing recipe regression",
		retained_bytes: 0,
		scratch_bytes: 0,
		selection_query_work: 1,
		observable_query_work: 1,
		max_validation_work: limit,
		query: &recipe,
	};
	let inverse = Some(InverseResourceRequest {
		approximation_tolerance: 0.1,
		max_degree: 255,
		..InverseResourceRequest::default()
	});
	let r = constructed_history_resources(
		&h,
		ConstructedResourceLimits::default(),
		inverse,
		Some(request(0)),
	)?;
	assert_eq!(calls.get(), 0);
	assert!(r.sampling.is_none() && r.sampling_rejection.is_some());
	let r = constructed_history_resources(
		&h,
		ConstructedResourceLimits::default(),
		inverse,
		Some(request(2000)),
	)?;
	assert_eq!(calls.get(), 8);
	assert!(r.sampling.is_none());
	assert!(
		r.sampling_rejection
			.as_deref()
			.is_some_and(|s| s.contains("changed"))
	);
	Ok(())
}

#[test]
fn zero_rhs_still_charges_the_already_live_history() -> Result<(), CfdError> {
	let h = history(true)?;
	assert!(
		constructed_history_resources(
			&h,
			ConstructedResourceLimits {
				max_bytes: h
					.retained_bytes()?
					.checked_sub(1)
					.ok_or(CfdError::InvalidInput("test bytes"))?,
				..ConstructedResourceLimits::default()
			},
			None,
			None
		)
		.is_err()
	);
	Ok(())
}

#[test]
fn callback_retained_and_scratch_storage_are_admitted_before_optional_inverse()
-> Result<(), CfdError> {
	use quest_cfd::{
		constructed_resources::ObservationResourceRequest,
		probability_observation::{DiagonalRange, SamplingRequest},
	};
	let h = history(false)?;
	let calls = std::cell::Cell::new(0usize);
	let callback = |_index: usize| {
		calls.set(calls.get() + 1);
		Ok((true, 0.))
	};
	let sampling = SamplingRequest {
		range: DiagonalRange::new(0., 1.)?,
		absolute_error: 0.1,
		failure_probability: 0.1,
		joint_success_lower_bound: 0.1,
		success_bound_provenance: "declared conditional premise",
		max_selected_shots: 1000,
		max_attempted_shots: 100_000,
		max_provenance_bytes: 1024,
		systematic_bias_bound: None,
	};
	let mut rejections = Vec::new();
	for (retained_bytes, scratch_bytes) in [(1_048_576, 0), (0, 1_048_576)] {
		let observation = ObservationResourceRequest {
			sampling,
			description: "bounded callback storage regression",
			retained_bytes,
			scratch_bytes,
			selection_query_work: 1,
			observable_query_work: 1,
			max_validation_work: 1000,
			query: &callback,
		};
		rejections.push(
			constructed_history_resources(
				&h,
				ConstructedResourceLimits {
					max_bytes: 1_048_576,
					..ConstructedResourceLimits::default()
				},
				None,
				Some(observation),
			)
			.is_err(),
		);
	}
	assert_eq!(
		rejections,
		vec![true, true],
		"retained then scratch admission"
	);
	assert_eq!(calls.get(), 0);
	Ok(())
}
