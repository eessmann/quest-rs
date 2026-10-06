#![cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::panic_in_result_fn,
	reason = "Small fixed MPI fixture checks independently counted loader operations"
)]
use quest::{
	Complex64,
	collective::MpiRuntime,
	qsvt::{
		matching::preprocess::{ProducerLimits, produce_matching},
		persisted_matching::{
			PersistenceLimits, load_matching, load_matching_resource, publish_produced,
		},
	},
};
use std::{path::PathBuf, time::Duration};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
#[allow(
	clippy::too_many_lines,
	reason = "One MPI lifecycle compares loader paths, immutable observations and independently counted collective work"
)]
fn loader_reports_actual_phase_work_without_replicating_records() -> TestResult {
	if std::env::var("QUEST_LOAD_STATISTICS_CHILD").is_err() {
		let root =
			std::env::temp_dir().join(format!("quest-load-statistics-{}", std::process::id()));
		std::fs::create_dir(&root)?;
		for ranks in [1, 2, 4] {
			let directory = root.join(format!("ranks-{ranks}"));
			std::fs::create_dir(&directory)?;
			let output = quest_test_support::mpi::MpiTest::new(ranks, Duration::from_secs(60))?
				.args([
					"--exact",
					"loader_reports_actual_phase_work_without_replicating_records",
					"--nocapture",
					"--test-threads=1",
				])
				.env("QUEST_LOAD_STATISTICS_CHILD", "1")
				.env("QUEST_LOAD_STATISTICS_DIRECTORY", directory)
				.output()?;
			assert!(
				output.status.success(),
				"ranks={ranks}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		std::fs::remove_dir_all(root)?;
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let rank = usize::try_from(world.rank()?)?;
	let parts = usize::try_from(world.size()?)?;
	let directory = PathBuf::from(std::env::var("QUEST_LOAD_STATISTICS_DIRECTORY")?);
	let mut entries = Vec::new();
	for column in (rank..8).step_by(parts) {
		for (slot, (row, value)) in [
			(column, Complex64::new(0.7, 0.1)),
			(column ^ 1, Complex64::new(-0.2, 0.05)),
		]
		.into_iter()
		.enumerate()
		{
			entries.push(Ok(quest_numerics::sparse_stream::SparseEntry {
				row,
				column,
				ordinal: u64::try_from(2 * column + slot)?,
				value,
			}));
		}
	}
	let produced = produce_matching(&world, 8, 8, entries, ProducerLimits::default())?;
	let limits = PersistenceLimits {
		max_bytes: 3 * 1024 * 1024,
		io: quest_qsvt_io::sharded_matching::ShardIoLimits {
			max_buffer_bytes: 2 * 1024 * 1024,
			max_manifest_bytes: 262_144,
			..Default::default()
		},
		..PersistenceLimits::default()
	};
	assert!(limits.max_bytes < produced.statistics().peak_managed_bytes);
	assert!(limits.max_bytes > produced.retained_bytes()?);
	let manifest = directory.join("manifest.json");
	publish_produced(&world, &produced, &directory, &manifest, 4, limits)?;
	let loaded = load_matching(&world, &manifest, &directory, limits)?;
	let resource = load_matching_resource(&world, &manifest, &directory, limits.into())?;
	assert!(!resource.load_statistics().replay_admitted);
	assert_eq!(resource.load_statistics().admission.broadcasts, 0);
	assert_eq!(resource.local_records(), loaded.local_records());
	assert_eq!(resource.header(), loaded.header());
	let rejected = quest_qsvt::matching_resource::ResourceReplayLimits {
		max_work: 1,
		..limits.replay
	};
	assert!(resource.clone().admit_replay(&world, rejected).is_err());
	let explicit = resource.clone().admit_replay(&world, limits.replay)?;
	assert!(explicit.load_statistics().replay_admitted);
	assert!(!resource.load_statistics().replay_admitted);
	assert_eq!(explicit.recipe().statistics(), loaded.recipe().statistics());
	assert_eq!(loaded.header(), produced.shard().header());
	let columns: Vec<_> = loaded.local_records().iter().map(|r| r.column).collect();
	assert_eq!(columns, produced.shard().records());
	let stats = loaded.load_statistics();
	assert!(stats.replay_admitted);
	assert_eq!(stats.local_records, 16 / parts);
	assert_eq!(stats.local_reverse_records, 16 / parts);
	let part_count = u64::try_from(parts)?;
	assert_eq!(stats.reverse_broadcasts, 16 + part_count);
	// Two colors: eight fixed points and four two-cycles. Admission traverses
	// records five times and checks each cycle from every pivot in both streams.
	assert_eq!(stats.admission.next_record_calls, 90);
	assert_eq!(stats.admission.forward_calls, 144);
	assert_eq!(stats.admission.reverse_calls, 64);
	assert_eq!(stats.admission.broadcasts, 90 * part_count + 208);
	assert!(stats.total >= stats.read_validate + stats.reverse_directory + stats.replay_admission);
	let cloned = loaded.clone();
	assert_eq!(cloned.load_statistics(), stats);
	let targets: Vec<_> = (0..loaded.header().num_qubits()?).collect();
	loaded.visit_gates(&world, &targets, 0, 0, false, |_| Ok(()))?;
	assert_eq!(
		loaded.load_statistics(),
		stats,
		"later replay must not rewrite load observations"
	);
	Ok(())
}
