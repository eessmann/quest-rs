//! CPU/MPI postselection simulation of inverse -> coherent time map -> diagonal readout.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::suboptimal_flops,
	reason = "Checked dimensions and fixed protocol frames bound collective composition arithmetic"
)]
use super::{
	PreparedHistoryInverse, ProbabilityReadoutLimits, ProbabilityReadoutRequest,
	ProbabilityReadoutResources, agree, capacity, common, native, plus, times,
};
use crate::{
	CfdError,
	physical_observation::GridPhysicalObservable,
	probability_observation::{HistoryProjection, TemporalNodeSide},
	temporal_encoding::TemporalEncoding,
};
use quest::{
	Outcome,
	collective::{CollectiveEnvironment, CollectiveRegister},
	qsvt::replay_native::CollectiveReplayGateExecutor,
};
use quest_qsvt::{ReplayEncoding, ReplayGate, ReplayKind};

/// Incremental composition admission. Previously constructed source costs are reported separately.
#[derive(Clone, Copy, Debug)]
pub struct TemporalReadoutLimits {
	/// Caller provenance binding the physical chart, snapshot and grid on all ranks.
	/// Equal dimensions and this integer do not prove that provenance. Zero rejects.
	pub source_identity: u64,
	pub chunk_amplitudes: usize,
	pub max_bytes: usize,
	pub max_work: usize,
	pub max_transport_bytes: usize,
	pub max_temporal_gates: usize,
}
impl Default for TemporalReadoutLimits {
	fn default() -> Self {
		Self {
			source_identity: 0,
			chunk_amplitudes: 256,
			max_bytes: 64 * 1024 * 1024,
			max_work: 1_000_000_000,
			max_transport_bytes: 1_000_000_000,
			max_temporal_gates: 1_000_000,
		}
	}
}
/// Bounds distinguish source construction already completed from incremental simulator work.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct TemporalReadoutResources {
	pub temporal_gates: usize,
	pub inverse_workspace_projections: usize,
	pub temporal_workspace_projections: usize,
	pub native_gate_dispatches: usize,
	pub native_projection_dispatches: usize,
	pub native_probability_dispatches: usize,
	pub amplitude_scan_passes: usize,
	pub callback_query_bound: usize,
	pub incremental_global_work_bound: usize,
	pub native_amplitude_transport_byte_bound: usize,
	pub collective_control_transport_byte_bound: usize,
	pub readout_transport_byte_bound: usize,
	pub local_extra_bytes: usize,
	pub maximum_rank_envelope_bytes: usize,
	pub maximum_node_envelope_bytes: usize,
	pub ranks_per_node: usize,
	pub previous_encoding_preparation_per_rank: usize,
	pub previous_encoding_preparation_aggregate: usize,
	pub previous_observable_preparation_per_rank: usize,
	pub previous_observable_preparation_aggregate: usize,
	pub replay_source_visits: usize,
	pub readout: ProbabilityReadoutResources,
}
/// Binary64 simulator evidence, not a quantum shot execution or physical solution certificate.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct TemporalObservation {
	pub original_total_mass: f64,
	pub original_inverse_logical_success_mass: f64,
	pub original_inverse_logical_success_probability: f64,
	pub original_workspace_failure_mass: f64,
	pub original_clean_padding_mass: f64,
	pub projected_temporal_total_mass: f64,
	pub joint_temporal_success_mass: f64,
	/// Relative to ORIGINAL input mass, including inverse and temporal postselection.
	pub joint_temporal_success_probability: f64,
	pub temporal_success_given_inverse_logical: Option<f64>,
	pub weighted_first_moment: f64,
	pub weighted_second_moment: f64,
	pub conditional_expectation: Option<f64>,
	pub conditional_variance: Option<f64>,
	pub physical_amplitude_scale: f64,
	pub physical_selected_squared_norm: f64,
	pub physical_scaled_quadratic_functional: f64,
	pub physical_time: f64,
	pub temporal_slab: usize,
	pub temporal_side: TemporalNodeSide,
	pub parent_history_source_identity: u64,
	pub temporal_source_identity: u64,
	pub temporal_construction_identity: u64,
	pub source_identity: u64,
	/// True only when the final binary64 total mass is exactly zero, with no tolerance.
	pub exact_zero_projected_branch: bool,
	pub quantum_measurements_executed: bool,
	pub resources: TemporalReadoutResources,
}
const fn invalid() -> CfdError {
	CfdError::InvalidInput("temporal postselection admission, source or arithmetic")
}
fn word(value: usize) -> Result<u64, CfdError> {
	u64::try_from(value).map_err(|_| invalid())
}
fn fatal<T>(operation: impl FnOnce() -> Result<T, CfdError>) -> T {
	match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
		Ok(Ok(value)) => value,
		Ok(Err(_)) | Err(_) => quest::collective::abort_job(),
	}
}
fn validate_gate(gate: ReplayGate, width: usize) -> quest_qsvt::Result<()> {
	let bad = || quest_qsvt::Error::Encoding("temporal mapped native gate");
	let extent = 1usize
		.checked_shl(u32::try_from(width).map_err(|_| bad())?)
		.ok_or_else(bad)?;
	if gate.control_mask >= extent || gate.control_value & !gate.control_mask != 0 {
		return Err(bad());
	}
	match gate.kind {
		ReplayKind::Phase(angle) => {
			if !angle.is_finite() || gate.target.is_some() {
				return Err(bad());
			}
		}
		ReplayKind::H | ReplayKind::X | ReplayKind::Ry(_) => {
			let target = gate.target.ok_or_else(bad)?;
			if target >= width
				|| gate.control_mask & (1usize << target) != 0
				|| matches!(gate.kind,ReplayKind::Ry(a) if !a.is_finite())
			{
				return Err(bad());
			}
		}
	}
	Ok(())
}
fn original_branches(
	env: &CollectiveEnvironment<'_, '_>,
	state: &CollectiveRegister<'_, '_, '_>,
	workspace: usize,
	history: usize,
	chunk: usize,
) -> Result<[f64; 2], CfdError> {
	let local = state.deployment().local_amplitudes();
	let start = agree(env, times(local, state.deployment().rank()))?;
	let mut own = [0.; 2];
	for offset in (0..local).step_by(chunk) {
		let values = state
			.read_local_amplitudes(offset, (local - offset).min(chunk))
			.map_err(native)?;
		agree(
			env,
			(|| {
				if times(values.capacity(), 16)? > times(chunk, 32)? {
					return Err(invalid());
				}
				for (i, z) in values.iter().enumerate() {
					let physical = plus(start, plus(offset, i)?)?;
					let mass = z.norm_sqr();
					if physical & workspace != 0 {
						own[0] += mass;
					} else if physical / 2 >= history {
						own[1] += mass;
					}
				}
				if own.iter().all(|v| v.is_finite()) {
					Ok(())
				} else {
					Err(invalid())
				}
			})(),
		)?;
	}
	let mut total = [0.; 2];
	let mut lane = env.communicator().collective_lane().map_err(native)?;
	for peer in 0..env.size().map_err(native)? {
		let mut packet = [0; 16];
		for (slot, value) in packet.as_chunks_mut::<8>().0.iter_mut().zip(own) {
			slot.copy_from_slice(&value.to_le_bytes());
		}
		lane.broadcast_bytes(peer, &mut packet).map_err(native)?;
		for (value, slot) in total.iter_mut().zip(packet.as_chunks::<8>().0) {
			*value += f64::from_le_bytes(*slot);
		}
	}
	drop(lane);
	agree(
		env,
		if total.iter().all(|v| v.is_finite()) {
			Ok(total)
		} else {
			Err(invalid())
		},
	)
}
impl<'env> PreparedHistoryInverse<'env, '_, '_> {
	/// Simulate joint inverse/time postselection in place, then reduce a typed physical diagonal.
	///
	/// First projects ALL inverse non-system bits to zero without renormalizing; only
	/// then may temporal flag/color gates reuse them. The register ends in the temporal
	/// success sector, possibly exactly zero. No quantum shots are executed. Caller
	/// provenance must bind the supplied physical chart/grid to the inverse coordinates.
	/// # Errors
	/// Recoverably rejects common metadata, width/deployment, range, callback, zero-input
	/// norm and budgets before mutation. Any error or panic AFTER the first projector
	/// aborts the MPI job; partial projected states are never returned as recoverable errors.
	#[allow(
		clippy::too_many_lines,
		reason = "A single preflight/commit boundary keeps all projection and collective ordering auditable"
	)]
	pub fn observe_temporal_postselected(
		&self,
		state: &mut CollectiveRegister<'env, '_, '_>,
		encoding: &TemporalEncoding,
		observable: &GridPhysicalObservable<'_>,
		limits: TemporalReadoutLimits,
	) -> Result<TemporalObservation, CfdError> {
		let env = self.environment;
		self.admit_register(state)?;
		let descriptor = agree(env, encoding.descriptor().map_err(CfdError::from))?;
		let source = encoding.resources();
		let width = self.count.get();
		let local = state.deployment().local_amplitudes();
		let parts = usize::try_from(env.size().map_err(native)?).map_err(|_| invalid())?;
		let chunk = limits.chunk_amplitudes.min(local);
		let system = self
			.header
			.system_dimension()?
			.checked_sub(1)
			.and_then(|n| n.checked_mul(2))
			.ok_or_else(invalid)?;
		let workspace = (self.count.dimension() - 1) & !system;
		let common_words = agree(
			env,
			(|| {
				Ok([
					word(width)?,
					word(chunk)?,
					word(limits.max_bytes)?,
					word(limits.max_work)?,
					word(limits.max_transport_bytes)?,
					word(limits.max_temporal_gates)?,
					limits.source_identity,
					self.report.source_identity,
					self.report.physical_rescaling.to_bits(),
					word(observable.physical_preparation_resources().prepare_work)?,
					word(observable.preparation_work())?,
					descriptor.source_identity,
					descriptor.construction_identity,
					word(descriptor.layout.num_qubits)?,
					descriptor.normalization.to_bits(),
					encoding.physical_time().to_bits(),
					word(encoding.slab())?,
					match encoding.side() {
						TemporalNodeSide::SlabLeft => 0,
						TemporalNodeSide::Interior => 1,
						TemporalNodeSide::SlabRight => 2,
					},
					word(observable.configuration_dimension())?,
					word(observable.retained_bytes())?,
					word(observable.query_bytes())?,
					word(observable.query_work())?,
					observable.range().lower().to_bits(),
					observable.range().upper().to_bits(),
				])
			})(),
		)?;
		common(env, &common_words)?;
		common(env, &encoding.history_semantics())?;
		agree(
			env,
			if chunk == 0
				|| limits.source_identity == 0
				|| state.deployment().is_gpu_accelerated()
				|| encoding.history_semantics() != self.history_semantics
				|| source.history_dimension != self.header.rows
				|| source.configuration_dimension != observable.configuration_dimension()
				|| descriptor.layout.num_qubits > width
				|| descriptor.layout.system_mask != system
				|| descriptor.layout.clean_workspace_mask != descriptor.layout.workspace_mask
				|| descriptor.layout.clean_workspace_value != 0
				|| descriptor.rows != source.configuration_dimension
				|| descriptor.cols != self.header.rows
				|| source.replay_gates > limits.max_temporal_gates
			{
				Err(invalid())
			} else {
				descriptor.validate().map_err(CfdError::from)
			},
		)?;
		let request = ProbabilityReadoutRequest {
			range: observable.range(),
			projection: HistoryProjection::CoefficientProjection,
			source_identity: limits.source_identity,
			limits: ProbabilityReadoutLimits {
				chunk_amplitudes: chunk,
				max_work: limits.max_work,
				max_transport_bytes: limits.max_transport_bytes,
				max_bytes: limits.max_bytes,
				callback_retained_bytes: observable.retained_bytes(),
				callback_query_bytes: observable.query_bytes(),
				callback_query_work: observable.query_work(),
			},
		};
		let select = |index| {
			if index < source.configuration_dimension {
				Ok((true, observable.value(index)?))
			} else {
				Ok((false, 0.))
			}
		};
		let (mut receipt, own_extra) = agree(
			env,
			(|| {
				let global = times(local, parts)?;
				let chunks = local.div_ceil(chunk);
				let inverse_projections =
					usize::try_from(workspace.count_ones()).map_err(|_| invalid())?;
				let temporal_projections =
					usize::try_from(descriptor.layout.workspace_mask.count_ones())
						.map_err(|_| invalid())?;
				let projections = plus(inverse_projections, temporal_projections)?;
				let dispatches = plus(source.replay_gates, plus(projections, 1)?)?;
				let control = times(
					times(parts, parts)?,
					plus(
						times(dispatches, 4096)?,
						plus(times(chunks, 4096)?, 65_536)?,
					)?,
				)?;
				let native_transport =
					times(times(times(source.replay_gates, global)?, parts)?, 32)?;
				let readout_transport = times(
					2,
					times(times(parts, parts)?, plus(8192, times(chunks, 2048)?)?)?,
				)?;
				let callback_queries = times(self.header.rows, 4)?;
				let work = plus(
					plus(
						times(times(source.preparation_work, parts)?, 2)?,
						times(times(dispatches, global)?, 128)?,
					)?,
					plus(
						plus(
							times(times(global, 5)?, 128)?,
							times(callback_queries, observable.query_work())?,
						)?,
						plus(control, readout_transport)?,
					)?,
				)?;
				let extra = plus(
					plus(source.retained_bytes, observable.retained_bytes())?,
					plus(times(chunk, 32)?, 8192)?,
				)?;
				// One reducer's reservation coexists with this retained source/executor phase.
				let reducer_extra = plus(
					plus(observable.retained_bytes(), observable.query_bytes())?,
					plus(
						times(chunk, 32)?,
						4096 + size_of::<super::ProbabilityObservation>(),
					)?,
				)?;
				let overlap = plus(extra, reducer_extra)?;
				if overlap > limits.max_bytes
					|| work > limits.max_work
					|| plus(plus(control, native_transport)?, readout_transport)?
						> limits.max_transport_bytes
				{
					return Err(invalid());
				}
				Ok((
					TemporalReadoutResources {
						temporal_gates: source.replay_gates,
						inverse_workspace_projections: inverse_projections,
						temporal_workspace_projections: temporal_projections,
						native_gate_dispatches: source.replay_gates,
						native_projection_dispatches: projections,
						native_probability_dispatches: 1,
						amplitude_scan_passes: 5,
						callback_query_bound: callback_queries,
						incremental_global_work_bound: work,
						native_amplitude_transport_byte_bound: native_transport,
						collective_control_transport_byte_bound: control,
						readout_transport_byte_bound: readout_transport,
						local_extra_bytes: overlap,
						maximum_rank_envelope_bytes: 0,
						maximum_node_envelope_bytes: 0,
						ranks_per_node: if self.limits.ranks_per_node == usize::MAX {
							parts
						} else {
							self.limits.ranks_per_node
						},
						previous_encoding_preparation_per_rank: source.preparation_work,
						previous_encoding_preparation_aggregate: times(
							source.preparation_work,
							parts,
						)?,
						previous_observable_preparation_per_rank: plus(
							observable.physical_preparation_resources().prepare_work,
							observable.preparation_work(),
						)?,
						previous_observable_preparation_aggregate: times(
							plus(
								observable.physical_preparation_resources().prepare_work,
								observable.preparation_work(),
							)?,
							parts,
						)?,
						replay_source_visits: 2,
						readout: ProbabilityReadoutResources {
							amplitude_passes: 2,
							callback_queries_bound: 0,
							work_bound: 0,
							transport_byte_bound: 0,
							local_extra_bytes: 0,
							maximum_rank_envelope_bytes: 0,
						},
					},
					extra,
				))
			})(),
		)?;
		receipt.maximum_rank_envelope_bytes = capacity(
			env,
			plus(self.external_bytes, receipt.local_extra_bytes),
			&self.limits,
		)?;
		receipt.maximum_node_envelope_bytes = agree(
			env,
			times(receipt.maximum_rank_envelope_bytes, receipt.ranks_per_node),
		)?;
		let _owner = env.reserve_external_bytes(own_extra).map_err(native)?;
		let mut executor = CollectiveReplayGateExecutor::new(state).map_err(native)?;
		let mut gates = 0usize;
		agree(
			env,
			encoding
				.visit_replay(false, &mut |gate| {
					validate_gate(gate, width)?;
					gates = gates
						.checked_add(1)
						.ok_or(quest_qsvt::Error::Budget("temporal gate count"))?;
					Ok(())
				})
				.map_err(CfdError::from),
		)?;
		agree(
			env,
			if gates == source.replay_gates {
				Ok(())
			} else {
				Err(invalid())
			},
		)?;
		// This immutable two-pass read validates EVERY configuration value, even when
		// its amplitude is zero. Its live reservation also preflights post-commit replay.
		let original = self.reduce_probability_observable(state, request, select)?;
		receipt.readout = original.resources;
		let [workspace_failure, padding] =
			original_branches(env, state, workspace, self.header.rows, chunk)?;
		let physical_scale = self.report.physical_rescaling * source.normalization;
		let maximum = observable
			.range()
			.lower()
			.abs()
			.max(observable.range().upper().abs());
		agree(
			env,
			if [
				physical_scale,
				original.total_probability_mass * maximum * maximum * 4.,
				original.total_probability_mass * physical_scale * physical_scale,
				original.total_probability_mass
					* physical_scale
					* physical_scale
					* observable
						.range()
						.lower()
						.abs()
						.max(observable.range().upper().abs()),
			]
			.iter()
			.all(|x| x.is_finite())
			{
				Ok(())
			} else {
				Err(invalid())
			},
		)?;
		Ok(fatal(|| {
			for bit in 0..width {
				if workspace & (1usize << bit) != 0 {
					state.project(bit, Outcome::Zero).map_err(native)?;
				}
			}
			encoding.visit_replay(false, &mut |gate| {
				executor.apply(state, gate).map_err(|_| {
					quest_qsvt::Error::Encoding("temporal native replay after projection")
				})
			})?;
			for bit in 0..descriptor.layout.num_qubits {
				if descriptor.layout.workspace_mask & (1usize << bit) != 0 {
					state.project(bit, Outcome::Zero).map_err(native)?;
				}
			}
			let projected = state.total_probability().map_err(native)?;
			if !projected.is_finite() || projected < 0. {
				return Err(invalid());
			}
			let (mass, first, second, expectation, variance) = if projected == 0. {
				(0., 0., 0., None, None)
			} else {
				let after = self.reduce_probability_observable(state, request, select)?;
				(
					after.selected_probability_mass,
					after.weighted_first_moment,
					after.weighted_second_moment,
					after.conditional_expectation,
					after.conditional_variance,
				)
			};
			let inverse_mass =
				original.inverse_success_probability * original.total_probability_mass;
			let scaled_norm = mass * physical_scale * physical_scale;
			let scaled_first = first * physical_scale * physical_scale;
			if !scaled_norm.is_finite() || !scaled_first.is_finite() {
				return Err(invalid());
			}
			Ok(TemporalObservation {
				original_total_mass: original.total_probability_mass,
				original_inverse_logical_success_mass: inverse_mass,
				original_inverse_logical_success_probability: original.inverse_success_probability,
				original_workspace_failure_mass: workspace_failure,
				original_clean_padding_mass: padding,
				projected_temporal_total_mass: projected,
				joint_temporal_success_mass: mass,
				joint_temporal_success_probability: mass / original.total_probability_mass,
				temporal_success_given_inverse_logical: (inverse_mass > 0.)
					.then(|| mass / inverse_mass),
				weighted_first_moment: first,
				weighted_second_moment: second,
				conditional_expectation: expectation,
				conditional_variance: variance,
				physical_amplitude_scale: physical_scale,
				physical_selected_squared_norm: scaled_norm,
				physical_scaled_quadratic_functional: scaled_first,
				physical_time: encoding.physical_time(),
				temporal_slab: encoding.slab(),
				temporal_side: encoding.side(),
				parent_history_source_identity: self.report.source_identity,
				temporal_source_identity: descriptor.source_identity,
				temporal_construction_identity: descriptor.construction_identity,
				source_identity: limits.source_identity,
				exact_zero_projected_branch: projected == 0.,
				quantum_measurements_executed: false,
				resources: receipt,
			})
		}))
	}
}
#[cfg(test)]
mod tests {
	use super::*;
	use std::{
		io::Write,
		path::{Path, PathBuf},
	};

