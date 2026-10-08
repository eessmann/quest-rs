//! Bounded whole-circuit history solve. This simulator is an explicit reference backend.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Finite scales are checked before use and residuals after execution"
)]
use crate::{CfdError, history::HistorySystem};
use quest_numerics::{Complex64, SparseLimits};
use quest_qsvt::{
	MatchingEncoding, NumericalPolicy,
	reciprocal::ReciprocalPolynomial,
	replay_transform::MatchingTransform,
	state_preparation::{AmplitudePreparation, PreparationLimits},
};

/// Explicit simulator selection. Native execution requires the `quantum` feature.
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum SolveBackend {
	#[default]
	ScalarReference,
	QuestCpu,
}

/// Coherent loading and classical simulator initialization carry different costs.
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum RhsPreparation {
	#[default]
	Coherent,
	SimulatorInitialization,
}

/// Independent admission limits and accuracy requirement for a circuit experiment.
#[derive(Clone, Copy, Debug)]
pub struct SolveBudget {
	/// Modeled solver-managed retained data and concurrent stage workspace.
	/// Allocator bookkeeping, loaded libraries and opaque backend scratch are excluded.
	pub max_bytes: usize,
	pub max_degree: usize,
	pub max_state_query_work: u64,
	pub relative_residual: f64,
	pub certify: bool,
	pub rhs_preparation: RhsPreparation,
}
impl Default for SolveBudget {
	fn default() -> Self {
		Self {
			max_bytes: 268_435_456,
			max_degree: 2047,
			max_state_query_work: 1_000_000_000,
			relative_residual: 0.01,
			certify: true,
			rhs_preparation: RhsPreparation::Coherent,
		}
	}
}
/// Wall-clock stages for this execution, not a scalability or speedup claim.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct SolveTimings {
	pub preprocessing_seconds: f64,
	pub synthesis_seconds: f64,
	pub certification_seconds: f64,
	pub rhs_preparation_seconds: f64,
	pub rhs_compilation_seconds: f64,
	pub backend_preparation_seconds: f64,
	pub transfer_seconds: f64,
	pub execution_seconds: f64,
	pub readout_seconds: f64,
}

/// Circuit execution and physical residual evidence. Full CFD validity is separate.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SolveReport {
	pub status: String,
	pub backend: String,
	pub history_dimension: usize,
	pub retained_physical_coordinates: Option<usize>,
	pub qubits: usize,
	pub polynomial_degree: usize,
	pub alpha: f64,
	pub spectral_lower: f64,
	pub spectral_upper: f64,
	pub reciprocal_scale: f64,
	pub approximation_error_bound: f64,
	pub projector_response_bound: Option<f64>,
	pub conversion_roundoff_estimate: f64,
	pub rhs_norm: f64,
	pub rhs_preparation: String,
	pub rhs_preparation_gates: usize,
	pub rhs_preparation_coefficients: usize,
	pub rhs_preparation_error_bound: Option<f64>,
	pub physical_rescaling: f64,
	pub success_probability: f64,
	pub relative_residual: f64,
	pub residual_tolerance: f64,
	pub timings: SolveTimings,
	/// The exact rounded decoded output is used for the physical residual.
	#[serde(skip)]
	pub solution: Vec<Complex64>,
}
/// Apply the direct non-Hermitian global inverse through odd SVT of A adjoint.
///
/// # Errors
/// Rejects any admission, synthesis/certification, execution or physical residual failure.
/// No failed quantum operation falls back to a classical PDE solution.
#[allow(
	clippy::too_many_lines,
	reason = "Single staged workflow preserves construction, certification, execution, and residual evidence"
)]
pub fn solve_history(
	history: &HistorySystem,
	budget: SolveBudget,
) -> Result<SolveReport, CfdError> {
	solve_history_with_backend(history, budget, SolveBackend::ScalarReference)
}
/// Execute the admitted circuit on the explicitly selected backend.
/// # Errors
/// Preserves construction, certification, backend and measured-residual failures.
#[allow(
	clippy::too_many_lines,
	reason = "Staged evidence is retained in one solver result"
)]
pub fn solve_history_with_backend(
	history: &HistorySystem,
	budget: SolveBudget,
	backend: SolveBackend,
) -> Result<SolveReport, CfdError> {
	solve_with_spectrum(history, budget, backend, None)
}

