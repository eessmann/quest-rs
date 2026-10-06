//! Bounded private fault injection; does not create a production callback API.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static CONVERTED: AtomicUsize = AtomicUsize::new(0);
pub(super) fn converted() {
	CONVERTED.fetch_add(1, Ordering::Relaxed);
}
#[allow(
	clippy::panic,
	reason = "Intentional library-test-only panic in a caught rank-local source conversion"
)]
pub(super) fn inject(rank: usize) {
	assert!(
		!(rank == 0 && std::env::var("QUEST_PERSISTED_CONVERSION_PANIC").is_ok()),
		"source conversion fault"
	);
}
#[test]
fn checked_constructor_envelopes_reject_overflow_and_cover_empty_coordination() {
	assert!(work(usize::MAX, 1, 2).is_err());
	assert!(payload(usize::MAX, 2).is_err());
	assert!(work(0, 8, 2).is_ok_and(|x| x > 4096));
	assert!(payload(0, 8).is_ok_and(|x| x > 0));
}
#[test]
#[allow(
	clippy::too_many_lines,
	clippy::panic_in_result_fn,
	clippy::indexing_slicing,
	clippy::arithmetic_side_effects,
	reason = "Bounded MPI metadata, panic and real native temporary-capacity faults prove common no-mutation rejection"
)]
fn bridge_private_failures_reject_collectively()
-> std::result::Result<(), Box<dyn std::error::Error>> {
	if std::env::var("QUEST_PERSISTED_FAULT_CHILD").is_err() {
		let base =
			std::env::temp_dir().join(format!("quest-persisted-faults-{}", std::process::id()));
		std::fs::create_dir(&base)?;
		for parts in [1, 2] {
			for mode in [
				"panic",
				"ownership",
				"header",
				"manifest",
				"overflow",
				"capacity",
			] {
				// Different common metadata is meaningful only with at least two peers.
				if parts == 1 && ["header", "manifest"].contains(&mode) {
					continue;
				}
				let directory = base.join(format!("{parts}-{mode}"));
				std::fs::create_dir(&directory)?;
				let mut child = quest_test_support::mpi::MpiTest::new(
					usize::try_from(parts)?,
					std::time::Duration::from_secs(30),
				)?;
				child.args(["--exact","qsvt::persisted_matching::preparation::failure_tests::bridge_private_failures_reject_collectively","--nocapture","--test-threads=1"]).env("QUEST_PERSISTED_FAULT_CHILD",mode).env("QUEST_PERSISTED_FAULT_DIRECTORY",&directory);
				if mode == "panic" {
					child.env("QUEST_PERSISTED_CONVERSION_PANIC", "1");
				}
				if mode == "capacity" {
					child.env("QUEST_MATCHING_VALIDATION_CHILD", "1");
				}
				let output = child.output()?;
				assert!(
					output.status.success(),
					"{parts}/{mode}: {} {}",
					String::from_utf8_lossy(&output.stdout),
					String::from_utf8_lossy(&output.stderr)
				);
			}
		}
		std::fs::remove_dir_all(base)?;
		return Ok(());
	}
	let runtime = crate::collective::MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = world.duplicate()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let directory = std::path::PathBuf::from(std::env::var("QUEST_PERSISTED_FAULT_DIRECTORY")?);
	let path = directory.join("manifest.json");
	let input = (0..2).filter(|i| i % parts == rank).map(|i| {
		Ok(quest_numerics::sparse_stream::SparseEntry {
			row: i,
			column: i,
			ordinal: u64::try_from(i).unwrap_or(u64::MAX),
			value: crate::Complex64::new(1.0, 0.0),
		})
	});
	let produced = crate::qsvt::matching::preprocess::produce_matching(
		&comm,
		2,
		2,
		input,
		crate::qsvt::matching::preprocess::ProducerLimits::default(),
	)?;
	let persistence = super::super::PersistenceLimits {
		io: quest_qsvt_io::sharded_matching::ShardIoLimits {
			chunk_records: 1,
			max_buffer_bytes: 16384,
			max_manifest_bytes: 65536,
			max_buckets: 4,
			max_records: 4,
			max_file_bytes: 1_048_576,
		},
		..super::super::PersistenceLimits::default()
	};
	super::super::publish_produced(&comm, &produced, &directory, &path, 2, persistence)?;
	drop(produced);
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(262_144))
		.build()?;
	let mut register = environment.state_vector_local(QubitCount::new(2)?)?;
	register.init_zero()?;
	let initial = register.read_local_amplitudes(0, register.deployment().local_amplitudes())?;
	// Retained source and transient decoder/IO reservations have distinct lifetimes.
	// The decoder needs 3*input_capacity + 8192 + manifest/receipt storage.
	let guard = environment.reserve_external_bytes(8192)?;
	let io_guard = environment.reserve_external_bytes(
		persistence.io.max_buffer_bytes + persistence.io.max_manifest_bytes,
	)?;
	let mut loaded = super::super::load_matching(&comm, &path, &directory, persistence)?;
	drop(io_guard); // The loader's IO/decoder temporary owners have been dropped.
	let baseline = environment.view().allocated_bytes();
	let mode = std::env::var("QUEST_PERSISTED_FAULT_CHILD")?;
	if rank == 0 {
		let data = std::sync::Arc::get_mut(&mut loaded.data).ok_or("unique test source")?;
		match mode.as_str() {
			"ownership" => data.rank = parts,
			"header" => data.header.source_identity ^= 1,
			"manifest" => data.manifest_identity[31] ^= 1,
			"overflow" => data.common_bytes = usize::MAX,
			_ => {}
		}
	}
	let limits = PersistedPreparationLimits {
		max_bytes: if mode == "capacity" { 98304 } else { 262_144 },
		capacity: RoutingCapacity {
			ranks_per_node: parts,
			node_budget: MemoryBudget::new(524_288),
		},
		..PersistedPreparationLimits::default()
	};
	assert!(
		loaded
			.into_prepared_matching(&environment, QubitCount::new(2)?, vec![0, 1], limits)
			.is_err()
	);
	if mode == "capacity" {
		assert_eq!(
			CONVERTED.load(Ordering::Relaxed),
			1,
			"fault must reach native preparation after conversion"
		);
	}
	assert_eq!(environment.view().allocated_bytes(), baseline);
	assert_eq!(register.read_local_amplitudes(0, initial.len())?, initial);
	drop(guard);
	Ok(())
}
#[test]
#[allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Bounded metadata-only mutations prove every header, policy, limit, SHA byte and ordered target binds the collective frame"
)]
fn frame_binds_complete_metadata_and_bounds_maximum_width()
-> std::result::Result<(), Box<dyn std::error::Error>> {
	let data = || super::super::Data {
		header: quest_qsvt::MatchingHeader {
			rows: 2,
			cols: 2,
			system_qubits: 1,
			color_qubits: 0,
			num_colors: 1,
			beta: 1.0,
			alpha: 1.0,
			source_identity: 123,
			record_count: 0,
			record_digest: 0,
		},
		rank: 0,
		parts: 1,
		records: vec![],
		reverse: vec![],
		frozen_identity: 0,
		manifest_identity: [0; 32],
		common_bytes: 0,
		query_work: 0,
		query_wire: 0,
		load_statistics: super::super::LoadStatistics::default(),
	};
	let limits = PersistedPreparationLimits::default();
	let base = frame(&data(), QubitCount::new(2)?, &[0, 1], limits)?;
	for field in 0..10 {
		let mut changed = data();
		match field {
			0 => changed.header.rows += 1,
			1 => changed.header.cols += 1,
			2 => changed.header.system_qubits += 1,
			3 => changed.header.color_qubits += 1,
			4 => changed.header.num_colors += 1,
			5 => changed.header.beta = 2.0,
			6 => changed.header.alpha = 2.0,
			7 => changed.header.source_identity += 1,
			8 => changed.header.record_count += 1,
			_ => changed.header.record_digest += 1,
		}
		let changed = frame(&changed, QubitCount::new(2)?, &[0, 1], limits)?;
		assert_ne!(
			base.bytes[..base.len],
			changed.bytes[..changed.len],
			"header field {field}"
		);
	}
	for i in 0..32 {
		let mut changed = data();
		changed.manifest_identity[i] = 1;
		let changed = frame(&changed, QubitCount::new(2)?, &[0, 1], limits)?;
		assert_ne!(
			base.bytes[..base.len],
			changed.bytes[..changed.len],
			"SHA byte {i}"
		);
	}
	for i in 0..7 {
		let mut changed = limits;
		match i {
			0 => changed.max_local_records += 1,
			1 => changed.max_bytes += 1,
			2 => changed.max_constructor_work += 1,
			3 => changed.max_application_payload_bytes += 1,
			4 => changed.policy.max_bytes += 1,
			5 => changed.capacity.ranks_per_node += 1,
			_ => changed.capacity.node_budget = MemoryBudget::new(usize::MAX - 1),
		}
		let changed = frame(&data(), QubitCount::new(2)?, &[0, 1], changed)?;
		assert_ne!(
			base.bytes[..base.len],
			changed.bytes[..changed.len],
			"limit field {i}"
		);
	}
	let changed = frame(&data(), QubitCount::new(2)?, &[1, 0], limits)?;
	assert_ne!(base.bytes[..base.len], changed.bytes[..changed.len]);
	let targets: Vec<_> = (0..usize::BITS - 2)
		.map(|n| usize::try_from(n).unwrap_or(0))
		.collect();
	let largest = frame(&data(), QubitCount::new(targets.len())?, &targets, limits)?;
	assert!(largest.len <= 1024);
	Ok(())
}