	const CHILD_DIRECTORY: &str = "QUEST_TEMPORAL_FATAL_DIRECTORY";
	const TEST_NAME: &str = "distributed_history::temporal_observation::tests::mutation_boundary_aborts_on_error_and_panic";

	struct WitnessDirectory(PathBuf);
	impl WitnessDirectory {
		fn create(mode: &str) -> std::io::Result<Self> {
			// TMPDIR must be on a shared filesystem for a multi-node Slurm launch.
			let stamp = std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.map_err(std::io::Error::other)?
				.as_nanos();
			let path = std::env::temp_dir().join(format!(
				"quest-temporal-fatal-{}-{stamp}-{mode}",
				std::process::id()
			));
			std::fs::create_dir(&path)?;
			Ok(Self(path))
		}
	}
	impl Drop for WitnessDirectory {
		fn drop(&mut self) {
			let _ = std::fs::remove_dir_all(&self.0);
		}
	}
	fn witness(directory: &Path, name: &str, contents: &[u8]) -> Result<(), CfdError> {
		let write = || -> std::io::Result<()> {
			let mut file = std::fs::OpenOptions::new()
				.write(true)
				.create_new(true)
				.open(directory.join(name))?;
			file.write_all(contents)?;
			file.sync_all()
		};
		write().map_err(|_| CfdError::InvalidInput("could not persist temporal fatal witness"))
	}