/// Execute with an explicitly bounded classical interval spectral proof.
///
/// This opt-in path computes evidence for this exact history and discards its
/// dense inverse candidate before circuit preparation. It does not replace the
/// circuit with that classical inverse. Analytic-default execution is unchanged.
/// # Errors
/// Rejects reference admission/proof failures and all ordinary circuit failures.
pub fn solve_history_with_reference_spectrum(
	history: &HistorySystem,
	budget: SolveBudget,
	backend: SolveBackend,
	mut reference_budget: crate::history_spectrum::ReferenceSpectrumBudget,
) -> Result<
	(
		SolveReport,
		crate::history_spectrum::ReferenceSpectrumReport,
	),
	CfdError,
> {
	reference_budget.max_bytes = reference_budget.max_bytes.min(budget.max_bytes);
	let (spectrum, report) =
		crate::history_spectrum::reference_spectrum(history, reference_budget)?;
	let result = solve_with_spectrum(history, budget, backend, Some(spectrum))?;
	Ok((result, report))
}

#[allow(
	clippy::too_many_lines,
	reason = "One staged circuit transaction preserves admission and residual evidence"
)]
fn solve_with_spectrum(
	history: &HistorySystem,
	budget: SolveBudget,
	backend: SolveBackend,
	supplied_spectrum: Option<quest_qsvt::reciprocal::SpectralBounds>,
) -> Result<SolveReport, CfdError> {
	if !budget.relative_residual.is_finite()
		|| budget.relative_residual <= 0.0
		|| budget.relative_residual >= 1.0
	{
		return Err(CfdError::InvalidInput("relative residual must be in (0,1)"));
	}
	let started = std::time::Instant::now();
	let mut timings = SolveTimings::default();
	let history_bytes = history.retained_bytes()?;
	let operator_bytes = history.operator().retained_bytes()?;
	let history_extra = history_bytes
		.checked_sub(operator_bytes)
		.ok_or(CfdError::InvalidInput("history byte accounting"))?;
	let limits = SparseLimits {
		max_bytes: remaining_bytes(budget.max_bytes, history_extra)?,
		..SparseLimits::default()
	};
	let spectrum = match supplied_spectrum {
		Some(spectrum) => spectrum,
		None => history.spectral_bounds(limits)?,
	};
	let spectrum_bytes = spectrum.retained_bytes()?;
	let adjoint = history.operator().adjoint(SparseLimits {
		max_bytes: remaining_bytes(budget.max_bytes, add_bytes(history_extra, spectrum_bytes)?)?,
		..limits
	})?;
	let encoding = MatchingEncoding::from_sparse(
		&adjoint,
		stage_policy(
			budget.max_bytes,
			add_bytes(
				add_bytes(history_bytes, spectrum_bytes)?,
				adjoint.retained_bytes()?,
			)?,
		)?,
	)?;
	drop(adjoint);
	let source_bytes = add_bytes(history_bytes, encoding.resources().retained_bytes)?;
	let policy = stage_policy(budget.max_bytes, source_bytes)?;
	let approximation_tolerance =
		budget.relative_residual * (spectrum.lower() / spectrum.upper()) * 0.001;
	let polynomial = ReciprocalPolynomial::geometric(
		&spectrum,
		encoding.normalization().get(),
		approximation_tolerance,
		budget.max_degree,
		policy,
	)?;
	let spectral_lower = spectrum.lower();
	let spectral_upper = spectrum.upper();
	drop(spectrum);
	let preprocessing_retained = add_bytes(source_bytes, polynomial.retained_bytes()?)?;
	let degree = polynomial
		.target()
		.coefficients()
		.len()
		.checked_sub(1)
		.ok_or(CfdError::Assembly("empty inverse polynomial"))?;
	let qubits = encoding
		.num_qubits()
		.checked_add(1)
		.ok_or(CfdError::InvalidInput("solver width overflow"))?;
	let state_size = 1_u64
		.checked_shl(
			u32::try_from(qubits).map_err(|_| CfdError::InvalidInput("solver width overflow"))?,
		)
		.ok_or(CfdError::InvalidInput(
			"solver state exceeds machine indexing",
		))?;
	let work = state_size
		.checked_mul(
			u64::try_from(degree).map_err(|_| CfdError::InvalidInput("solver degree overflow"))?,
		)
		.and_then(|v| v.checked_mul(2))
		.ok_or(CfdError::InvalidInput("solver work overflow"))?;
	if work > budget.max_state_query_work {
		return Err(CfdError::Unsupported(format!(
			"circuit resource rejection: {work} state-query units exceed {}",
			budget.max_state_query_work
		)));
	}
	if state_size.checked_mul(24).is_none_or(|v| {
		v > u64::try_from(remaining_bytes(budget.max_bytes, preprocessing_retained).unwrap_or(0))
			.unwrap_or(0)
	}) {
		return Err(CfdError::Unsupported(
			"circuit state and scratch exceed local simulator memory budget".to_owned(),
		));
	}
	timings.preprocessing_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	let mut synthesis_policy = quest_qsp::Policy::default();
	synthesis_policy.limits.resources.max_peak_bytes =
		remaining_bytes(budget.max_bytes, preprocessing_retained)?;
	let frozen = polynomial.synthesize(synthesis_policy)?;
	timings.synthesis_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	let certification_policy = quest_qsp::certification::CertificationPolicy {
		max_bytes: remaining_bytes(budget.max_bytes, preprocessing_retained)?,
		..quest_qsp::certification::CertificationPolicy::default()
	};
	let transform_external = add_bytes(history_bytes, polynomial.retained_bytes()?)?;
	let execution_policy = stage_policy(budget.max_bytes, transform_external)?;
	let (transform, projector_response_bound) = if budget.certify {
		let certificate = quest_qsp::certification::CertificationBuilder::new()
			.candidate(frozen)
			.policy(certification_policy)
			.map_err(|e| CfdError::Unsupported(format!("QSP certification admission: {e}")))?
			.certify()
			.map_err(|e| CfdError::Unsupported(format!("QSP certification failed: {e}")))?;
		let source_certificate_bytes = certificate
			.report()
			.attempts()
			.iter()
			.map(quest_qsp::certification::CertificationAttempt::modeled_peak_bytes)
			.max()
			.ok_or(CfdError::Assembly(
				"source certificate lacks resource evidence",
			))?;
		let projector_policy = quest_qsp::certification::CertificationPolicy {
			max_bytes: remaining_bytes(
				budget.max_bytes,
				add_bytes(preprocessing_retained, source_certificate_bytes)?,
			)?,
			..certification_policy
		};
		let projector = certificate
			.certify_projector_phases(projector_policy)
			.map_err(|e| CfdError::Unsupported(format!("projector certification failed: {e}")))?;
		drop(certificate);
		let bound = projector.response_bound().upper_f64();
		(
			MatchingTransform::from_certified_projector(encoding, projector, execution_policy)?,
			Some(bound),
		)
	} else {
		let sequence = frozen.phase_sequence();
		drop(frozen);
		(
			MatchingTransform::new(encoding, sequence, execution_policy)?,
			None,
		)
	};
	timings.certification_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	let preparation = match budget.rhs_preparation {
		RhsPreparation::Coherent => Some(AmplitudePreparation::new(
			history.rhs(),
			PreparationLimits {
				max_bytes: remaining_bytes(
					budget.max_bytes,
					add_bytes(transform_external, transform.retained_bytes()?)?,
				)?,
				max_gates: usize::try_from(
					budget.max_state_query_work.saturating_sub(work) / state_size,
				)
				.map_err(|_| CfdError::InvalidInput("preparation gate work overflow"))?,
				..PreparationLimits::default()
			},
		)?),
		RhsPreparation::SimulatorInitialization => None,
	};
	timings.rhs_compilation_seconds = started.elapsed().as_secs_f64();
	let preparation_bytes = preparation
		.as_ref()
		.map_or(0, |p| p.resources().retained_bytes);
	let target_count = preparation.as_ref().map_or(0, AmplitudePreparation::qubits);
	remaining_bytes(
		budget.max_bytes,
		add_bytes(
			add_bytes(transform_external, transform.retained_bytes()?)?,
			add_bytes(preparation_bytes, buffer_bytes::<usize>(target_count)?)?,
		)?,
	)?;
	let mut preparation_targets = Vec::new();
	preparation_targets
		.try_reserve_exact(target_count)
		.map_err(|_| CfdError::InvalidInput("preparation target allocation"))?;
	preparation_targets.extend(1..=target_count);
	let target_bytes = buffer_bytes::<usize>(preparation_targets.capacity())?;
	let preparation_bytes = add_bytes(preparation_bytes, target_bytes)?;
	let execution_policy = stage_policy(
		budget.max_bytes,
		add_bytes(transform_external, preparation_bytes)?,
	)?;
	let rhs_preparation_gates = preparation
		.as_ref()
		.map_or(0, |p| p.resources().elementary_gates);
	let rhs_preparation_coefficients = preparation
		.as_ref()
		.map_or(0, |p| p.resources().coefficients);
	let rhs_preparation_error_bound = preparation
		.as_ref()
		.and_then(AmplitudePreparation::certified_error_bound);
	let started = std::time::Instant::now();
	let (mut state, rhs_norm) = if let Some(preparation) = &preparation {
		let length = usize::try_from(state_size)
			.map_err(|_| CfdError::InvalidInput("state width overflow"))?;
		remaining_bytes(
			execution_policy.max_bytes,
			add_bytes(
				transform.retained_bytes()?,
				buffer_bytes::<Complex64>(length)?,
			)?,
		)?;
		let mut state = Vec::new();
		state
			.try_reserve_exact(length)
			.map_err(|_| CfdError::InvalidInput("coherent state allocation"))?;
		state.resize(length, Complex64::new(0.0, 0.0));
		*state
			.first_mut()
			.ok_or(CfdError::Assembly("empty coherent state"))? = Complex64::new(1.0, 0.0);
		(state, preparation.norm())
	} else {
		transform.prepare_rhs(history.rhs(), execution_policy)?
	};
	timings.rhs_preparation_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	match backend {
		SolveBackend::ScalarReference => {
			if let Some(preparation) = &preparation {
				preparation.apply_reference(
					&mut state,
					&preparation_targets,
					false,
					stage_policy(
						budget.max_bytes,
						add_bytes(
							add_bytes(transform_external, transform.retained_bytes()?)?,
							target_bytes,
						)?,
					)?,
					usize::try_from(budget.max_state_query_work.saturating_sub(work))
						.map_err(|_| CfdError::InvalidInput("preparation work width"))?,
				)?;
				timings.rhs_preparation_seconds += started.elapsed().as_secs_f64();
			}
			let started = std::time::Instant::now();
			transform.apply_reference(&mut state, false, execution_policy)?;
			timings.execution_seconds = started.elapsed().as_secs_f64();
		}
		SolveBackend::QuestCpu => {
			let native = execute_native(
				&transform,
				&mut state,
				execution_policy,
				preparation
					.as_ref()
					.map(|p| (p, preparation_targets.as_slice())),
			)?;
			timings.backend_preparation_seconds = native.backend_preparation_seconds;
			timings.transfer_seconds = native.transfer_seconds;
			timings.rhs_preparation_seconds += native.rhs_preparation_seconds;
			timings.execution_seconds = native.execution_seconds;
			timings.readout_seconds = native.readout_seconds;
		}
	}
	let started = std::time::Instant::now();
	drop(preparation);
	drop(preparation_targets);
	let conversion_roundoff_estimate = transform.conversion_roundoff_estimate();
	let decode_bytes = add_bytes(transform_external, transform.retained_bytes()?)?;
	let decode_bytes = add_bytes(decode_bytes, buffer_bytes::<Complex64>(state.capacity())?)?;
	remaining_bytes(
		budget.max_bytes,
		add_bytes(
			decode_bytes,
			buffer_bytes::<Complex64>(history.operator().rows())?,
		)?,
	)?;
	let mut solution = transform.successful_amplitudes(&state)?;
	drop(state);
	drop(transform);
	let success_probability = solution.iter().map(Complex64::norm_sqr).sum::<f64>();
	let physical_rescaling = polynomial.physical_rescaling(rhs_norm)?;
	let alpha = polynomial.alpha();
	let reciprocal_scale = polynomial.scale();
	let approximation_error_bound = polynomial.error_bound();
	drop(polynomial);
	for value in &mut solution {
		*value *= physical_rescaling;
	}
	let residual_limits = SparseLimits {
		max_bytes: remaining_bytes(
			budget.max_bytes,
			add_bytes(
				history_extra,
				buffer_bytes::<Complex64>(solution.capacity())?,
			)?,
		)?,
		..limits
	};
	let applied = history.operator().matvec(&solution, residual_limits)?;
	let residual = applied
		.iter()
		.zip(history.rhs())
		.fold(0.0_f64, |norm, (a, b)| norm.hypot((*a - *b).norm()));
	let relative_residual = residual / rhs_norm;
	if !relative_residual.is_finite() || relative_residual > budget.relative_residual {
		return Err(CfdError::Qsvt(quest_qsvt::Error::Residual {
			operation: "physical history residual",
			residual: relative_residual,
			tolerance: budget.relative_residual,
		}));
	}
	timings.readout_seconds += started.elapsed().as_secs_f64();
	Ok(SolveReport {
		status: "quantum-circuit-reference-executed".to_owned(),
		backend: match backend {
			SolveBackend::ScalarReference => {
				"bounded scalar simulation of the complete matching QSVT unitary"
			}
			SolveBackend::QuestCpu => "QuEST CPU execution of the complete matching QSVT unitary",
		}
		.to_owned(),
		history_dimension: history.operator().rows(),
		retained_physical_coordinates: None,
		qubits,
		polynomial_degree: degree,
		alpha,
		spectral_lower,
		spectral_upper,
		reciprocal_scale,
		approximation_error_bound,
		projector_response_bound,
		conversion_roundoff_estimate,
		rhs_norm,
		rhs_preparation: match budget.rhs_preparation {
			RhsPreparation::Coherent => "coherent amplitude-tree circuit",
			RhsPreparation::SimulatorInitialization => "direct classical simulator initialization",
		}
		.to_owned(),
		rhs_preparation_gates,
		rhs_preparation_coefficients,
		rhs_preparation_error_bound,
		physical_rescaling,
		success_probability,
		relative_residual,
		residual_tolerance: budget.relative_residual,
		timings,
		solution,
	})
}

