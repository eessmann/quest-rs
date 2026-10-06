//! Reload immutable resources and optionally compare native execution paths.
#![allow(
	clippy::arithmetic_side_effects,
	reason = "Bounded numerical probe compares every local state coordinate"
)]
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
mod experiment {
	use quest::{
		Complex64, MemoryBudget, QubitCount,
		collective::{CollectiveEnvironment, MpiRuntime},
		qsvt::persisted_matching::{
			LoadStatistics, LoadedMatching, LoadedMatchingResource, PersistedPreparationLimits,
			PersistenceLimits, load_matching, load_matching_resource,
		},
	};
	use quest_qsvt::{MatchingHeader, matching_resource::ResourceReplayLimits};
	use serde_json::{Value, json};
	use sha2::{Digest, Sha256};
	use std::fmt::Write as _;
	use std::{fs::OpenOptions, io::Write, path::PathBuf, time::Instant};
	type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
	#[allow(
		clippy::large_enum_variant,
		reason = "The probe holds one bounded owner on its stack and adds no heap allocation"
	)]
	enum Loaded {
		Legacy(LoadedMatching),
		Resource(LoadedMatchingResource),
	}
	impl Loaded {
		fn details(&self) -> (LoadStatistics, MatchingHeader, usize) {
			match self {
				Self::Legacy(x) => (x.load_statistics(), x.header(), x.retained_bytes()),
				Self::Resource(x) => (x.load_statistics(), x.header(), x.retained_bytes()),
			}
		}
	}
	fn native(
		loaded: Loaded,
		comm: &quest_sys::mpi::MpiCommunicator<'_>,
		max_bytes: usize,
		work: usize,
	) -> Result<Value> {
		let count = QubitCount::new(loaded.details().1.num_qubits()?)?;
		let targets = (0..count.get()).collect();
		let threads = std::env::var("OMP_NUM_THREADS")
			.unwrap_or_else(|_| "1".into())
			.parse::<usize>()?;
		let started = Instant::now();
		let mut builder =
			CollectiveEnvironment::builder(comm)?.memory_budget(MemoryBudget::new(max_bytes));
		if threads > 1 {
			builder = builder.with_multithreading();
		}
		let environment = builder.build()?;
		let environment_seconds = started.elapsed().as_secs_f64();
		let started = Instant::now();
		let limits = PersistedPreparationLimits {
			max_bytes,
			max_constructor_work: work,
			max_application_payload_bytes: work,
			policy: quest_qsvt::NumericalPolicy { max_bytes },
			..PersistedPreparationLimits::default()
		};
		let (mut prepared, receipt) = match loaded {
			Loaded::Legacy(x) => x.into_prepared_matching(&environment, count, targets, limits)?,
			Loaded::Resource(x) => {
				x.into_prepared_matching(&environment, count, targets, limits)?
			}
		};
		let preparation_seconds = started.elapsed().as_secs_f64();
		let mut register = environment.state_vector_local(count)?;
		register.init_plus()?;
		let started = Instant::now();
		prepared.apply(&mut register, false, 0, 0)?;
		let forward_seconds = started.elapsed().as_secs_f64();
		let local = register.deployment().local_amplitudes();
		let amplitudes = register.read_local_amplitudes(0, local)?;
		let mut forward_hash = Sha256::new();
		for z in &amplitudes {
			forward_hash.update(z.re.to_le_bytes());
			forward_hash.update(z.im.to_le_bytes());
		}
		let mut forward_sha256 = String::with_capacity(64);
		for byte in forward_hash.finalize() {
			write!(forward_sha256, "{byte:02x}")?;
		}
		drop(amplitudes);
		let started = Instant::now();
		prepared.apply(&mut register, true, 0, 0)?;
		let adjoint_seconds = started.elapsed().as_secs_f64();
		let norm = register.total_probability()?;
		let expected = Complex64::new(2_f64.powi(i32::try_from(count.get())?).sqrt().recip(), 0.0);
		let maximum_error = register
			.read_local_amplitudes(0, local)?
			.into_iter()
			.map(|z| (z - expected).norm())
			.fold(0.0, f64::max);
		if !norm.is_finite()
			|| (norm - 1.0).abs() > 1e-10
			|| !maximum_error.is_finite()
			|| maximum_error > 1e-10
		{
			return Err("native all-coordinate roundtrip tolerance".into());
		}
		Ok(
			json!({"environment_seconds":environment_seconds,"preparation_seconds":preparation_seconds,
			"forward_seconds":forward_seconds,"adjoint_seconds":adjoint_seconds,"norm":norm,
			"all_local_coordinates_checked":local,"maximum_error":maximum_error,"forward_sha256":forward_sha256,
			"requested_openmp_threads":threads,"native_multithreaded":environment.view().capabilities().multithreaded,
			"manifest_sha256":receipt.manifest_sha256,"source_identity":receipt.native_source_identity,
			"construction_identity":receipt.native_construction_identity}),
		)
	}
	#[allow(
		clippy::too_many_lines,
		reason = "One explicit diagnostic receipt lists timing and logical-call scopes"
	)]
	pub fn run() -> Result {
		let mut args = std::env::args().skip(1);
		let directory = PathBuf::from(args.next().ok_or("input resource directory")?);
		let output = PathBuf::from(args.next().ok_or("existing output directory")?);
		let max_bytes = args
			.next()
			.ok_or("managed payload budget in bytes")?
			.parse::<usize>()?;
		let work = args
			.next()
			.ok_or("query/work/wire/gate limit")?
			.parse::<usize>()?;
		// Omitting the mode preserves the original reload-only diagnostic.
		let mode = args.next();
		if args.next().is_some()
			|| max_bytes == 0
			|| work == 0
			|| mode
				.as_deref()
				.is_some_and(|m| !matches!(m, "legacy" | "resource"))
		{
			return Err(
				"usage: matching_load_probe INPUT OUTPUT MAX_BYTES WORK_LIMIT [legacy|resource]"
					.into(),
			);
		}
		let limits = PersistenceLimits {
			max_bytes,
			replay: ResourceReplayLimits {
				max_bytes,
				max_queries: work,
				max_work: work,
				max_communication_bytes: work,
				max_gates: work,
			},
			max_communication_bytes: work,
			..PersistenceLimits::default()
		};
		let runtime = MpiRuntime::initialize()?;
		let mut comm = runtime.world()?;
		let topology = comm.shared_memory_topology()?;
		let rank = comm.rank()?;
		let parts = comm.size()?;
		if !comm.collective_lane()?.all_agree(true)? {
			return Err("load start agreement".into());
		}
		let path = directory.join("manifest.json");
		let loaded = if mode.as_deref() == Some("resource") {
			Loaded::Resource(load_matching_resource(
				&comm,
				path,
				&directory,
				limits.into(),
			)?)
		} else {
			Loaded::Legacy(load_matching(&comm, path, &directory, limits)?)
		};
		let (stats, header, retained) = loaded.details();
		let native = if mode.is_some() {
			Some(native(loaded, &comm, max_bytes, work)?)
		} else {
			None
		};
		let result = json!({
			"schema_version":2,"mode":mode.unwrap_or_else(||"legacy-load-only".into()),
			"rank":rank,"ranks":parts,"processor_name":topology.processor_name,
			"shared_memory_local_size":topology.local_size,"dimension":header.rows,"records":header.record_count,
			"source_identity":header.source_identity,"record_digest":header.record_digest,
			"managed_payload_budget_bytes":max_bytes,"query_work_wire_gate_limit":work,
			"retained_payload_bound_bytes":retained,"local_records":stats.local_records,
			"local_reverse_records":stats.local_reverse_records,"local_record_capacity":stats.local_record_capacity,
			"local_reverse_capacity":stats.local_reverse_capacity,"replay_admitted":stats.replay_admitted,
			"seconds": {"manifest_and_admission":stats.manifest_and_admission.as_secs_f64(),
				"read_validate":stats.read_validate.as_secs_f64(),"reverse_directory":stats.reverse_directory.as_secs_f64(),
				"replay_admission":stats.replay_admission.as_secs_f64(),"total":stats.total.as_secs_f64()},
			"reverse_broadcasts":stats.reverse_broadcasts,"admission":{
				"next_record_calls":stats.admission.next_record_calls,"forward_calls":stats.admission.forward_calls,
				"reverse_calls":stats.admission.reverse_calls,"broadcasts":stats.admission.broadcasts},"native":native,
		});
		let mut file = OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(output.join(format!("rank-{rank}.json")))?;
		serde_json::to_writer_pretty(&mut file, &result)?;
		file.write_all(b"\n")?;
		file.sync_all()?;
		if !comm.collective_lane()?.all_agree(true)? {
			return Err("load completion agreement".into());
		}
		Ok(())
	}
}
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
fn main() {
	if let Err(error) = experiment::run() {
		eprintln!("matching load probe failed: {error}");
		quest_sys::mpi::abort_job();
	}
}
#[cfg(not(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi)))]
fn main() {
	eprintln!("matching_load_probe requires qsvt-io, mpi and native MPI support");
	std::process::exit(1);
}
