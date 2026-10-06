//! Real bounded overcapacity, isolated to subprocesses; no production fault interface.
use super::super::*;
use crate::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
};
use std::sync::atomic::{AtomicUsize, Ordering};
static MANIFESTS: AtomicUsize = AtomicUsize::new(0);
static BUCKETS: AtomicUsize = AtomicUsize::new(0);
static ROUTING: AtomicUsize = AtomicUsize::new(0);
pub(in crate::qsvt::persisted_matching) fn manifest() {
	MANIFESTS.fetch_add(1, Ordering::Relaxed);
}
pub(in crate::qsvt::persisted_matching) fn bucket() {
	BUCKETS.fetch_add(1, Ordering::Relaxed);
}
pub(in crate::qsvt::persisted_matching) fn routing() {
	ROUTING.fetch_add(1, Ordering::Relaxed);
}
pub(in crate::qsvt::persisted_matching) fn inflate<T>(
	values: &mut Vec<T>,
	rank: usize,
	stage: &str,
) -> Result<()> {
	if rank == 0 && std::env::var("QUEST_LOADER_FAULT_CHILD").as_deref() == Ok(stage) {
		values
			.try_reserve_exact(8192)
			.map_err(|_| Error::Allocation)?;
	}
	Ok(())
}
#[test]
#[allow(
	clippy::too_many_lines,
	clippy::panic_in_result_fn,
	clippy::arithmetic_side_effects,
	reason = "Bounded MPI real-capacity faults assert exact stage ordering, quantum and prior-child ownership preservation"
)]
fn actual_loader_capacity_rejects_before_next_stage()
-> std::result::Result<(), Box<dyn std::error::Error>> {
	if std::env::var("QUEST_LOADER_FAULT_CHILD").is_err() {
		let root =
			std::env::temp_dir().join(format!("quest-loader-capacity-{}", std::process::id()));
		std::fs::create_dir(&root)?;
		for parts in [1, 2] {
			for mode in [
				"records",
				"reverse",
				"premanifest-budget",
				"premanifest-work",
				"limits",
			] {
				if mode == "limits" && parts == 1 {
					continue;
				}
				let dir = root.join(format!("{parts}-{mode}"));
				std::fs::create_dir(&dir)?;
				let output=quest_test_support::mpi::MpiTest::new(usize::try_from(parts)?, std::time::Duration::from_secs(30))?.args(["--exact","qsvt::persisted_matching::loading::capacity_tests::actual_loader_capacity_rejects_before_next_stage","--nocapture","--test-threads=1"]).env("QUEST_LOADER_FAULT_CHILD",mode).env("QUEST_LOADER_DIRECTORY",dir).output()?;
				assert!(
					output.status.success(),
					"{parts}/{mode}: {} {}",
					String::from_utf8_lossy(&output.stdout),
					String::from_utf8_lossy(&output.stderr)
				);
			}
		}
		std::fs::remove_dir_all(root)?;
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = world.duplicate()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let directory = std::path::PathBuf::from(std::env::var("QUEST_LOADER_DIRECTORY")?);
	let path = directory.join("manifest.json");
	let produced = crate::qsvt::matching::preprocess::produce_matching(
		&comm,
		2,
		2,
		(0..2).filter(|i| i % parts == rank).map(|i| {
			Ok(quest_numerics::sparse_stream::SparseEntry {
				row: i,
				column: i,
				ordinal: u64::try_from(i).unwrap_or(u64::MAX),
				value: Complex64::new(0., 1.),
			})
		}),
		crate::qsvt::matching::preprocess::ProducerLimits::default(),
	)?;
	let limits = PersistenceLimits {
		io: ShardIoLimits {
			chunk_records: 1,
			max_buffer_bytes: 16384,
			max_manifest_bytes: 65536,
			max_buckets: 2,
			max_records: 2,
			max_file_bytes: 1_048_576,
		},
		max_local_records: 2,
		max_bytes: 131_072,
		..Default::default()
	};
	publish_produced(
		&comm,
		&produced,
		&directory,
		&path,
		2,
		PersistenceLimits {
			max_bytes: PersistenceLimits::default().max_bytes,
			..limits
		},
	)?;
	let shard = MatchingShard::from_parts(
		produced.shard().header(),
		rank,
		parts,
		produced.shard().records().to_vec(),
		NumericalPolicy::default(),
	)?;
	drop(produced);
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(1_048_576))
		.build()?;
	let mut child = environment.prepare_matching(shard, QubitCount::new(2)?, vec![0, 1])?;
	let mut register = environment.state_vector_local(QubitCount::new(2)?)?;
	register.init_plus()?;
	let initial = register.read_local_amplitudes(0, register.deployment().local_amplitudes())?;
	let guard = environment.reserve_external_bytes(limits.max_bytes)?;
	let baseline = environment.view().allocated_bytes();
	let mode = std::env::var("QUEST_LOADER_FAULT_CHILD")?;
	let mut fault_limits = limits;
	if rank == 0 {
		match mode.as_str() {
			"premanifest-budget" => fault_limits.max_bytes = 1,
			"premanifest-work" => fault_limits.replay.max_work = 1,
			"limits" => fault_limits.io.max_records = 1,
			_ => {}
		}
	}
	let load_path = if mode.starts_with("premanifest") {
		directory.join("absent-manifest.json")
	} else {
		path
	};
	assert!(
		load_matching(&comm, &load_path, &directory, fault_limits).is_err(),
		"actual capacity exceeds loader envelope despite requested count fitting"
	);
	if mode.starts_with("premanifest") || mode == "limits" {
		assert_eq!(MANIFESTS.load(Ordering::Relaxed), 0);
	} else if mode == "records" {
		assert_eq!(BUCKETS.load(Ordering::Relaxed), 0);
	} else {
		assert_eq!(ROUTING.load(Ordering::Relaxed), 0);
	}
	assert_eq!(environment.view().allocated_bytes(), baseline);
	assert_eq!(register.read_local_amplitudes(0, initial.len())?, initial);
	child.apply(&mut register, false, 0, 0)?;
	child.apply(&mut register, true, 0, 0)?;
	for (a, b) in register
		.read_local_amplitudes(0, initial.len())?
		.iter()
		.zip(initial)
	{
		assert!((*a - b).norm() < 1e-12);
	}
	assert_eq!(environment.view().allocated_bytes(), baseline);
	drop(guard);
	Ok(())
}

#[test]
fn peak_arithmetic_is_checked() {
	let limits = PersistenceLimits::default();
	assert!(super::peak(usize::MAX, 0, limits).is_err());
	assert!(super::peak(0, usize::MAX, limits).is_err());
	assert!(
		super::peak(
			0,
			0,
			PersistenceLimits {
				io: ShardIoLimits {
					max_manifest_bytes: usize::MAX,
					..limits.io
				},
				..limits
			}
		)
		.is_err()
	);
	assert!(
		super::admit(
			0,
			0,
			PersistenceLimits {
				max_bytes: 1,
				..limits
			}
		)
		.is_err()
	);
}