	#[test]
	#[allow(
		clippy::panic_in_result_fn,
		clippy::panic,
		reason = "Bounded subprocess regression deliberately triggers fatal post-projection error/panic boundaries"
	)]
	fn mutation_boundary_aborts_on_error_and_panic() -> Result<(), Box<dyn std::error::Error>> {
		let Ok(mode) = std::env::var("QUEST_TEMPORAL_FATAL_CHILD") else {
			for mode in ["error", "panic"] {
				let directory = WitnessDirectory::create(mode)?;
				let output =
					quest_test_support::mpi::MpiTest::new(2, std::time::Duration::from_secs(20))?
						.args(["--exact", TEST_NAME, "--nocapture", "--test-threads=1"])
						.env("QUEST_TEMPORAL_FATAL_CHILD", mode)
						.env(CHILD_DIRECTORY, &directory.0)
						.output()?;
				eprintln!(
					"temporal fatal mode={mode}: stdout={} stderr={}",
					String::from_utf8_lossy(&output.stdout),
					String::from_utf8_lossy(&output.stderr)
				);
				assert!(!output.status.success());
				assert!(
					!output.status.timed_out,
					"fatal boundary must abort, not hang"
				);
				for rank in 0..2 {
					assert_eq!(
						std::fs::read(directory.0.join(format!("projection-completed-{rank}")))?,
						b"native projection completed\n"
					);
					assert!(
						!directory
							.0
							.join(format!("after-fatal-return-{rank}"))
							.exists()
					);
				}
				assert_eq!(
					std::fs::read(directory.0.join("peer-ready"))?,
					b"unmatched receive stage ready\n"
				);
				assert_eq!(
					std::fs::read(directory.0.join("triggered"))?,
					mode.as_bytes()
				);
			}
			return Ok(());
		};
		assert!(matches!(mode.as_str(), "error" | "panic"));
		let directory = std::env::var_os(CHILD_DIRECTORY)
			.map(PathBuf::from)
			.ok_or("missing temporal fatal witness directory")?;
		let runtime = quest::collective::MpiRuntime::initialize()?;
		let comm = runtime.world()?;
		quest_test_support::mpi::assert_rank_count(comm.size()?)?;
		let env = CollectiveEnvironment::builder(&comm)?
			.memory_budget(quest::MemoryBudget::new(1_048_576))
			.build()?;
		let mut state = env.state_vector_local(quest::QubitCount::new(2)?)?;
		state.init_zero()?;
		let rank = comm.rank()?;
		fatal(|| {
			state.project(0, Outcome::Zero).map_err(native)?;
			witness(
				&directory,
				&format!("projection-completed-{rank}"),
				b"native projection completed\n",
			)?;
			if rank == 1 {
				witness(&directory, "peer-ready", b"unmatched receive stage ready\n")?;
			}
			// Acquire the lane after project(), which owns its own collective lane.
			let mut lane = comm.collective_lane().map_err(native)?;
			// Both ranks have mutated native state and persisted their evidence before the trigger.
			if !lane.all_agree(true).map_err(native)? {
				return Err(invalid());
			}
			if rank == 0 {
				witness(&directory, "triggered", mode.as_bytes())?;
				assert!(mode != "panic", "injected post-projection panic");
				return Err(invalid());
			}
			let mut pending = [0u8; 1];
			lane.receive_bytes(&mut pending, 0, 3051).map_err(native)?;
			Ok(())
		});
		witness(
			&directory,
			&format!("after-fatal-return-{rank}"),
			b"fatal boundary returned\n",
		)?;
		Err("fatal boundary unexpectedly returned".into())
	}
}
