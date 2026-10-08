//! Streamed collective causal-history inverse with bounded scalar readout.
//!
//! No complete history, RHS, gate stream or state is collected. Spectral and
//! floating-point execution premises remain distinct from QSP certificates.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked dimensions, stage envelopes, fixed scalar packets and bounded local chunks"
)]
use crate::{
	CfdError,
	stream_history::{HistoryRowDynamics, TemporalHistoryRecipe},
};
use quest::qsvt::{
	amplitude_preparation::{
		DistributedPreparationLimits, DistributedPreparationResources, PreparationOutcome,
		PreparedAmplitudes,
	},
	matching::preprocess::{ProducerLimits, ProducerStatistics, produce_matching},
	matching_transform::collective::PreparedMatchingTransform,
};
use quest::{
	Complex64, QubitCount,
	collective::{CollectiveEnvironment, CollectiveRegister},
};
use quest_qsvt::reciprocal::{ReciprocalPolynomial, SpectralBounds, SpectralEvidence};
use quest_qsvt::replay_transform::MatchingSchedule;
use quest_qsvt::{MatchingHeader, NumericalPolicy, StandardConvention};

#[path = "distributed_probability.rs"]
mod probability;
pub use probability::{
	ProbabilityObservation, ProbabilityReadoutLimits, ProbabilityReadoutRequest,
	ProbabilityReadoutResources, RepeatedSamplingCost,
};

#[path = "distributed_temporal_observation.rs"]
mod temporal_observation;
pub use temporal_observation::{
	TemporalObservation, TemporalReadoutLimits, TemporalReadoutResources,
};

#[cfg(test)]
#[path = "distributed_history_factory_tests.rs"]
mod factory_tests;

