//! Bounded costs of actually constructed H-adjoint encodings and coherent RHS sources.
//!
//! No state, dense U or expanded gate stream is allocated. Portable controlled
//! primitives are not a Clifford+T decomposition or native dispatch-work count.
use crate::{
	CfdError,
	history::HistorySystem,
	probability_observation::{SamplingRequest, plan_sampling},
};
use quest_numerics::SparseLimits;
use quest_qsvt::{
	EncodingDescriptor, MatchingEncoding, NumericalPolicy, ReplayEncoding, ReplayGate, ReplayKind,
	reciprocal::ReciprocalPolynomial,
	replay_transform::{ReplayTransform, TransformStep},
	state_preparation::{AmplitudePreparation, PreparationLimits},
};
use serde::Serialize;
use std::time::Instant;

/// Whole live numerical-payload limits. Output receipts/allocator overhead are excluded.
#[derive(Clone, Copy, Debug)]
pub struct ConstructedResourceLimits {
	pub max_bytes: usize,
	/// Other caller-owned numerical payload live with the borrowed history.
	pub external_retained_bytes: usize,
	/// Checked conservative scan/primitive model, not measured CPU instructions.
	pub max_matching_work: u64,
	/// One visitor/step allowance shared across all requested forward/adjoint counts.
	pub max_count_work: usize,
	pub max_gates_per_orientation: usize,
	pub max_preparation_compile_work: usize,
	pub max_preparation_gates: usize,
}
impl Default for ConstructedResourceLimits {
	fn default() -> Self {
		Self {
			max_bytes: 268_435_456,
			external_retained_bytes: 0,
			max_matching_work: 1_000_000_000_000,
			max_count_work: 20_000_000,
			max_gates_per_orientation: 10_000_000,
			max_preparation_compile_work: 67_108_864,
			max_preparation_gates: 8_388_608,
		}
	}
}
/// Optional actual polynomial and binary64 phase synthesis; never executes an inverse.
#[derive(Clone, Copy, Debug)]
pub struct InverseResourceRequest {
	pub approximation_tolerance: f64,
	pub max_degree: usize,
	pub max_synthesis_work: usize,
	pub max_completion_grid: usize,
}
impl Default for InverseResourceRequest {
	fn default() -> Self {
		Self {
			approximation_tolerance: 0.03,
			max_degree: 2047,
			max_synthesis_work: 8_589_934_592,
			max_completion_grid: 1_048_576,
		}
	}
}
/// Actual bounded classical selection/diagonal recipe plus conditional sampling premises.
/// Work declarations cover the supplied callback; future measurement premises remain caller evidence.
pub struct ObservationResourceRequest<'a> {
	pub sampling: SamplingRequest<'a>,
	pub description: &'a str,
	/// Callback-owned/borrowed numerical payload, in addition to `limits.external_retained_bytes`.
	pub retained_bytes: usize,
	/// Maximum temporary numerical payload for one callback invocation.
	pub scratch_bytes: usize,
	pub selection_query_work: u64,
	pub observable_query_work: u64,
	pub max_validation_work: u64,
	pub query: &'a dyn Fn(usize) -> Result<(bool, f64), CfdError>,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct GateInventory {
	pub gates: usize,
	pub x: usize,
	pub h: usize,
	pub ry: usize,
	pub phase: usize,
	pub controlled_gates: usize,
	pub control_occurrences: usize,
	pub maximum_controls: u32,
}
#[derive(Debug, Default, Serialize)]
pub struct ReplayCount {
	pub forward: GateInventory,
	pub adjoint: GateInventory,
	pub accepted_work: usize,
	pub observed_emissions: usize,
	pub rejection: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct ProjectorCost {
	pub fixed_mask: usize,
	pub fixed_value: usize,
	pub logical_start: usize,
	pub logical_end: usize,
}
#[derive(Debug, Serialize)]
pub struct DescriptorCost {
	pub rows: usize,
	pub cols: usize,
	pub data_qubits: u32,
	pub encoding_qubits: usize,
	pub inverse_qubits: usize,
	pub auxiliary_qubits_including_signal: usize,
	pub normalization: f64,
	pub system_mask: usize,
	pub workspace_mask: usize,
	pub clean_workspace_mask: usize,
	pub clean_workspace_value: usize,
	pub left: ProjectorCost,
	pub right: ProjectorCost,
	pub source_identity: u64,
	pub construction_identity: u64,
	pub preparation_error_bound: Option<f64>,
	pub encoding_error_bound: Option<f64>,
	pub binary64_parameters: bool,
}
#[derive(Debug, Serialize)]
pub struct EncodingCost {
	pub descriptor: DescriptorCost,
	pub retained_bytes: usize,
	pub construction_peak_bytes: usize,
	pub constructor_count_pass_gates: usize,
	pub matching_scan_work_model_ceiling: u64,
	pub count: ReplayCount,
}
#[derive(Debug, Serialize)]
pub struct PreparationCost {
	pub qubits: usize,
	pub padded_dimension: usize,
	pub coefficients: usize,
	pub constructor_declared_gates: usize,
	pub compile_work: usize,
	pub retained_bytes: usize,
	pub construction_peak_bytes: usize,
	pub norm: f64,
	pub error_bound: Option<f64>,
	pub count: ReplayCount,
}
#[derive(Debug, Serialize)]
pub struct InverseCost {
	pub spectral_lower: f64,
	pub spectral_upper: f64,
	pub spectral_evidence: String,
	pub degree: usize,
	pub reciprocal_scale: f64,
	pub physical_rescaling: f64,
	pub polynomial_error_bound: f64,
	pub coefficient_rounding_bound: f64,
	pub projector_response_bound: Option<f64>,
	pub execution_amplitude_error_bound: Option<f64>,
	pub binary64_completion_diagnostic: f64,
	pub binary64_reconstruction_diagnostic: Option<f64>,
	pub conversion_roundoff_estimate: f64,
	pub phases: usize,
	pub retained_bytes: usize,
	pub qubits: usize,
	pub oracle_calls_forward: usize,
	pub oracle_calls_adjoint: usize,
	pub count: ReplayCount,
	pub synthesis_work_limit: usize,
	pub synthesis_byte_envelope: usize,
}
#[derive(Debug, Serialize)]
pub struct SamplingCost {
	pub selected_shots: u64,
	pub attempted_shots: u64,
	pub caller_joint_success_lower_bound: f64,
	pub success_bound_provenance: String,
	pub observable_description: String,
	pub range_lower: f64,
	pub range_upper: f64,
	pub statistical_absolute_error: f64,
	pub total_error_bound: Option<f64>,
	pub systematic_bias_bound: Option<f64>,
	pub failure_probability: f64,
	pub attempted_preparation_primitives: u64,
	pub attempted_inverse_primitives: u64,
	pub attempted_inverse_oracle_calls: u64,
	pub measured_register_bits: u64,
	pub classical_attempt_selection_work: u64,
	pub classical_selected_observable_work: u64,
	pub validation_queries: usize,
	pub validation_work: u64,
	pub source_digest: u64,
	pub quantum_measurements_executed: bool,
}
#[derive(Debug, Default, Serialize)]
pub struct ResourceTimings {
	pub source_construction_seconds: f64,
	pub rhs_compilation_seconds: f64,
	pub synthesis_seconds: f64,
	pub count_seconds: f64,
	pub sampling_validation_seconds: f64,
}
#[derive(Debug, Serialize)]
pub struct ConstructedResourceReport {
	pub status: &'static str,
	pub quantum_execution: bool,
	pub history_dimension: usize,
	pub history_nonzeros: usize,
	pub history_retained_bytes: usize,
	pub caller_external_retained_bytes: usize,
	pub observation_retained_bytes: usize,
	pub observation_scratch_bytes: usize,
	pub managed_peak_byte_envelope: usize,
	pub rhs_norm: f64,
	pub count_work: usize,
	pub encoding: Option<EncodingCost>,
	pub rhs_preparation: Option<PreparationCost>,
	pub inverse: Option<InverseCost>,
	pub inverse_rejection: Option<String>,
	pub sampling: Option<SamplingCost>,
	pub sampling_rejection: Option<String>,
	pub timings: ResourceTimings,
	pub memory_scope: &'static str,
	pub primitive_cost_scope: &'static str,
}
fn add(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b).ok_or(CfdError::InvalidInput(
		"constructed resource count overflow",
	))
}
fn mul(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b).ok_or(CfdError::InvalidInput(
		"constructed resource count overflow",
	))
}
fn available(l: ConstructedResourceLimits, live: usize) -> Result<usize, CfdError> {
	l.max_bytes.checked_sub(live).ok_or(CfdError::InvalidInput(
		"constructed resource whole-live byte budget",
	))
}
fn charge_peak(
	r: &mut ConstructedResourceReport,
	l: ConstructedResourceLimits,
	bytes: usize,
) -> Result<(), CfdError> {
	available(l, bytes)?;
	r.managed_peak_byte_envelope = r.managed_peak_byte_envelope.max(bytes);
	Ok(())
}
fn descriptor(d: EncodingDescriptor) -> Result<DescriptorCost, CfdError> {
	let projector = |p: quest_qsvt::CompactProjector| ProjectorCost {
		fixed_mask: p.fixed_mask,
		fixed_value: p.fixed_value,
		logical_start: p.logical_range.start,
		logical_end: p.logical_range.end,
	};
	Ok(DescriptorCost {
		rows: d.rows,
		cols: d.cols,
		data_qubits: d.layout.system_mask.count_ones(),
		encoding_qubits: d.layout.num_qubits,
		inverse_qubits: add(d.layout.num_qubits, 1)?,
		auxiliary_qubits_including_signal: add(
			usize::try_from(d.layout.workspace_mask.count_ones())
				.map_err(|_| CfdError::InvalidInput("workspace bit count"))?,
			1,
		)?,
		normalization: d.normalization,
		system_mask: d.layout.system_mask,
		workspace_mask: d.layout.workspace_mask,
		clean_workspace_mask: d.layout.clean_workspace_mask,
		clean_workspace_value: d.layout.clean_workspace_value,
		left: projector(d.left),
		right: projector(d.right),
		source_identity: d.source_identity,
		construction_identity: d.construction_identity,
		preparation_error_bound: d.errors.preparation,
		encoding_error_bound: d.errors.encoding,
		binary64_parameters: d.errors.binary64_parameters,
	})
}
fn record(i: &mut GateInventory, g: ReplayGate) -> quest_qsvt::Result<()> {
	let inc = |x: &mut usize, n| {
		*x = x
			.checked_add(n)
			.ok_or(quest_qsvt::Error::Budget("resource gate inventory"))?;
		Ok::<_, quest_qsvt::Error>(())
	};
	inc(&mut i.gates, 1)?;
	match g.kind {
		ReplayKind::X => inc(&mut i.x, 1)?,
		ReplayKind::H => inc(&mut i.h, 1)?,
		ReplayKind::Ry(_) => inc(&mut i.ry, 1)?,
		ReplayKind::Phase(_) => inc(&mut i.phase, 1)?,
	}
	inc(&mut i.controlled_gates, usize::from(g.control_mask != 0))?;
	inc(
		&mut i.control_occurrences,
		usize::try_from(g.control_mask.count_ones())
			.map_err(|_| quest_qsvt::Error::Budget("resource control count"))?,
	)?;
	i.maximum_controls = i.maximum_controls.max(g.control_mask.count_ones());
	Ok(())
}
fn count_pair(
	l: ConstructedResourceLimits,
	used: &mut usize,
	mut visit: impl FnMut(
		bool,
		&mut dyn FnMut(ReplayGate) -> quest_qsvt::Result<()>,
	) -> quest_qsvt::Result<()>,
) -> ReplayCount {
	let mut result = ReplayCount::default();
	let before = *used;
	for adjoint in [false, true] {
		if *used >= l.max_count_work || l.max_gates_per_orientation == 0 {
			result.rejection = Some("count allowance exhausted before replay".into());
			break;
		}
		let inventory = if adjoint {
			&mut result.adjoint
		} else {
			&mut result.forward
		};
		let outcome = visit(adjoint, &mut |g| {
			result.observed_emissions = result
				.observed_emissions
				.checked_add(1)
				.ok_or(quest_qsvt::Error::Budget("observed emission count"))?;
			if *used >= l.max_count_work || inventory.gates >= l.max_gates_per_orientation {
				return Err(quest_qsvt::Error::Budget("shared resource count allowance"));
			}
			*used = used
				.checked_add(1)
				.ok_or(quest_qsvt::Error::Budget("accepted count work"))?;
			record(inventory, g)
		});
		if let Err(e) = outcome {
			result.rejection = Some(e.to_string());
			break;
		}
	}
	result.accepted_work = used.saturating_sub(before);
	result
}
/// Construct source costs for the direct odd-SVT inverse orientation, H adjoint.
/// Zero RHS has no normalized quantum state and skips every circuit constructor.
///
/// # Errors
/// Rejects source/storage/preparation admissions and nonfinite inputs. Optional inverse,
/// counting and sampling rejections retain earlier successfully constructed source evidence.
#[allow(
	clippy::too_many_lines,
	reason = "Keep source construction, whole-live admission and optional stages in explicit ownership order"
)]
pub fn constructed_history_resources(
	history: &HistorySystem,
	l: ConstructedResourceLimits,
	inverse: Option<InverseResourceRequest>,
	observation: Option<ObservationResourceRequest<'_>>,
) -> Result<ConstructedResourceReport, CfdError> {
	let rhs_norm = history.rhs().iter().try_fold(0_f64, |n, z| {
		if !z.re.is_finite() || !z.im.is_finite() {
			return Err(CfdError::InvalidInput("nonfinite resource RHS"));
		}
		Ok(n.hypot(z.norm()))
	})?;
	if !rhs_norm.is_finite() {
		return Err(CfdError::InvalidInput("resource RHS norm overflow"));
	}
	let history_bytes = history.retained_bytes()?;
	let observation_retained = observation
		.as_ref()
		.map_or(0, |request| request.retained_bytes);
	let observation_scratch = observation
		.as_ref()
		.map_or(0, |request| request.scratch_bytes);
	let external = add(
		add(history_bytes, l.external_retained_bytes)?,
		observation_retained,
	)?;
	let preflight_peak = add(external, observation_scratch)?;
	available(l, preflight_peak)?;
	let mut r = ConstructedResourceReport {
		status: "zero-rhs-no-circuit",
		quantum_execution: false,
		history_dimension: history.operator().rows(),
		history_nonzeros: history.operator().nnz(),
		history_retained_bytes: history_bytes,
		caller_external_retained_bytes: l.external_retained_bytes,
		observation_retained_bytes: observation_retained,
		observation_scratch_bytes: observation_scratch,
		managed_peak_byte_envelope: preflight_peak,
		rhs_norm,
		count_work: 0,
		encoding: None,
		rhs_preparation: None,
		inverse: None,
		inverse_rejection: None,
		sampling: None,
		sampling_rejection: None,
		timings: ResourceTimings::default(),
		memory_scope: "whole live borrowed history + declared caller payload + constructor/synthesis managed envelopes; receipt formatting, allocator bookkeeping, opaque backend/RSS and preceding physical/history build peak excluded",
		primitive_cost_scope: "portable X/H/Ry/scalar Phase with signed controls; no Clifford+T decomposition, native dispatch-work or hardware depth claim",
	};
	if rhs_norm == 0. {
		return Ok(r);
	}
	available(l, external)?;
	let e = history.operator().nnz();
	let n = history.operator().rows().max(history.operator().cols());
	let bits = usize::try_from(
		n.checked_next_power_of_two()
			.ok_or(CfdError::InvalidInput("resource dimension"))?
			.ilog2(),
	)
	.map_err(|_| CfdError::InvalidInput("resource index bits"))?;
	// Worst candidate scans E²*K, K<=E. Completion has E² scans and bounded
	// binary searches. Its constructor also counts every default/edge/transposition gate.
	let scans = mul(mul(e, e)?, e.max(1))?;
	let completion = mul(mul(add(e, 1)?, add(e, 1)?)?, mul(add(bits, 1)?, 32)?)?;
	let model = u64::try_from(add(scans, completion)?)
		.map_err(|_| CfdError::InvalidInput("matching work model"))?;
	if model > l.max_matching_work {
		return Err(CfdError::InvalidInput(
			"matching construction work model ceiling",
		));
	}
	let started = Instant::now();
	let adjoint = history.operator().adjoint(SparseLimits {
		max_bytes: available(l, external)?,
		..SparseLimits::default()
	})?;
	charge_peak(
		&mut r,
		l,
		add(external, mul(adjoint.retained_bytes()?, 2)?)?,
	)?;
	let source = MatchingEncoding::from_sparse(
		&adjoint,
		NumericalPolicy {
			max_bytes: available(l, add(external, adjoint.retained_bytes()?)?)?,
		},
	)?;
	let sr = source.resources();
	charge_peak(
		&mut r,
		l,
		add(
			add(external, adjoint.retained_bytes()?)?,
			sr.construction_peak_bytes,
		)?,
	)?;
	drop(adjoint);
	r.timings.source_construction_seconds = started.elapsed().as_secs_f64();
	let source_live = add(external, sr.retained_bytes)?;
	let started = Instant::now();
	let prep = AmplitudePreparation::new(
		history.rhs(),
		PreparationLimits {
			max_bytes: available(l, source_live)?,
			max_compile_work: l.max_preparation_compile_work,
			max_gates: l.max_preparation_gates,
			..PreparationLimits::default()
		},
	)?;
	let pr = prep.resources();
	charge_peak(&mut r, l, add(source_live, pr.construction_peak_bytes)?)?;
	r.timings.rhs_compilation_seconds = started.elapsed().as_secs_f64();
	let retained = add(source_live, pr.retained_bytes)?;
	charge_peak(&mut r, l, add(retained, observation_scratch)?)?;
	let started = Instant::now();
	let count = count_pair(l, &mut r.count_work, |adj, v| source.visit_replay(adj, v));
	r.encoding = Some(EncodingCost {
		descriptor: descriptor(source.descriptor()?)?,
		retained_bytes: sr.retained_bytes,
		construction_peak_bytes: sr.construction_peak_bytes,
		constructor_count_pass_gates: sr.replay_gates,
		matching_scan_work_model_ceiling: model,
		count,
	});
	let count = count_pair(l, &mut r.count_work, |adj, v| prep.visit_gates(adj, v));
	r.rhs_preparation = Some(PreparationCost {
		qubits: prep.qubits(),
		padded_dimension: pr.padded_dimension,
		coefficients: pr.coefficients,
		constructor_declared_gates: pr.elementary_gates,
		compile_work: pr.compile_work,
		retained_bytes: pr.retained_bytes,
		construction_peak_bytes: pr.construction_peak_bytes,
		norm: prep.norm(),
		error_bound: prep.certified_error_bound(),
		count,
	});
	r.timings.count_seconds = started.elapsed().as_secs_f64();
	if let Some(request) = inverse {
		let started = Instant::now();
		match inverse_cost(history, &source, prep.norm(), retained, l, request, &mut r) {
			Ok(cost) => r.inverse = Some(cost),
			Err(e) => {
				r.timings.synthesis_seconds = started.elapsed().as_secs_f64();
				r.inverse_rejection = Some(e.to_string());
			}
		}
	}
	if let Some(request) = observation {
		let started = Instant::now();
		match sampling_cost(history, l, &request, &r) {
			Ok(cost) => r.sampling = Some(cost),
			Err(e) => r.sampling_rejection = Some(e.to_string()),
		}
		r.timings.sampling_validation_seconds = started.elapsed().as_secs_f64();
	}
	let rejected = r
		.encoding
		.as_ref()
		.is_some_and(|v| v.count.rejection.is_some())
		|| r.rhs_preparation
			.as_ref()
			.is_some_and(|v| v.count.rejection.is_some())
		|| r.inverse
			.as_ref()
			.is_some_and(|v| v.count.rejection.is_some())
		|| r.inverse_rejection.is_some()
		|| r.sampling_rejection.is_some();
	r.status = if rejected {
		"constructed-resources-partial"
	} else {
		"constructed-resources-counted"
	};
	Ok(r)
}
#[allow(
	clippy::too_many_arguments,
	clippy::too_many_lines,
	reason = "Carry independently admitted source, live payload and inverse-stage budgets without hidden state"
)]
fn inverse_cost(
	history: &HistorySystem,
	source: &MatchingEncoding,
	rhs_norm: f64,
	live: usize,
	l: ConstructedResourceLimits,
	request: InverseResourceRequest,
	r: &mut ConstructedResourceReport,
) -> Result<InverseCost, CfdError> {
	let started = Instant::now();
	// Spectral/polynomial constructors can allocate temporary payload and then
	// reject without returning resource telemetry. Preserve their whole admitted
	// envelope before attempting them, including on those early failure paths.
	let attempted_bytes = available(l, live)?;
	charge_peak(r, l, add(live, attempted_bytes)?)?;
	let spectrum = history.spectral_bounds(SparseLimits {
		max_bytes: attempted_bytes,
		..SparseLimits::default()
	})?;
	let polynomial = ReciprocalPolynomial::geometric(
		&spectrum,
		source.normalization().get(),
		request.approximation_tolerance,
		request.max_degree,
		NumericalPolicy {
			max_bytes: available(l, add(live, spectrum.retained_bytes()?)?)?,
		},
	)?;
	let polynomial_live = add(
		add(live, spectrum.retained_bytes()?)?,
		polynomial.retained_bytes()?,
	)?;
	let synthesis_bytes = available(l, polynomial_live)?;
	let mut policy = quest_qsp::Policy::default();
	policy.limits.resources.max_peak_bytes = synthesis_bytes;
	policy.limits.resources.max_work_units = request.max_synthesis_work;
	policy.limits.shapes.max_completion_grid = request.max_completion_grid;
	// QSP exposes its admitted workspace envelope, not allocator peak telemetry.
	charge_peak(r, l, add(polynomial_live, synthesis_bytes)?)?;
	let frozen = polynomial.synthesize(policy)?;
	let completion = frozen.completion_residual();
	let reconstruction = frozen.reconstruction_residual();
	let sequence = frozen.phase_sequence();
	drop(frozen);
	let transform = ReplayTransform::new(
		source.clone(),
		sequence,
		NumericalPolicy {
			max_bytes: available(l, polynomial_live)?,
		},
	)?;
	let schedule = transform.schedule(NumericalPolicy {
		max_bytes: available(l, polynomial_live)?,
	})?;
	charge_peak(r, l, add(polynomial_live, transform.retained_bytes()?)?)?;
	r.timings.synthesis_seconds = started.elapsed().as_secs_f64();
	let started = Instant::now();
	let mut oracle_calls = [0usize; 2];
	let mut step_rejection = None;
	for (index, adjoint) in [false, true].into_iter().enumerate() {
		let outcome = transform.visit_steps::<quest_qsvt::Error>(adjoint, |step| {
			if r.count_work >= l.max_count_work {
				return Err(quest_qsvt::Error::Budget("inverse step-count allowance"));
			}
			r.count_work = r
				.count_work
				.checked_add(1)
				.ok_or(quest_qsvt::Error::Budget("inverse step work"))?;
			if matches!(step, TransformStep::Oracle { .. }) {
				let item = oracle_calls
					.get_mut(index)
					.ok_or(quest_qsvt::Error::Budget("orientation index"))?;
				*item = item
					.checked_add(1)
					.ok_or(quest_qsvt::Error::Budget("oracle count"))?;
			}
			Ok(())
		});
		if let Err(e) = outcome {
			step_rejection = Some(e.to_string());
			break;
		}
	}
	let count = if step_rejection.is_some() {
		ReplayCount {
			rejection: step_rejection,
			..ReplayCount::default()
		}
	} else {
		count_pair(l, &mut r.count_work, |adj, v| transform.visit_gates(adj, v))
	};
	r.timings.count_seconds += started.elapsed().as_secs_f64();
	Ok(InverseCost {
		spectral_lower: spectrum.lower(),
		spectral_upper: spectrum.upper(),
		spectral_evidence: format!("{:?}", spectrum.evidence()),
		degree: transform.degree(),
		reciprocal_scale: polynomial.scale(),
		physical_rescaling: polynomial.physical_rescaling(rhs_norm)?,
		polynomial_error_bound: polynomial.error_bound(),
		coefficient_rounding_bound: polynomial.coefficient_rounding_bound(),
		projector_response_bound: None,
		execution_amplitude_error_bound: None,
		binary64_completion_diagnostic: completion,
		binary64_reconstruction_diagnostic: reconstruction,
		conversion_roundoff_estimate: schedule.conversion_roundoff_estimate(),
		phases: schedule.values().len(),
		retained_bytes: transform.retained_bytes()?,
		qubits: transform.num_qubits()?,
		oracle_calls_forward: *oracle_calls
			.first()
			.ok_or(CfdError::InvalidInput("forward query count"))?,
		oracle_calls_adjoint: *oracle_calls
			.last()
			.ok_or(CfdError::InvalidInput("adjoint query count"))?,
		count,
		synthesis_work_limit: request.max_synthesis_work,
		synthesis_byte_envelope: synthesis_bytes,
	})
}
fn times(a: u64, b: u64) -> Result<u64, CfdError> {
	a.checked_mul(b)
		.ok_or(CfdError::InvalidInput("sampling resource count overflow"))
}
fn as_count(n: usize) -> Result<u64, CfdError> {
	u64::try_from(n).map_err(|_| CfdError::InvalidInput("sampling resource width"))
}
#[allow(
	clippy::too_many_lines,
	reason = "Verify supplied source twice before retaining conditional shot and repeated-circuit costs"
)]
fn sampling_cost(
	history: &HistorySystem,
	limits: ConstructedResourceLimits,
	request: &ObservationResourceRequest<'_>,
	r: &ConstructedResourceReport,
) -> Result<SamplingCost, CfdError> {
	let callback_live = add(
		add(
			add(
				add(history.retained_bytes()?, limits.external_retained_bytes)?,
				request.retained_bytes,
			)?,
			request.scratch_bytes,
		)?,
		add(
			r.encoding
				.as_ref()
				.map_or(0, |source| source.retained_bytes),
			r.rhs_preparation
				.as_ref()
				.map_or(0, |source| source.retained_bytes),
		)?,
	)?;
	available(limits, callback_live)?;
	let inverse = r.inverse.as_ref().ok_or(CfdError::InvalidInput(
		"sampling costs require a constructed inverse schedule",
	))?;
	let preparation = r.rhs_preparation.as_ref().ok_or(CfdError::InvalidInput(
		"sampling costs require coherent RHS source",
	))?;
	if inverse.count.rejection.is_some() || preparation.count.rejection.is_some() {
		return Err(CfdError::InvalidInput(
			"sampling costs require complete replay counts",
		));
	}
	if request.description.trim().is_empty()
		|| request.description.len() > request.sampling.max_provenance_bytes
		|| request.selection_query_work == 0
		|| request.observable_query_work == 0
	{
		return Err(CfdError::InvalidInput("sampling recipe provenance/work"));
	}
	let queries = mul(history.operator().rows(), 2)?;
	let callback_work = request
		.selection_query_work
		.checked_add(request.observable_query_work)
		// Three eight-byte words require 24 XOR/multiply updates per query;
		// also allow for conversions, finite/range checks and loop bookkeeping.
		.and_then(|n| n.checked_add(128))
		.ok_or(CfdError::InvalidInput("sampling validation work overflow"))?;
	let work = times(as_count(queries)?, callback_work)?;
	if work > request.max_validation_work {
		return Err(CfdError::InvalidInput(
			"sampling source validation work budget",
		));
	}
	let mut digests = [0xcbf2_9ce4_8422_2325_u64; 2];
	for digest in &mut digests {
		for index in 0..history.operator().rows() {
			let (selected, value) = (request.query)(index)?;
			if !request.sampling.range.contains(value) {
				return Err(CfdError::InvalidInput(
					"sampling recipe exceeds supplied range",
				));
			}
			for word in [as_count(index)?, u64::from(selected), value.to_bits()] {
				for byte in word.to_le_bytes() {
					*digest = (*digest ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
				}
			}
		}
	}
	if digests.first() != digests.last() {
		return Err(CfdError::InvalidInput(
			"sampling recipe changed between validation passes",
		));
	}
	let plan = plan_sampling(request.sampling)?;
	let attempted = plan.attempted_shots;
	let selected = plan.selected_shots;
	Ok(SamplingCost {
		selected_shots: selected,
		attempted_shots: attempted,
		caller_joint_success_lower_bound: plan.caller_joint_success_lower_bound,
		success_bound_provenance: plan.success_bound_provenance.to_owned(),
		observable_description: request.description.to_owned(),
		range_lower: plan.range.lower(),
		range_upper: plan.range.upper(),
		statistical_absolute_error: plan.statistical_absolute_error,
		total_error_bound: plan.total_error_bound,
		systematic_bias_bound: plan.systematic_bias_bound,
		failure_probability: plan.failure_probability,
		attempted_preparation_primitives: times(
			attempted,
			as_count(preparation.count.forward.gates)?,
		)?,
		attempted_inverse_primitives: times(attempted, as_count(inverse.count.forward.gates)?)?,
		attempted_inverse_oracle_calls: times(attempted, as_count(inverse.oracle_calls_forward)?)?,
		measured_register_bits: times(attempted, as_count(inverse.qubits)?)?,
		classical_attempt_selection_work: times(attempted, request.selection_query_work)?,
		classical_selected_observable_work: times(selected, request.observable_query_work)?,
		validation_queries: queries,
		validation_work: work,
		source_digest: *digests
			.first()
			.ok_or(CfdError::InvalidInput("sampling source digest"))?,
		quantum_measurements_executed: false,
	})
}
