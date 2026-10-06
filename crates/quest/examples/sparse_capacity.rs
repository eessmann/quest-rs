//! Bounded Linux/MPI experiment: source-owned COO -> persistence -> native matching.
#![forbid(unsafe_code)]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::cast_precision_loss,
	reason = "Checked dimension admission precedes bounded rank, stage and native indexing arithmetic"
)]
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#[path = "sparse_capacity/admission.rs"]
mod admission;
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#[path = "sparse_capacity/support.rs"]
mod support;
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
mod experiment {
	use super::admission::{Arguments, Mode, ProcessLimits};
	use quest::{
		Complex64, MemoryBudget, QubitCount,
		collective::{CollectiveEnvironment, MpiRuntime},
		qsvt::{
			matching::{
				collective::RoutingCapacity,
				preprocess::{ProducerLimits, produce_matching},
			},
			persisted_matching::{PersistenceLimits, load_matching_resource, publish_produced},
		},
	};
	use quest_qsvt::{NumericalPolicy, matching_resource::ResourceReplayLimits};
	use quest_qsvt_io::sharded_matching::ShardIoLimits;
	use serde_json::{Value, json};
	use std::time::Instant;
	type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

	fn memory() -> Result<(usize, usize, usize, usize)> {
		let status = std::fs::read_to_string("/proc/self/status")?;
		let number = |prefix: &str| -> Result<usize> {
			let word = status
				.lines()
				.find_map(|line| line.strip_prefix(prefix))
				.and_then(|line| line.split_whitespace().next())
				.ok_or("Linux memory counter")?;
			Ok(word
				.parse::<usize>()?
				.checked_mul(1024)
				.ok_or("RSS overflow")?)
		};
		Ok((
			number("VmRSS:")?,
			number("VmHWM:")?,
			number("VmSize:")?,
			number("VmPeak:")?,
		))
	}
	fn node_envelope(
		comm: &quest_sys::mpi::MpiCommunicator<'_>,
		leader_rank: i32,
		rank: usize,
		parts: usize,
		envelope: usize,
	) -> Result<usize> {
		let encoded = u64::try_from(envelope);
		if !comm.collective_lane()?.all_agree(encoded.is_ok())? {
			return Err("rank envelope exceeds transport count width".into());
		}
		let encoded = encoded?;
		let mut total = Some(0usize);
		for root in 0..parts {
			let mut bytes = [0_u8; 12];
			if root == rank {
				let (leader, value) = bytes.split_at_mut(4);
				leader.copy_from_slice(&leader_rank.to_le_bytes());
				value.copy_from_slice(&encoded.to_le_bytes());
			}
			comm.collective_lane()?
				.broadcast_bytes(i32::try_from(root)?, &mut bytes)?;
			let (leader, value) = bytes.split_at(4);
			if i32::from_le_bytes(leader.try_into()?) == leader_rank {
				let incoming = usize::try_from(u64::from_le_bytes(value.try_into()?)).ok();
				total = total
					.zip(incoming)
					.and_then(|(sum, value)| sum.checked_add(value));
			}
		}
		if !comm.collective_lane()?.all_agree(total.is_some())? {
			return Err("node rank-process envelope overflow".into());
		}
		Ok(total.ok_or("node envelope")?)
	}
	fn barrier(comm: &quest_sys::mpi::MpiCommunicator<'_>) -> Result {
		if !comm.collective_lane()?.all_agree(true)? {
			return Err("stage barrier".into());
		}
		Ok(())
	}
	fn finish(comm: &quest_sys::mpi::MpiCommunicator<'_>, start: Instant) -> Result<Value> {
		barrier(comm)?;
		let seconds = start.elapsed().as_secs_f64();
		let (rss, high, address, peak_address) = memory()?;
		Ok(
			json!({"seconds":seconds,"rss_endpoint_bytes":rss,"rss_high_water_bytes":high,"address_space_endpoint_bytes":address,"address_space_high_water_bytes":peak_address}),
		)
	}
	#[allow(
		clippy::too_many_lines,
		reason = "Linear stage lifetime boundaries are explicit for peak accounting"
	)]
	pub fn run() -> Result {
		let args = Arguments::parse(std::env::args().skip(1))?;
		let directory = &args.directory;
		let (n, repetitions) = (args.dimension, args.repetitions);
		let (rank_budget, node_budget) = (args.rank_budget, args.node_budget);
		let (placement, threads) = (args.placement, args.threads);
		let (omp_stack_bytes, omp_stack_allowance) = (args.stack_bytes, args.stack_allowance);
		let observed_limits = ProcessLimits::observe()?;
		args.admit_limits(observed_limits)?;
		if args.explicit_threads
			&& (std::env::var("OMP_NUM_THREADS")? != threads.to_string()
				|| std::env::var("OMP_PLACES")? != "cores"
				|| std::env::var("OMP_PROC_BIND")? != "close"
				|| std::env::var("OMP_DYNAMIC")? != "FALSE"
				|| std::env::var("OMP_STACKSIZE")? != format!("{omp_stack_bytes}B"))
		{
			return Err("OpenMP environment differs from requested thread/stack profile".into());
		}
		let runtime = MpiRuntime::initialize()?;
		let mut comm = runtime.world()?;
		let rank = usize::try_from(comm.rank()?)?;
		let parts = usize::try_from(comm.size()?)?;
		let topology = comm.shared_memory_topology()?;
		let local_size = usize::try_from(topology.local_size)?;
		let valid = n >= parts
			&& parts.is_power_of_two()
			&& parts <= 32
			&& rank_budget
				.checked_mul(local_size)
				.is_some_and(|b| b <= node_budget)
			&& placement.is_none_or(|(nodes, per_node)| {
				nodes.checked_mul(per_node) == Some(parts) && local_size == per_node
			});
		if !comm.collective_lane()?.all_agree(valid)? {
			return Err("actual node placement envelope".into());
		}
		let mut pids = Vec::with_capacity(local_size.min(32));
		for root in 0..parts {
			let mut bytes = [0_u8; 8];
			if root == rank {
				let (leader, pid) = bytes.split_at_mut(4);
				leader.copy_from_slice(&topology.leader_rank.to_le_bytes());
				pid.copy_from_slice(&std::process::id().to_le_bytes());
			}
			comm.collective_lane()?
				.broadcast_bytes(i32::try_from(root)?, &mut bytes)?;
			let (leader, pid) = bytes.split_at(4);
			if i32::from_le_bytes(leader.try_into()?) == topology.leader_rank {
				pids.push(u32::from_le_bytes(pid.try_into()?));
			}
		}
		if pids.len() != local_size {
			return Err("shared-memory PID coverage".into());
		}
		let sampler = if placement.is_some() && topology.local_rank == 0 {
			Some(super::support::NodeSampler::start(pids)?)
		} else {
			None
		};
		let baseline = memory()?;
		let baseline_threads = super::support::process_threads()?;
		let mut stages = serde_json::Map::new();
		let rank_envelope = args.rank_envelope(baseline.2, observed_limits);
		if !comm.collective_lane()?.all_agree(rank_envelope.is_ok())? {
			return Err("MPI baseline, managed rank allowance and OpenMP stacks exceed process limit or overflow".into());
		}
		let rank_envelope = rank_envelope?;
		let node_envelope = if args.mode == Mode::Scaling {
			node_envelope(&comm, topology.leader_rank, rank, parts, rank_envelope)?
		} else {
			0
		};
		let local_entries = (rank..n)
			.step_by(parts)
			.count()
			.checked_mul(2)
			.ok_or("source count")?;
		let local_limit = n
			.div_ceil(parts)
			.checked_mul(2)
			.ok_or("local input limit")?;
		let work_limit = n.checked_mul(1_048_576).ok_or("work limit overflow")?;
		let source_buffer_bytes = local_limit
			.checked_mul(40)
			.and_then(|b| b.checked_add(65_536))
			.ok_or("source buffer bytes")?;
		let native_minimum = n
			.checked_mul(256)
			.and_then(|b| b.checked_div(parts))
			.ok_or("native state envelope")?;
		if !comm
			.collective_lane()?
			.all_agree(source_buffer_bytes <= rank_budget && native_minimum <= rank_budget)?
		{
			return Err("source/state minimum exceeds managed rank allowance".into());
		}
		let source_started = Instant::now();
		let (input, source_sha256, source_bytes) = super::support::source(
			&directory.join(format!("input-rank-{rank}.coo32")),
			n,
			rank,
			parts,
		)?;
		let source_seconds = source_started.elapsed().as_secs_f64();
		let mut producer_limits = ProducerLimits {
			max_bytes: rank_budget,
			max_local_edges: local_limit,
			max_endpoint_records: local_limit.checked_mul(2).ok_or("endpoint count")?,
			max_work: work_limit,
			max_probes: work_limit,
			max_communication_bytes: work_limit,
			..ProducerLimits::default()
		};
		producer_limits.stream.max_bytes = source_buffer_bytes;
		producer_limits.stream.buffer_entries = local_limit;
		producer_limits.stream.max_entries = local_limit;
		producer_limits.stream.max_work = work_limit;
		barrier(&comm)?;
		let start = Instant::now();
		let produced = produce_matching(&comm, n, n, input, producer_limits)?;
		let preprocessing = produced.statistics();
		stages.insert("preprocessing".into(), finish(&comm, start)?);
		let limits = PersistenceLimits {
			io: ShardIoLimits {
				chunk_records: 4096,
				max_buffer_bytes: 2 * 1024 * 1024,
				max_manifest_bytes: 262_144,
				max_buckets: parts,
				max_records: local_limit,
				max_file_bytes: u64::try_from(
					local_limit
						.div_ceil(4096)
						.checked_mul(4096 * 80 + 65_536)
						.and_then(|b| b.checked_add(65_536))
						.ok_or("HDF5 file bytes")?,
				)?,
			},
			max_bytes: rank_budget,
			max_local_records: local_limit,
			replay: ResourceReplayLimits {
				max_bytes: rank_budget,
				max_queries: work_limit,
				max_work: work_limit,
				max_communication_bytes: work_limit,
				max_gates: work_limit,
			},
			max_communication_bytes: work_limit,
		};
		let manifest = directory.join("manifest.json");
		barrier(&comm)?;
		let start = Instant::now();
		publish_produced(&comm, &produced, directory, &manifest, parts, limits)?;
		stages.insert("persistence".into(), finish(&comm, start)?);
		drop(produced);
		barrier(&comm)?;
		let start = Instant::now();
		let loaded = load_matching_resource(&comm, &manifest, directory, limits.into())?;
		let header = loaded.header();
		let retained = loaded.retained_bytes();
		let load_statistics = loaded.load_statistics();
		let policy = NumericalPolicy {
			max_bytes: rank_budget,
		};
		// Placement-aware receipts use only the resource needed by native execution.
		// Keep the legacy no-placement invocation's admitted portable replay receipt.
		let (shard, replay_bound) = if placement.is_some() {
			let shard = loaded.matching_shard(policy)?;
			drop(loaded);
			(shard, None)
		} else {
			let loaded = loaded.admit_replay(&comm, limits.replay)?;
			let bound = loaded.recipe().statistics().communication_bytes;
			let shard = loaded.matching_shard(policy)?;
			drop(loaded);
			(shard, Some(bound))
		};
		stages.insert("load".into(), finish(&comm, start)?);
		barrier(&comm)?;
		let start = Instant::now();
		let mut builder =
			CollectiveEnvironment::builder(&comm)?.memory_budget(MemoryBudget::new(rank_budget));
		if threads > 1 {
			builder = builder.with_multithreading();
		}
		let env = builder.build()?;
		let native_environment_multithreaded = env.view().capabilities().multithreaded;
		let active = 1 + header.system_qubits + header.color_qubits;
		let count = QubitCount::new(active + 1)?;
		let mut register = env.state_vector_local(count)?;
		let mut prepared = env.prepare_matching_with_capacity(
			shard,
			count,
			(0..active).collect(),
			RoutingCapacity {
				ranks_per_node: local_size,
				node_budget: MemoryBudget::new(node_budget),
			},
		)?;
		let allocated = env.view().allocated_bytes();
		let native_array_payload = super::support::native_array_payload(
			register.deployment(),
			prepared.scratch_deployment(),
		)?;
		let global_amplitudes = 1usize
			.checked_shl(u32::try_from(active + 1)?)
			.ok_or("state width")?;
		let local_amplitudes = register.deployment().local_amplitudes();
		let native_register_multithreaded = register.deployment().is_multithreaded();
		if !comm.collective_lane()?.all_agree(
			native_environment_multithreaded == (threads > 1)
				&& native_register_multithreaded == (threads > 1),
		)? {
			return Err("actual native threading deployment differs from request".into());
		}
		if local_amplitudes.checked_mul(parts) != Some(global_amplitudes) {
			return Err("native register is not distributed".into());
		}
		register.init_plus()?;
		let prepared_threads = super::support::process_threads()?;
		stages.insert("prepare".into(), finish(&comm, start)?);
		barrier(&comm)?;
		let start = Instant::now();
		let mask = 1usize << active;
		let mut sent = 0usize;
		let mut received = 0usize;
		let mut coordination = 0usize;
		let mut candidates = 0usize;
		let mut batches = 0usize;
		let mut iteration_seconds = Vec::with_capacity(repetitions);
		for repetition in 0..repetitions {
			let iteration_start = Instant::now();
			let value = if repetition % 2 == 0 { 0 } else { mask };
			for adjoint in [false, true] {
				prepared.apply(&mut register, adjoint, mask, value)?;
				let stats = prepared.last_statistics();
				sent = sent
					.checked_add(stats.point_to_point_sent_bytes)
					.ok_or("routing byte overflow")?;
				received = received
					.checked_add(stats.point_to_point_received_bytes)
					.ok_or("routing byte overflow")?;
				coordination = coordination
					.checked_add(stats.coordination_calls)
					.ok_or("coordination overflow")?;
				candidates = candidates
					.checked_add(stats.local_pair_candidates)
					.ok_or("candidate overflow")?;
				batches = batches.checked_add(stats.batches).ok_or("batch overflow")?;
			}
			iteration_seconds.push(iteration_start.elapsed().as_secs_f64());
		}
		stages.insert("execution".into(), finish(&comm, start)?);
		let norm = register.total_probability()?;
		// Fixed-size validation samples; no native partition/full state copied to Rust.
		let expected = 1.0 / 2.0_f64.powi(i32::try_from(active + 1)?).sqrt();
		let error = register
			.read_local_amplitudes(0, 8.min(local_amplitudes))?
			.into_iter()
			.map(|value| (value - Complex64::new(expected, 0.0)).norm())
			.fold(0.0, f64::max);
		if !norm.is_finite() || (norm - 1.0).abs() > 1e-10 || !error.is_finite() || error > 1e-10 {
			return Err("controlled whole-U forward/adjoint validation".into());
		}
		barrier(&comm)?;
		let node_sampling = sampler
			.map(super::support::NodeSampler::finish)
			.transpose()?;
		let (_, high, _, address_high) = memory()?;
		let final_threads = super::support::process_threads()?;
		let mut receipt = json!({
			"rank":rank,"ranks":parts,"dimension":n,"repetitions":repetitions,
			"rss_high_water_bytes":high,
			"address_space_high_water_bytes":address_high, "baseline_rss_bytes":baseline.0,
			"baseline_address_space_bytes":baseline.2,
			"model_rank_budget_bytes":rank_budget,"model_node_budget_bytes":node_budget,
			// Conservative admitted stage envelope, not a measured allocation peak.
			"model_peak_bytes":rank_budget,"model_peak_kind":"conservative admitted stage envelope, not measured peak","producer_managed_peak_upper_bound_bytes":preprocessing.peak_managed_bytes,
			"loaded_retained_upper_bound_bytes":retained,"native_live_reserved_bytes":allocated,
			"local_input_entries":local_entries,"global_input_entries":2*n,
			"local_native_amplitudes":local_amplitudes,"global_native_amplitudes":global_amplitudes,
			"producer_sent_payload_bytes":preprocessing.sent_bytes,
			"producer_global_edges":preprocessing.global_edges,"colors":header.num_colors,
			"persisted_recipe_single_replay_communication_upper_bound_bytes":replay_bound,
			"persistence_load_measured_communication": if placement.is_some() {
				"logical broadcast calls only; wire bytes and MPI-internal communication unmeasured"
			} else { "not instrumented; recipe replay bound is not a load measurement" },
			"execution_sent_bytes":sent,"execution_received_bytes":received,
			"execution_coordination_calls":coordination,"execution_local_pair_candidates":candidates,
			"execution_batches":batches,"execution_roundtrip_seconds":iteration_seconds,"norm":norm,"maximum_sample_error":error,"stages":stages
		});
		if placement.is_some() {
			let persisted_bytes =
				std::fs::read_dir(directory)?.try_fold(0_u64, |sum, entry| -> Result<u64> {
					let entry = entry?;
					let name = entry.file_name();
					let name = name.to_string_lossy();
					if rank == 0 && name.ends_with(".h5") {
						Ok(sum
							.checked_add(entry.metadata()?.len())
							.ok_or("persisted bytes")?)
					} else {
						Ok(sum)
					}
				})?;
			let object = receipt.as_object_mut().ok_or("receipt object")?;
			object.insert("schema_version".into(), json!(5));
			object.insert("loading_route".into(), json!("resource-only-native"));
			object.insert("portable_replay_admission".into(), json!("unrun"));
			object.insert(
				"load_statistics".into(),
				super::support::load_statistics(load_statistics),
			);
			object.insert("native_array_payload".into(), native_array_payload);
			if args.explicit_threads {
				object.insert("threading".into(), json!({
					"requested_threads":threads,"omp_stack_bytes_per_worker":omp_stack_bytes,
					"omp_stack_allowance_bytes":omp_stack_allowance,
					"native_environment_multithreaded":native_environment_multithreaded,
					"native_register_multithreaded":native_register_multithreaded,
					"baseline_process_threads":baseline_threads,"prepared_process_threads":prepared_threads,"final_process_threads":final_threads,
					"omp_num_threads":std::env::var("OMP_NUM_THREADS")?,"omp_places":std::env::var("OMP_PLACES")?,
					"omp_proc_bind":std::env::var("OMP_PROC_BIND")?,"omp_dynamic":std::env::var("OMP_DYNAMIC")?,
					"omp_stacksize":std::env::var("OMP_STACKSIZE")?,"native_openmp_team_size":null,
					"scope":"native QuEST operations may use OpenMP; source generation, producer, loading, matching pair arithmetic and MPI routing remain serial"
				}));
			}
			object.insert("node".into(), json!({"leader_rank":topology.leader_rank,"local_rank":topology.local_rank,"local_size":topology.local_size,"processor_name":topology.processor_name}));
			object.insert("node_memory_sampling".into(), json!(node_sampling));
			object.insert(
				"canonical_input_bytes".into(),
				json!(n.checked_mul(64).ok_or("canonical bytes")?),
			);
			object.insert("local_canonical_input_bytes".into(), json!(source_bytes));
			object.insert("local_input_sha256".into(), json!(source_sha256));
			object.insert("input_storage_seconds".into(), json!(source_seconds));
			object.insert(
				"persisted_encoding_file_bytes".into(),
				json!(persisted_bytes),
			);
		}
		let final_limits = ProcessLimits::observe()?;
		let object = receipt.as_object_mut().ok_or("receipt object")?;
		if let Some(evidence) =
			args.scaling_evidence(observed_limits, final_limits, rank_envelope, node_envelope)
		{
			object.extend(
				evidence
					.as_object()
					.ok_or("scaling evidence object")?
					.clone(),
			);
		} else {
			object.insert(
				"process_address_space_cap_bytes".into(),
				json!(final_limits.equal_finite()?),
			);
		}
		std::fs::write(
			directory.join(format!("rank-{rank}.json.tmp")),
			serde_json::to_vec_pretty(&receipt)?,
		)?;
		std::fs::rename(
			directory.join(format!("rank-{rank}.json.tmp")),
			directory.join(format!("rank-{rank}.json")),
		)?;
		barrier(&comm)?;
		Ok(())
	}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
	return experiment::run();
	#[cfg(not(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi)))]
	Err("sparse_capacity requires qsvt-io, mpi and a native MPI QuEST build".into())
}