/// Independent caller-declared initial recipe and stage budgets.
#[derive(Clone, Debug)]
pub struct DistributedHistoryLimits {
	pub max_local_bytes: usize,
	pub max_node_bytes: usize,
	/// Defaults conservatively to the full communicator on one node.
	pub ranks_per_node: usize,
	pub initial_retained_bytes: usize,
	pub initial_query_bytes: usize,
	pub initial_query_work: usize,
	pub max_rhs_query_work: usize,
	pub max_degree: usize,
	pub approximation_tolerance: f64,
	pub max_synthesis_bytes: usize,
	pub synthesis: quest_qsp::Policy,
	pub certification: Option<quest_qsp::certification::CertificationPolicy>,
	pub producer: ProducerLimits,
	pub preparation: DistributedPreparationLimits,
	pub readout_chunk_amplitudes: usize,
	pub max_readout_work: usize,
	/// Work/scratch of one caller observable/selection query.
	pub readout_query_work: usize,
	pub readout_query_bytes: usize,
	/// An explicitly supplied uniform amplitude error premise, never certified here.
	pub execution_amplitude_error: Option<f64>,
}
impl Default for DistributedHistoryLimits {
	fn default() -> Self {
		Self {
			max_local_bytes: 512 * 1024 * 1024,
			max_node_bytes: usize::MAX,
			ranks_per_node: usize::MAX,
			initial_retained_bytes: 0,
			initial_query_bytes: 0,
			initial_query_work: 1,
			max_rhs_query_work: 100_000_000,
			max_degree: 2047,
			approximation_tolerance: 1e-5,
			max_synthesis_bytes: 32 * 1024 * 1024,
			synthesis: quest_qsp::Policy::default(),
			certification: Some(quest_qsp::certification::CertificationPolicy {
				max_bytes: 32 * 1024 * 1024,
				..Default::default()
			}),
			producer: ProducerLimits::default(),
			preparation: DistributedPreparationLimits::default(),
			readout_chunk_amplitudes: 256,
			max_readout_work: 100_000_000,
			readout_query_work: 1,
			readout_query_bytes: 0,
			execution_amplitude_error: None,
		}
	}
}
/// Per-rank wall times for completed stages, including their collective waiting.
/// These observations are not critical-path, throughput or RSS measurements.
#[derive(Clone, Copy, Debug, Default)]
pub struct DistributedHistoryTimings {
	pub rhs_compilation_seconds: f64,
	pub streamed_preprocessing_seconds: f64,
	pub reciprocal_construction_seconds: f64,
	pub phase_synthesis_seconds: f64,
	pub projector_certification_seconds: f64,
	pub native_preparation_seconds: f64,
	pub rhs_initialization_seconds: f64,
	pub last_inverse_seconds: f64,
	pub cumulative_inverse_seconds: f64,
	pub completed_inverse_replays: usize,
}
/// Scalar evidence and explicit per-rank application-cost receipts.
#[derive(Debug)]
pub struct DistributedInverseReport {
	pub history_dimension: usize,
	pub qubits: usize,
	pub polynomial_degree: usize,
	pub alpha: f64,
	pub spectral_lower: f64,
	pub spectral_upper: f64,
	pub spectral_evidence: SpectralEvidence,
	pub reciprocal_scale: f64,
	pub approximation_error_bound: f64,
	pub projector_response_bound: Option<f64>,
	pub conditional_relative_residual_bound: Option<f64>,
	pub execution_amplitude_error_premise: Option<f64>,
	pub conversion_roundoff_estimate: f64,
	pub rhs_norm: f64,
	pub physical_rescaling: f64,
	pub producer: ProducerStatistics,
	pub rhs_preparation: DistributedPreparationResources,
	/// Charged full cap while the opaque frozen candidate lives, not measured RSS.
	pub synthesis_byte_envelope: usize,
	pub certificate_byte_envelope: usize,
	pub managed_external_retained_bytes: usize,
	pub managed_local_peak_bytes: usize,
	pub source_identity: u64,
	pub timings: DistributedHistoryTimings,
}
/// Zero RHS returns before matching production, polynomial synthesis or quantum mutation.
#[allow(
	clippy::large_enum_variant,
	reason = "Returning admitted owners by value avoids an infallible extra allocation"
)]
pub enum DistributedHistoryOutcome<'env, 'comm, 'runtime> {
	ZeroRhs {
		history_dimension: usize,
		source_identity: u64,
	},
	Prepared(PreparedHistoryInverse<'env, 'comm, 'runtime>),
}
/// Reusable native whole-unitary schedule and disjoint coherent RHS tables.
pub struct PreparedHistoryInverse<'env, 'comm, 'runtime> {
	history_semantics: [u64; 11],
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
	transform: PreparedMatchingTransform<'env, 'comm, 'runtime>,
	rhs: PreparedAmplitudes<'env, 'comm, 'runtime>,
	header: MatchingHeader,
	count: QubitCount,
	report: DistributedInverseReport,
	limits: DistributedHistoryLimits,
	external_bytes: usize,
	_source_reservation: quest::collective::CollectiveReservation<'env>,
	_owner_reservation: quest::collective::CollectiveReservation<'env>,
}
/// Successful mass, selected time/degree mass, and unnormalized physical observable.
#[derive(Clone, Copy, Debug)]
pub struct HistoryObservableReduction {
	pub total_probability: f64,
	pub inverse_success_probability: f64,
	pub selected_probability: f64,
	pub conditional_selected_fraction: Option<f64>,
	pub physical_selected_squared_norm: f64,
	pub physical_linear_observable: Complex64,
	pub readout_seconds: f64,
}
fn native(error: impl std::fmt::Display) -> CfdError {
	CfdError::Unsupported(format!("collective history runtime: {error}"))
}
fn plus(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_add(b)
		.ok_or(CfdError::InvalidInput("distributed history count overflow"))
}
fn times(a: usize, b: usize) -> Result<usize, CfdError> {
	a.checked_mul(b)
		.ok_or(CfdError::InvalidInput("distributed history count overflow"))
}
fn agree<T>(
	env: &CollectiveEnvironment<'_, '_>,
	value: Result<T, CfdError>,
) -> Result<T, CfdError> {
	let ok = env
		.communicator()
		.collective_lane()
		.map_err(native)?
		.all_agree(value.is_ok())
		.map_err(native)?;
	if !ok {
		return Err(value
			.err()
			.unwrap_or(CfdError::InvalidInput("peer history admission failed")));
	}
	value
}
fn common(env: &CollectiveEnvironment<'_, '_>, words: &[u64]) -> Result<(), CfdError> {
	let mut lane = env.communicator().collective_lane().map_err(native)?;
	for word in words {
		let mut packet = word.to_le_bytes();
		lane.broadcast_bytes(0, &mut packet).map_err(native)?;
		if !lane
			.all_agree(packet == word.to_le_bytes())
			.map_err(native)?
		{
			return Err(CfdError::InvalidInput(
				"inconsistent collective history metadata",
			));
		}
	}
	Ok(())
}
fn capacity(
	env: &CollectiveEnvironment<'_, '_>,
	external: Result<usize, CfdError>,
	l: &DistributedHistoryLimits,
) -> Result<usize, CfdError> {
	let own = agree(
		env,
		(|| plus(env.view().memory_budget().bytes(), external?))(),
	)?;
	let parts = usize::try_from(env.size().map_err(native)?)
		.map_err(|_| CfdError::InvalidInput("history communicator size"))?;
	let per_node = if l.ranks_per_node == usize::MAX {
		parts
	} else {
		l.ranks_per_node
	};
	let mut maximum = 0;
	{
		let mut lane = env.communicator().collective_lane().map_err(native)?;
		for peer in 0..parts {
			let mut p = u64::try_from(own)
				.map_err(|_| CfdError::InvalidInput("history byte width"))?
				.to_le_bytes();
			lane.broadcast_bytes(
				i32::try_from(peer).map_err(|_| CfdError::InvalidInput("history peer"))?,
				&mut p,
			)
			.map_err(native)?;
			maximum = maximum.max(
				usize::try_from(u64::from_le_bytes(p))
					.map_err(|_| CfdError::InvalidInput("history byte width"))?,
			);
		}
	}
	agree(
		env,
		if per_node == 0
			|| per_node > parts
			|| maximum > l.max_local_bytes
			|| times(maximum, per_node)? > l.max_node_bytes
		{
			Err(CfdError::InvalidInput(
				"distributed history live rank/node capacity",
			))
		} else {
			Ok(maximum)
		},
	)
}
const fn evidence_bytes(e: &SpectralEvidence) -> usize {
	match e {
		SpectralEvidence::Analytic { description }
		| SpectralEvidence::DenseReference { description, .. }
		| SpectralEvidence::CallerPremise { description } => description.capacity(),
	}
}
const fn policy(bytes: usize) -> NumericalPolicy {
	NumericalPolicy { max_bytes: bytes }
}
/// Prepare a direct odd SVT inverse of H through a streamed encoding of H adjoint.
///
/// The scalar initial callback can use `SymmetricCarleman::lift_entry` directly.
/// Every rank calls in the same order; callback declarations and spectral evidence
/// are caller premises. Errors agree before the next collective construction stage.
/// # Errors
/// Rejects malformed common metadata, callback/row errors, work/storage/node limits,
/// inadequate reciprocal/QSP evidence, native deployment or preparation admission.
pub fn prepare_history_inverse<'env, 'comm, 'runtime, D: HistoryRowDynamics>(
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	recipe: &TemporalHistoryRecipe<'_, D>,
	initial: impl FnMut(usize) -> Result<Complex64, CfdError>,
	spectrum: &SpectralBounds,
	l: DistributedHistoryLimits,
) -> Result<DistributedHistoryOutcome<'env, 'comm, 'runtime>, CfdError> {
	prepare_history_inverse_from_factory(env, recipe, initial, spectrum, l, |range| {
		Ok(recipe.rows(range)?.map(|entry| {
			entry.map_err(|_| quest_numerics::Error::Length("history row recipe failed"))
		}))
	})
}