#[cfg(feature = "quantum")]
fn execute_native(
	transform: &MatchingTransform,
	state: &mut [Complex64],
	policy: NumericalPolicy,
	preparation: Option<(&AmplitudePreparation, &[usize])>,
) -> Result<SolveTimings, CfdError> {
	let started = std::time::Instant::now();
	let mut timings = SolveTimings::default();
	let native_error = |e| CfdError::Unsupported(format!("QuEST execution failed: {e}"));
	let rust_retained = add_bytes(
		transform.retained_bytes()?,
		buffer_bytes::<Complex64>(state.len())?,
	)?;
	let native_policy = stage_policy(policy.max_bytes, rust_retained)?;
	let environment = quest::Environment::builder()
		.memory_budget(quest::MemoryBudget::new(native_policy.max_bytes))
		.build()
		.map_err(native_error)?;
	let count = quest::QubitCount::new(transform.num_qubits()).map_err(native_error)?;
	let shard =
		quest_qsvt::MatchingShard::from_encoding(transform.encoding(), 0, 1, native_policy)?;
	let width = transform.encoding().num_qubits();
	let mut prepared = environment
		.prepare_matching_transform(
			shard,
			count,
			(0..width).collect(),
			width,
			transform.schedule(policy)?,
		)
		.map_err(|e| CfdError::Unsupported(format!("QuEST transform preparation failed: {e}")))?;
	let mut register = environment.state_vector(count).map_err(native_error)?;
	timings.backend_preparation_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	if let Some((preparation, targets)) = preparation {
		let mut executor =
			quest::qsvt::replay_native::ReplayGateExecutor::new(&register).map_err(native_error)?;
		register.init_zero().map_err(native_error)?;
		let mut native_failure = None;
		let outcome = preparation.visit_mapped_gates(targets, 0, 0, false, &mut |gate| {
			executor.apply(&mut register, gate).map_err(|error| {
				native_failure = Some(error);
				quest_qsvt::Error::Encoding("native RHS preparation failed")
			})
		});
		if let Some(error) = native_failure {
			return Err(native_error(error));
		}
		outcome?;
		timings.rhs_preparation_seconds = started.elapsed().as_secs_f64();
	} else {
		register.init_pure(state).map_err(native_error)?;
		timings.transfer_seconds = started.elapsed().as_secs_f64();
	}
	let started = std::time::Instant::now();
	prepared
		.apply(&mut register, false)
		.map_err(|e| CfdError::Unsupported(format!("QuEST transform execution failed: {e}")))?;
	timings.execution_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	state.copy_from_slice(&register.amplitudes(0, state.len()).map_err(native_error)?);
	timings.readout_seconds = started.elapsed().as_secs_f64();
	Ok(timings)
}
#[cfg(not(feature = "quantum"))]
fn execute_native(
	_transform: &MatchingTransform,
	_state: &mut [Complex64],
	_policy: NumericalPolicy,
	_preparation: Option<(&AmplitudePreparation, &[usize])>,
) -> Result<SolveTimings, CfdError> {
	Err(CfdError::Unsupported(
		"QuEST CPU backend requires building quest-cfd with --features quantum".to_owned(),
	))
}

fn add_bytes(left: usize, right: usize) -> Result<usize, CfdError> {
	left.checked_add(right)
		.ok_or(CfdError::InvalidInput("solver byte accounting overflow"))
}
fn buffer_bytes<T>(capacity: usize) -> Result<usize, CfdError> {
	capacity
		.checked_mul(size_of::<T>())
		.and_then(|n| n.checked_add(size_of::<Vec<T>>()))
		.ok_or(CfdError::InvalidInput("solver buffer byte overflow"))
}
fn remaining_bytes(total: usize, retained: usize) -> Result<usize, CfdError> {
	total
		.checked_sub(retained)
		.filter(|remaining| *remaining > 0)
		.ok_or_else(|| {
			CfdError::Unsupported(
				"solver-managed retained storage exceeds caller memory budget".to_owned(),
			)
		})
}
fn stage_policy(total: usize, retained: usize) -> Result<NumericalPolicy, CfdError> {
	Ok(NumericalPolicy {
		max_bytes: remaining_bytes(total, retained)?,
	})
}
