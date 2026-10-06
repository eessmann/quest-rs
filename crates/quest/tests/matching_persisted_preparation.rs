#![cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Bounded independent whole-unitary reference and collective admission assertions"
)]
use quest::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
	qsvt::{
		matching::{
			collective::RoutingCapacity,
			preprocess::{ProducerLimits, produce_matching},
		},
		persisted_matching::{
			PersistedPreparationLimits, PersistenceLimits, load_matching, load_matching_resource,
			publish_produced,
		},
	},
};
use quest_qsvt::{MatchingEncoding, NumericalPolicy};
use std::path::PathBuf;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
#[allow(
	clippy::too_many_lines,
	reason = "One bounded MPI lifecycle retains all ownership and no-mutation assertions together"
)]
fn consuming_persisted_preparation_admits_ownership_and_whole_unitary() -> TestResult {
	if std::env::var("QUEST_PERSISTED_PREPARATION_CHILD").is_err() {
		let base = std::env::temp_dir().join(format!(
			"quest-persisted-preparation-{}",
			std::process::id()
		));
		std::fs::create_dir(&base)?;
		for (ranks, split) in [(1, false), (2, false), (4, false), (8, false), (4, true)] {
			let directory = base.join(format!("job-{ranks}-{split}"));
			std::fs::create_dir(&directory)?;
			let output = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(120),
			)?
			.args([
				"--exact",
				"consuming_persisted_preparation_admits_ownership_and_whole_unitary",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PERSISTED_PREPARATION_CHILD", "1")
			.env(
				"QUEST_PERSISTED_PREPARATION_SPLIT",
				if split { "1" } else { "0" },
			)
			.env("QUEST_PERSISTED_PREPARATION_DIRECTORY", &directory)
			.output()?;
			assert!(
				output.status.success(),
				"MPI {ranks}/{split}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		std::fs::remove_dir_all(base)?;
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let split = std::env::var("QUEST_PERSISTED_PREPARATION_SPLIT")? == "1";
	let comm = if split {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let mut directory = PathBuf::from(std::env::var("QUEST_PERSISTED_PREPARATION_DIRECTORY")?);
	if split {
		directory = directory.join(format!("group-{}", usize::try_from(world.rank()?)? / 2));
		if rank == 0 {
			std::fs::create_dir(&directory)?;
		}
	}
	{
		let mut lane = comm.collective_lane()?;
		assert!(lane.all_agree(true)?);
	}
	let entries = [
		(0, 0, Complex64::new(1.0, 1.0)),
		(0, 1, Complex64::new(-1.0, 0.0)),
		(0, 2, Complex64::new(0.0, 2.0)),
		(1, 0, Complex64::new(0.2, 0.0)),
	];
	let input = entries
		.iter()
		.enumerate()
		.filter(|(i, _)| i % parts == rank)
		.map(|(i, &(row, column, value))| {
			Ok(quest_numerics::sparse_stream::SparseEntry {
				row,
				column,
				ordinal: u64::try_from(i).unwrap_or(u64::MAX),
				value,
			})
		});
	let produced = produce_matching(&comm, 2, 3, input, ProducerLimits::default())?;
	let persistence = PersistenceLimits {
		io: quest_qsvt_io::sharded_matching::ShardIoLimits {
			chunk_records: 2,
			max_buffer_bytes: 32768,
			max_manifest_bytes: 262_144,
			max_buckets: 16,
			max_records: 32,
			max_file_bytes: 1_048_576,
		},
		..PersistenceLimits::default()
	};
	let path = directory.join("manifest.json");
	let published = publish_produced(&comm, &produced, &directory, &path, 8, persistence)?;
	let manifest_sha256 = published.semantic_sha256();
	drop(published);
	drop(produced);
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(2_097_152))
		.build()?;
	let mut register = environment.state_vector_local(QubitCount::new(7)?)?;
	// Independent cold state/reference storage is tiny test-only data, outside the runtime owner model.
	let state: Vec<_> = (0..128_u32)
		.map(|i| Complex64::new(f64::from(i % 13) - 6.0, f64::from(i % 7) - 3.0))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|x| x / norm).collect();
	let local = register.deployment().local_amplitudes();
	let start = rank * local;
	register.write_local_amplitudes(0, &state[start..start + local])?;
	let initial = register.read_local_amplitudes(0, local)?;
	let targets = vec![3, 0, 2, 1, 4];
	let limits = PersistedPreparationLimits {
		capacity: RoutingCapacity {
			ranks_per_node: parts,
			node_budget: MemoryBudget::new(16_777_216),
		},
		..PersistedPreparationLimits::default()
	};
	// Caller loading allowance stays alive through every handoff; the bridge cannot silently transfer it.
	let loading_guard = environment.reserve_external_bytes(
		persistence.io.max_buffer_bytes + persistence.io.max_manifest_bytes + 16_384,
	)?;
	let baseline = environment.view().allocated_bytes();
	let loaded = load_matching(&comm, &path, &directory, persistence)?;
	let alias = if rank == 0 {
		Some(loaded.clone())
	} else {
		None
	};
	assert!(
		loaded
			.into_prepared_matching(&environment, QubitCount::new(7)?, targets.clone(), limits)
			.is_err()
	);
	assert_eq!(environment.view().allocated_bytes(), baseline);
	assert_eq!(register.read_local_amplitudes(0, local)?, initial);
	drop(alias);
	for failure in 0..8 {
		if parts == 1 && failure >= 6 {
			continue;
		}
		let loaded = load_matching(&comm, &path, &directory, persistence)?;
		let mut bad = limits;
		let mut map = targets.clone();
		match failure {
			0 => {
				if rank == 0 {
					bad.policy = NumericalPolicy { max_bytes: 1 };
				}
			}
			1 => bad.max_bytes = baseline,
			2 => bad.max_constructor_work = 1,
			3 => bad.max_local_records = 0,
			4 => {
				if rank == 0 {
					map[4] = 3;
				}
			}
			5 => {
				if rank == 0 {
					map.reserve_exact(262_144);
				}
			}
			6 => {
				if rank == 0 {
					bad.policy.max_bytes = 32_000_000;
				}
			}
			_ => {
				if rank == 0 {
					map.swap(1, 2);
				}
			}
		}
		assert!(
			loaded
				.into_prepared_matching(&environment, QubitCount::new(7)?, map, bad)
				.is_err(),
			"failure {failure}"
		);
		assert_eq!(environment.view().allocated_bytes(), baseline);
		assert_eq!(register.read_local_amplitudes(0, local)?, initial);
	}
	let loaded = load_matching(&comm, &path, &directory, persistence)?;
	let recipe = loaded.recipe().clone(); // This independent scalar recipe is not a Data alias.
	let manifest_identity = u64::from_le_bytes(manifest_sha256[..8].try_into()?);
	let (mut prepared, receipt) = loaded.into_prepared_matching(
		&environment,
		QubitCount::new(7)?,
		targets.clone(),
		limits,
	)?;
	assert_eq!(
		u64::from_le_bytes(receipt.manifest_sha256[..8].try_into()?),
		manifest_identity
	);
	assert_eq!(recipe.descriptor().rows, 2);
	assert!(receipt.loaded_source_bytes > 0 && receipt.snapshot_bytes > 0);
	assert!(receipt.rank_peak_bytes >= environment.view().allocated_bytes());
	assert!(environment.view().allocated_bytes() >= baseline + prepared.retained_bytes()?);
	// An earlier child remains owned and usable after a later handoff rejects.
	let with_first_child = environment.view().allocated_bytes();
	let later = load_matching(&comm, &path, &directory, persistence)?;
	let mut later_limits = limits;
	if rank == 0 {
		later_limits.policy.max_bytes = 1;
	}
	assert!(
		later
			.into_prepared_matching(
				&environment,
				QubitCount::new(7)?,
				targets.clone(),
				later_limits
			)
			.is_err()
	);
	assert_eq!(environment.view().allocated_bytes(), with_first_child);
	assert_eq!(register.read_local_amplitudes(0, local)?, initial);
	// The complete first-child U and standalone U† checks below now follow this rejection.
	let sparse = quest_numerics::SparseMatrix::from_triplets(
		2,
		3,
		quest_numerics::SparseFormat::Csr,
		entries.to_vec(),
		quest_numerics::SparseLimits::default(),
	)?;
	let reference = MatchingEncoding::from_sparse(&sparse, NumericalPolicy::default())?;
	let unitary = quest_qsvt::materialize_oracle(
		&reference.to_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	let extract = |basis: usize| {
		targets
			.iter()
			.enumerate()
			.fold(0, |x, (i, t)| x | (((basis >> t) & 1) << i))
	};
	let resource = load_matching_resource(&comm, &path, &directory, persistence.into())?;
	assert!(!resource.load_statistics().replay_admitted);
	assert_eq!(resource.load_statistics().admission.broadcasts, 0);
	let alias = if rank == 0 {
		Some(resource.clone())
	} else {
		None
	};
	assert!(
		resource
			.into_prepared_matching(&environment, QubitCount::new(7)?, targets.clone(), limits)
			.is_err()
	);
	assert_eq!(environment.view().allocated_bytes(), with_first_child);
	drop(alias);
	let resource = load_matching_resource(&comm, &path, &directory, persistence.into())?;
	let (mut native_only, native_receipt) = resource.into_prepared_matching(
		&environment,
		QubitCount::new(7)?,
		targets.clone(),
		limits,
	)?;
	assert_eq!(native_receipt.manifest_sha256, receipt.manifest_sha256);
	assert_eq!(
		native_receipt.native_source_identity,
		receipt.native_source_identity
	);
	assert_eq!(
		native_receipt.native_construction_identity,
		receipt.native_construction_identity
	);
	assert_eq!(native_receipt.local_records, receipt.local_records);
	for prepared in [&mut prepared, &mut native_only] {
		for value in [0, 64] {
			for adjoint in [false, true] {
				register.write_local_amplitudes(0, &state[start..start + local])?;
				prepared.apply(&mut register, adjoint, 64, value)?;
				for (row, actual) in
					(start..start + local).zip(register.read_local_amplitudes(0, local)?)
				{
					let expected = if row & 64 == value {
						(0..128)
							.filter(|col| col & 0x60 == row & 0x60)
							.map(|col| {
								let u = if adjoint {
									unitary[(extract(col), extract(row))].conj()
								} else {
									unitary[(extract(row), extract(col))]
								};
								u * state[col]
							})
							.sum()
					} else {
						state[row]
					};
					assert!((actual - expected).norm() < 3e-12);
				}
			}
		}
	}
	drop(native_only);
	drop(prepared);
	assert_eq!(environment.view().allocated_bytes(), baseline);
	drop(loading_guard);
	Ok(())
}