/// Internal source-construction boundary after nonzero coherent RHS preparation.
///
/// Every rank invokes the factory once at the same stage, then agrees its result.
/// A factory may execute an agreed collective construction protocol; it must admit
/// and guard its additional live resources and agree recoverable failures internally.
/// The returned iterator must perform no MPI calls and yields original H entries.
/// This backend alone conjugate-transposes them, preserving input ordinals.
/// Factory-only resources are not inferred from the temporal recipe declaration.
/// A zero RHS returns before invoking the factory or performing sparse construction.
#[allow(
	clippy::too_many_lines,
	reason = "One staged ownership transaction keeps collective admission and temporary envelopes explicit"
)]
pub(crate) fn prepare_history_inverse_from_factory<'env, 'comm, 'runtime, D, F, I>(
	env: &'env CollectiveEnvironment<'comm, 'runtime>,
	recipe: &TemporalHistoryRecipe<'_, D>,
	mut initial: impl FnMut(usize) -> Result<Complex64, CfdError>,
	spectrum: &SpectralBounds,
	mut l: DistributedHistoryLimits,
	source_factory: F,
) -> Result<DistributedHistoryOutcome<'env, 'comm, 'runtime>, CfdError>
where
	D: HistoryRowDynamics,
	F: FnOnce(std::ops::Range<usize>) -> Result<I, CfdError>,
	I: Iterator<Item = quest_numerics::Result<quest_numerics::sparse_stream::SparseEntry>>,
{
	let mut timings = DistributedHistoryTimings::default();
	let n = recipe.dimension();
	let temporal_words = agree(env, recipe.semantic_words())?;
	common(env, &temporal_words)?;
	let parts = usize::try_from(env.size().map_err(native)?)
		.map_err(|_| CfdError::InvalidInput("history communicator size"))?;
	let rank = usize::try_from(env.rank().map_err(native)?)
		.map_err(|_| CfdError::InvalidInput("history communicator rank"))?;
	common(
		env,
		&[
			u64::try_from(n).map_err(|_| CfdError::InvalidInput("history width"))?,
			l.approximation_tolerance.to_bits(),
			spectrum.lower().to_bits(),
			spectrum.upper().to_bits(),
			u64::try_from(l.max_local_bytes)
				.map_err(|_| CfdError::InvalidInput("history bytes"))?,
			u64::try_from(l.max_node_bytes).map_err(|_| CfdError::InvalidInput("history bytes"))?,
			u64::try_from(l.ranks_per_node)
				.map_err(|_| CfdError::InvalidInput("history placement"))?,
			u64::try_from(l.max_degree).map_err(|_| CfdError::InvalidInput("history degree"))?,
			u64::try_from(l.readout_chunk_amplitudes)
				.map_err(|_| CfdError::InvalidInput("history readout"))?,
			u64::from(l.certification.is_some()),
		],
	)?;
	let recipe_bytes = agree(
		env,
		(|| {
			plus(
				recipe.resources().peak_managed_bytes,
				plus(
					l.initial_retained_bytes,
					plus(spectrum.retained_bytes()?, 8192)?,
				)?,
			)
		})(),
	)?;
	let mut peak = capacity(env, plus(recipe_bytes, l.initial_query_bytes), &l)?;
	agree(
		env,
		(|| {
			if l.readout_chunk_amplitudes == 0
				|| l.max_synthesis_bytes == 0
				|| l.execution_amplitude_error
					.is_some_and(|e| !e.is_finite() || e < 0.)
			{
				return Err(CfdError::InvalidInput(
					"distributed history numerical/readout limits",
				));
			}
			let padded = n
				.checked_next_power_of_two()
				.ok_or(CfdError::InvalidInput("history padding"))?;
			let extent = padded
				.checked_div(parts)
				.ok_or(CfdError::InvalidInput("history extent"))?;
			let calls = n.saturating_sub(times(rank, extent)?).min(extent);
			if times(
				calls,
				plus(
					recipe.resources().maximum_work_per_row,
					l.initial_query_work,
				)?,
			)? > l.max_rhs_query_work
			{
				return Err(CfdError::InvalidInput("distributed RHS scalar query work"));
			}
			Ok(())
		})(),
	)?;
	let native_cap = env.view().memory_budget().bytes();
	l.preparation.max_local_bytes = l.preparation.max_local_bytes.min(native_cap);
	l.preparation.ranks_per_node = if l.ranks_per_node == usize::MAX {
		parts
	} else {
		l.ranks_per_node
	};
	l.preparation.node_budget = quest::MemoryBudget::new(l.max_node_bytes);
	let source_reservation = env.reserve_external_bytes(recipe_bytes).map_err(native)?;
	let started = std::time::Instant::now();
	let rhs = match env
		.prepare_amplitudes_from_fn(n, l.preparation, |index| {
			recipe
				.rhs_value(index, &mut initial)
				.map_err(|_| quest::Error::Value("history scalar RHS callback"))
		})
		.map_err(native)?
	{
		PreparationOutcome::Zero { source_identity } => {
			return Ok(DistributedHistoryOutcome::ZeroRhs {
				history_dimension: n,
				source_identity,
			});
		}
		PreparationOutcome::Prepared(p) => p,
	};
	timings.rhs_compilation_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	let rhs_resources = rhs.resources();
	let (start, end) = agree(
		env,
		(|| Ok((times(rank, n)? / parts, times(plus(rank, 1)?, n)? / parts)))(),
	)?;
	let mut rows = agree(env, source_factory(start..end))?;
	// Catch caller row panics as iterator errors so producer agreement can stop all ranks.
	let mut failed = false;
	let entries = std::iter::from_fn(move || {
		if failed {
			return None;
		}
		match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rows.next())) {
			Ok(Some(Ok(mut entry))) => {
				std::mem::swap(&mut entry.row, &mut entry.column);
				entry.value = entry.value.conj();
				Some(Ok(entry))
			}
			Ok(Some(Err(_))) | Err(_) => {
				failed = true;
				Some(Err(quest_numerics::Error::Length(
					"history row recipe failed",
				)))
			}
			Ok(None) => None,
		}
	});
	let recipe_peak = capacity(env, Ok(recipe_bytes), &l)?;
	let remaining = agree(
		env,
		l.max_local_bytes
			.checked_sub(recipe_peak)
			.ok_or(CfdError::InvalidInput("history producer remaining bytes")),
	)?;
	l.producer.max_bytes = l.producer.max_bytes.min(remaining);
	peak = peak.max(capacity(env, plus(recipe_bytes, l.producer.max_bytes), &l)?);
	let owned_source =
		produce_matching(env.communicator(), n, n, entries, l.producer.clone()).map_err(native)?;
	let (shard, edges, reverse, producer) = owned_source.into_parts();
	drop(edges);
	drop(reverse);
	timings.streamed_preprocessing_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	let header = shard.header();
	let source_bytes = agree(env, (|| plus(recipe_bytes, shard.storage_bytes()?))())?;
	let source_peak = capacity(env, Ok(source_bytes), &l)?;
	peak = peak.max(source_peak);
	let polynomial_room = agree(
		env,
		l.max_local_bytes
			.checked_sub(source_peak)
			.ok_or(CfdError::InvalidInput("history polynomial remaining bytes")),
	)?;
	let polynomial = agree(
		env,
		ReciprocalPolynomial::geometric(
			spectrum,
			header.alpha,
			l.approximation_tolerance,
			l.max_degree,
			policy(polynomial_room),
		)
		.map_err(CfdError::from),
	)?;
	timings.reciprocal_construction_seconds = started.elapsed().as_secs_f64();
	let retained = agree(env, (|| plus(source_bytes, polynomial.retained_bytes()?))())?;
	let cert_cap = l.certification.map_or(0, |p| p.max_bytes);
	let scalar_envelope = agree(env, (|| plus(l.max_synthesis_bytes, times(cert_cap, 2)?))())?;
	peak = peak.max(capacity(env, plus(retained, scalar_envelope), &l)?);
	l.synthesis.limits.resources.max_peak_bytes = l
		.synthesis
		.limits
		.resources
		.max_peak_bytes
		.min(l.max_synthesis_bytes);
	let started = std::time::Instant::now();
	let candidate = agree(
		env,
		polynomial.synthesize(l.synthesis).map_err(CfdError::from),
	)?;
	timings.phase_synthesis_seconds = started.elapsed().as_secs_f64();
	let started = std::time::Instant::now();
	let (phases, readout, bound, conversion) = if let Some(cp) = l.certification {
		let certified = agree(
			env,
			quest_qsp::certification::CertificationBuilder::new()
				.candidate(candidate)
				.policy(cp)
				.and_then(quest_qsp::certification::CertificationBuilder::certify)
				.map_err(native),
		)?;
		let projector = agree(env, certified.certify_projector_phases(cp).map_err(native))?;
		peak = peak.max(capacity(
			env,
			(|| {
				plus(
					retained,
					plus(scalar_envelope, times(projector.values().len(), 8)?)?,
				)
			})(),
			&l,
		)?);
		let phases = agree(
			env,
			(|| {
				let mut v = Vec::new();
				v.try_reserve_exact(projector.values().len())
					.map_err(|_| CfdError::InvalidInput("history phase allocation"))?;
				v.extend_from_slice(projector.values());
				Ok(v)
			})(),
		)?;
		(
			phases,
			projector.readout_phase(),
			Some(projector.response_bound().upper_f64()),
			0.0,
		)
	} else {
		peak = peak.max(capacity(
			env,
			(|| {
				plus(
					retained,
					plus(scalar_envelope, times(candidate.phases().len(), 16)?)?,
				)
			})(),
			&l,
		)?);
		agree(
			env,
			(|| {
				let sequence = candidate.phase_sequence();
				let converted = quest_qsp::WxSymmetric::projector_phases(&sequence);
				let degree = sequence.degree();
				let readout = -f64::from(
					u32::try_from(degree.saturating_sub(1) % 4)
						.map_err(|_| CfdError::InvalidInput("history readout phase"))?,
				) * std::f64::consts::PI;
				let mut phases = Vec::new();
				phases
					.try_reserve_exact(converted.values().len())
					.map_err(|_| CfdError::InvalidInput("history phase allocation"))?;
				phases.extend_from_slice(converted.values());
				Ok((phases, readout, None, converted.roundoff_estimate()))
			})(),
		)?
	};
	timings.projector_certification_seconds = started.elapsed().as_secs_f64();
	let schedule = agree(
		env,
		MatchingSchedule::from_parts(header, phases, readout, policy(remaining))
			.map_err(CfdError::from),
	)?;
	let degree = schedule.degree();
	let count = agree(
		env,
		(|| QubitCount::new(schedule.num_qubits()?).map_err(native))(),
	)?;
	let report = agree(
		env,
		(|| {
			Ok(DistributedInverseReport {
				history_dimension: n,
				qubits: count.get(),
				polynomial_degree: degree,
				alpha: header.alpha,
				spectral_lower: spectrum.lower(),
				spectral_upper: spectrum.upper(),
				spectral_evidence: spectrum.evidence().clone(),
				reciprocal_scale: polynomial.scale(),
				approximation_error_bound: polynomial.error_bound(),
				projector_response_bound: bound,
				conditional_relative_residual_bound: match (bound, l.execution_amplitude_error) {
					(Some(b), Some(e)) => Some(polynomial.relative_residual_bound(b, e)?),
					_ => None,
				},
				execution_amplitude_error_premise: l.execution_amplitude_error,
				conversion_roundoff_estimate: conversion,
				rhs_norm: rhs.norm(),
				physical_rescaling: polynomial.physical_rescaling(rhs.norm())?,
				producer,
				rhs_preparation: rhs_resources,
				synthesis_byte_envelope: l.max_synthesis_bytes,
				certificate_byte_envelope: times(cert_cap, 2)?,
				managed_external_retained_bytes: 0,
				managed_local_peak_bytes: peak,
				source_identity: header.source_identity,
				timings,
			})
		})(),
	)?;
	drop(polynomial);
	let started = std::time::Instant::now();
	let targets = agree(
		env,
		(|| {
			let mut targets = Vec::new();
			targets
				.try_reserve_exact(header.num_qubits()?)
				.map_err(|_| CfdError::InvalidInput("history target allocation"))?;
			targets.extend(0..header.num_qubits()?);
			Ok(targets)
		})(),
	)?;
	let transform = env
		.prepare_matching_transform(shard, count, targets, header.num_qubits()?, schedule)
		.map_err(native)?;
	let owner_bytes = agree(
		env,
		(|| {
			plus(
				size_of::<PreparedHistoryInverse<'_, '_, '_>>(),
				plus(
					evidence_bytes(&report.spectral_evidence),
					transform.schedule().retained_bytes()?,
				)?,
			)
		})(),
	)?;
	let owner_reservation = env.reserve_external_bytes(owner_bytes).map_err(native)?;
	let mut prepared = PreparedHistoryInverse {
		history_semantics: recipe.semantic_words()?,
		environment: env,
		transform,
		rhs,
		header,
		count,
		report,
		limits: l,
		external_bytes: 0,
		_source_reservation: source_reservation,
		_owner_reservation: owner_reservation,
	};
	prepared.report.timings.native_preparation_seconds = started.elapsed().as_secs_f64();
	// The complete native cap includes owned matching/preparation tables and local state.
	prepared.external_bytes = agree(
		env,
		(|| {
			plus(
				recipe_bytes,
				plus(
					size_of::<PreparedHistoryInverse<'_, '_, '_>>(),
					plus(
						evidence_bytes(&prepared.report.spectral_evidence),
						prepared.transform.schedule().retained_bytes()?,
					)?,
				)?,
			)
		})(),
	)?;
	prepared.report.managed_external_retained_bytes = prepared.external_bytes;
	prepared.report.managed_local_peak_bytes = prepared.report.managed_local_peak_bytes.max(
		capacity(env, Ok(prepared.external_bytes), &prepared.limits)?,
	);
	Ok(DistributedHistoryOutcome::Prepared(prepared))
}
impl PreparedHistoryInverse<'_, '_, '_> {
	#[must_use]
	pub const fn report(&self) -> &DistributedInverseReport {
		&self.report
	}
	#[must_use]
	pub const fn qubit_count(&self) -> QubitCount {
		self.count
	}
	fn admit_register(&self, state: &CollectiveRegister<'_, '_, '_>) -> Result<(), CfdError> {
		agree(
			self.environment,
			if std::ptr::eq(state.collective_environment(), self.environment)
				&& state.num_qubits() == self.count
			{
				Ok(())
			} else {
				Err(CfdError::InvalidInput("distributed history register width"))
			},
		)?;
		capacity(self.environment, Ok(self.external_bytes), &self.limits)?;
		Ok(())
	}
	/// Prepare the RHS on system bits 1..q without touching signed inactive branches.
	/// # Errors
	/// Rejects mismatched owners, widths, controls or live native/storage budgets before mutation.
	pub fn prepare_rhs(
		&mut self,
		state: &mut CollectiveRegister<'_, '_, '_>,
		control_mask: usize,
		control_value: usize,
		adjoint: bool,
	) -> Result<(), CfdError> {
		self.admit_register(state)?;
		let mut targets = [0; 64];
		for (i, t) in targets
			.iter_mut()
			.take(self.header.system_qubits)
			.enumerate()
		{
			*t = plus(i, 1)?;
		}
		self.rhs
			.apply(
				state,
				&targets[..self.header.system_qubits],
				control_mask,
				control_value,
				adjoint,
			)
			.map_err(native)
	}
	/// Initialize a normalized pure RHS coherently; this is an explicit state replacement.
	/// # Errors
	/// Rejects register admission before initialization; later native failures are fatal runtime boundaries.
	pub fn initialize_rhs(
		&mut self,
		state: &mut CollectiveRegister<'_, '_, '_>,
	) -> Result<(), CfdError> {
		let started = std::time::Instant::now();
		self.admit_register(state)?;
		let mut targets = [0; 64];
		for (i, t) in targets
			.iter_mut()
			.take(self.header.system_qubits)
			.enumerate()
		{
			*t = plus(i, 1)?;
		}
		self.rhs
			.admit_apply(state, &targets[..self.header.system_qubits], 0, 0, false)
			.map_err(native)?;
		state.init_zero().map_err(native)?;
		// Preflight releases its temporary executor; after the explicit reset,
		// any actual reallocation or replay-entry failure is a fatal MPI boundary.
		match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			self.prepare_rhs(state, 0, 0, false)
		})) {
			Ok(Ok(())) => {
				self.report.timings.rhs_initialization_seconds = started.elapsed().as_secs_f64();
				Ok(())
			}
			Ok(Err(_)) | Err(_) => quest::collective::abort_job(),
		}
	}
	/// Replay the complete inverse unitary or literal adjoint, retaining every failure/padding sector.
	/// # Errors
	/// Rejects common operation/register/storage admission or propagates native runtime failure.
	pub fn apply_inverse(
		&mut self,
		state: &mut CollectiveRegister<'_, '_, '_>,
		adjoint: bool,
	) -> Result<(), CfdError> {
		self.admit_register(state)?;
		let started = std::time::Instant::now();
		self.transform.apply(state, adjoint).map_err(native)?;
		let elapsed = started.elapsed().as_secs_f64();
		self.report.timings.last_inverse_seconds = elapsed;
		self.report.timings.cumulative_inverse_seconds += elapsed;
		self.report.timings.completed_inverse_replays = self
			.report
			.timings
			.completed_inverse_replays
			.saturating_add(1);
		Ok(())
	}
	/// Reduce a caller-defined time/degree selection and linear observable in bounded chunks.
	/// The callback returns `(selected, weight)` for a logical history coordinate;
	/// the observable is `sum selected weight_i * physical_solution_i` (no conjugation).
	/// No projection, conditioning, solution vector or global state is constructed.
	/// # Errors
	/// Rejects readout work/storage, callback failures or nonfinite sums collectively.
	#[allow(
		clippy::too_many_lines,
		reason = "One bounded read/agree/reduce transaction publishes no partial observable"
	)]
	pub fn reduce_observable(
		&self,
		state: &CollectiveRegister<'_, '_, '_>,
		mut select: impl FnMut(usize) -> Result<(bool, Complex64), CfdError>,
	) -> Result<HistoryObservableReduction, CfdError> {
		let started = std::time::Instant::now();
		self.admit_register(state)?;
		let local = state.deployment().local_amplitudes();
		let chunk = self.limits.readout_chunk_amplitudes.min(local);
		common(
			self.environment,
			&[
				u64::try_from(local).map_err(|_| CfdError::InvalidInput("history local width"))?,
				u64::try_from(chunk).map_err(|_| CfdError::InvalidInput("history chunk"))?,
			],
		)?;
		agree(
			self.environment,
			(|| {
				if times(local, plus(self.limits.readout_query_work, 16)?)?
					> self.limits.max_readout_work
				{
					Err(CfdError::InvalidInput("history readout query work"))
				} else {
					Ok(())
				}
			})(),
		)?;
		capacity(
			self.environment,
			(|| {
				plus(
					self.external_bytes,
					plus(times(chunk, 32)?, self.limits.readout_query_bytes)?,
				)
			})(),
			&self.limits,
		)?;
		let start = times(state.deployment().rank(), local)?;
		let mut sums = [0.0; 5];
		for offset in (0..local).step_by(chunk) {
			let values = state
				.read_local_amplitudes(offset, (local - offset).min(chunk))
				.map_err(native)?;
			agree(
				self.environment,
				std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
					for (i, z) in values.iter().enumerate() {
						sums[0] += z.norm_sqr();
						let physical = plus(start, plus(offset, i)?)?;
						// Matching flag/color/response must be zero and logical padding excluded.
						let logical = physical / 2;
						if physical % 2 == 0 && logical < self.header.rows {
							sums[1] += z.norm_sqr();
							let (selected, weight) = select(logical)?;
							if !weight.re.is_finite() || !weight.im.is_finite() {
								return Err(CfdError::InvalidInput(
									"nonfinite history observable weight",
								));
							}
							if selected {
								sums[2] += z.norm_sqr();
								let observable = weight * z;
								sums[3] += observable.re;
								sums[4] += observable.im;
							}
						}
					}
					if sums.iter().any(|x| !x.is_finite()) {
						return Err(CfdError::InvalidInput("nonfinite history readout"));
					}
					Ok(())
				}))
				.unwrap_or(Err(CfdError::InvalidInput(
					"history observable callback panicked",
				))),
			)?;
		}
		let own = sums;
		let mut global = [0.0; 5];
		{
			let mut lane = self
				.environment
				.communicator()
				.collective_lane()
				.map_err(native)?;
			for peer in 0..self.environment.size().map_err(native)? {
				let mut packet = [0; 40];
				for (slot, value) in packet.as_chunks_mut::<8>().0.iter_mut().zip(own) {
					slot.copy_from_slice(&value.to_le_bytes());
				}
				lane.broadcast_bytes(peer, &mut packet).map_err(native)?;
				for (slot, value) in packet.as_chunks::<8>().0.iter().zip(&mut global) {
					*value += f64::from_le_bytes(*slot);
				}
			}
		}
		let scale = self.report.physical_rescaling;
		let physical_squared_norm = global[2] * scale * scale;
		let physical_observable = Complex64::new(global[3] * scale, global[4] * scale);
		agree(
			self.environment,
			if global.iter().any(|x| !x.is_finite())
				|| !physical_squared_norm.is_finite()
				|| !physical_observable.re.is_finite()
				|| !physical_observable.im.is_finite()
			{
				Err(CfdError::InvalidInput("history reduction overflow"))
			} else {
				Ok(())
			},
		)?;
		Ok(HistoryObservableReduction {
			total_probability: global[0],
			inverse_success_probability: global[1],
			selected_probability: global[2],
			conditional_selected_fraction: (global[1] > 0.).then(|| global[2] / global[1]),
			physical_selected_squared_norm: physical_squared_norm,
			physical_linear_observable: physical_observable,
			readout_seconds: started.elapsed().as_secs_f64(),
		})
	}
}
