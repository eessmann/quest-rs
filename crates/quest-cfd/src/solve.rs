//! Bounded whole-circuit history solve. This simulator is an explicit reference backend.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Finite scales are checked before use and residuals after execution"
)]
use crate::{CfdError, history::HistorySystem};
use quest_numerics::{Complex64, SparseLimits};
use quest_qsvt::{
	MatchingEncoding, NumericalPolicy, reciprocal::ReciprocalPolynomial,
	replay_transform::MatchingTransform,
};

/// Explicit simulator selection. Native execution requires the `quantum` feature.
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum SolveBackend {
	#[default]
	ScalarReference,
	QuestCpu,
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
}
impl Default for SolveBudget {
	fn default() -> Self {
		Self {
			max_bytes: 268_435_456,
			max_degree: 2047,
			max_state_query_work: 1_000_000_000,
			relative_residual: 0.01,
			certify: true,
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
	let spectrum = history.spectral_bounds(limits)?;
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
	synthesis_policy.limits.max_bytes = remaining_bytes(budget.max_bytes, preprocessing_retained)?;
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
	let (mut state, rhs_norm) = transform.prepare_rhs(history.rhs(), execution_policy)?;
	timings.rhs_preparation_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	match backend {
		SolveBackend::ScalarReference => {
			transform.apply_reference(&mut state, false, execution_policy)?;
			timings.execution_seconds = started.elapsed().as_secs_f64();
		}
		SolveBackend::QuestCpu => {
			let native = execute_native(&transform, &mut state, execution_policy)?;
			timings.backend_preparation_seconds = native.backend_preparation_seconds;
			timings.transfer_seconds = native.transfer_seconds;
			timings.execution_seconds = native.execution_seconds;
			timings.readout_seconds = native.readout_seconds;
		}
	}
	let started = std::time::Instant::now();
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
	register.init_pure(state).map_err(native_error)?;
	timings.transfer_seconds = started.elapsed().as_secs_f64();
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
