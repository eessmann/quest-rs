#![cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Small independent portable and native differential plus collective assertions"
)]
use quest::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
	qsvt::{
		matching::preprocess::{ProducerLimits, produce_matching},
		persisted_matching::{
			LoadedMatching, PersistenceLimits, load_matching, load_matching_resource,
			publish_produced,
		},
		replay_native::CollectiveReplayGateExecutor,
	},
};
use quest_compile::{Angle, Control, ControlState, Gate, QuantumRegionBuilder};
use quest_qsvt::{MatchingEncoding, NumericalPolicy, ReplayKind, materialize_program};
use std::path::PathBuf;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn portable(
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	loaded: &LoadedMatching,
	targets: &[usize],
	mask: usize,
	value: usize,
	adjoint: bool,
) -> TestResult<faer::Mat<Complex64>> {
	let mut builder = QuantumRegionBuilder::new(7, 0)?;
	let mut count = 0;
	loaded.visit_gates(comm, targets, mask, value, adjoint, |primitive| {
		count += 1;
		if count > 512 {
			return Err(quest::Error::Value("cold portable fixture gate bound"));
		}
		let mut controls = Vec::new();
		for bit in 0..7 {
			if primitive.control_mask & (1 << bit) != 0 {
				controls.push(Control::new(
					builder.qubit(bit)?,
					if primitive.control_value & (1 << bit) == 0 {
						ControlState::Zero
					} else {
						ControlState::One
					},
				));
			}
		}
		if let ReplayKind::Phase(angle) = primitive.kind {
			builder.global_phase(Angle::radians(angle)?, &controls)?;
		} else {
			let gate = match primitive.kind {
				ReplayKind::H => Gate::H,
				ReplayKind::X => Gate::X,
				ReplayKind::Ry(a) => Gate::Ry(Angle::radians(a)?),
				ReplayKind::Phase(_) => return Err(quest::Error::Value("phase fixture")),
			};
			builder.gate(
				gate,
				&[builder.qubit(
					primitive
						.target
						.ok_or(quest::Error::Value("target fixture"))?,
				)?],
				&controls,
			)?;
		}
		Ok(())
	})?;
	Ok(materialize_program(
		&builder.finish()?.bind(&[])?,
		NumericalPolicy::default(),
	)?)
}
#[test]
#[allow(
	clippy::too_many_lines,
	reason = "Self-launching bounded MPI lifecycle fixture exercises collective sequence and full register differentials"
)]
fn persisted_gate_replay_and_collective_restart() -> TestResult {
	if std::env::var("QUEST_RESOURCE_CHILD").is_err() {
		let mut canonical = None;
		let base =
			std::env::temp_dir().join(format!("quest-persisted-replay-{}", std::process::id()));
		std::fs::create_dir(&base)?;
		for (ranks, split) in [(1, 0), (2, 0), (4, 0), (8, 0), (4, 1)] {
			let directory = base.join(format!("job-{ranks}-{split}"));
			std::fs::create_dir(&directory)?;
			let output = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(120),
			)?
			.args([
				"--exact",
				"persisted_gate_replay_and_collective_restart",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_RESOURCE_CHILD", "1")
			.env("QUEST_RESOURCE_SPLIT", split.to_string())
			.env("QUEST_RESOURCE_DIRECTORY", &directory)
			.output()?;
			let stdout = String::from_utf8(output.stdout)?;
			let stderr = String::from_utf8(output.stderr)?;
			assert!(
				output.status.success(),
				"MPI ranks={ranks} split={split}: {stdout}\n{stderr}"
			);
			let identities: Vec<_> = stdout
				.lines()
				.filter_map(|line| line.find("RESOURCE_ID ").and_then(|i| line.get(i..)))
				.collect();
			assert_ne!(identities.len(), 0);
			for identity in &identities {
				if let Some(expected) = &canonical {
					assert_eq!(identity, expected);
				} else {
					canonical = Some((*identity).to_owned());
				}
			}
			eprintln!(
				"Persisted gate replay ranks={ranks} split={split}: {}",
				identities[0]
			);
		}
		// Explicit change of producer rank count to restart rank count, with 8 fixed logical buckets.
		let restart = base.join("rank-restart");
		std::fs::create_dir(&restart)?;
		for (ranks, mode) in [
			(4, "publish"),
			(2, "restart"),
			(8, "restart"),
			(3, "reject"),
		] {
			let output = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(120),
			)?
			.args([
				"--exact",
				"persisted_gate_replay_and_collective_restart",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_RESOURCE_CHILD", "1")
			.env("QUEST_RESOURCE_SPLIT", "0")
			.env("QUEST_RESOURCE_MODE", mode)
			.env("QUEST_RESOURCE_DIRECTORY", &restart)
			.output()?;
			assert!(
				output.status.success(),
				"Restart {ranks} {mode}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
			eprintln!("Persisted producer/restart ranks={ranks} mode={mode}: passed");
		}
		std::fs::remove_dir_all(base)?;
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let split = std::env::var("QUEST_RESOURCE_SPLIT")?.parse::<usize>()?;
	let comm = if split == 1 {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let mut directory = PathBuf::from(std::env::var("QUEST_RESOURCE_DIRECTORY")?);
	if split == 1 {
		directory = directory.join(format!("group-{}", usize::try_from(world.rank()?)? / 2));
		if rank == 0 {
			std::fs::create_dir(&directory)?;
		}
	}
	{
		let mut lane = comm.collective_lane()?;
		assert!(lane.all_agree(true)?);
	}
	let limits = PersistenceLimits {
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
	let mode = std::env::var("QUEST_RESOURCE_MODE").unwrap_or_default();
	if mode == "reject" {
		assert!(load_matching(&comm, &path, &directory, limits).is_err());
		return Ok(());
	}
	if mode != "restart" {
		let input = (10..14_u64)
			.filter(|&ordinal| usize::try_from(ordinal).is_ok_and(|o| o % parts == rank))
			.map(|ordinal| {
				let (row, column, value) = match ordinal {
					10 => (0, 0, Complex64::new(1.0, 1.0)),
					11 => (0, 1, Complex64::new(-1.0, 0.0)),
					12 => (0, 2, Complex64::new(0.0, 2.0)),
					_ => (1, 0, Complex64::new(0.2, 0.0)),
				};
				Ok(quest_numerics::sparse_stream::SparseEntry {
					row,
					column,
					ordinal,
					value,
				})
			});
		let produced = produce_matching(&comm, 2, 3, input, ProducerLimits::default())?;

		let rejected = directory.join("budget-rejected");
		if rank == 0 {
			std::fs::create_dir(&rejected)?;
		}
		{
			let mut lane = comm.collective_lane()?;
			assert!(lane.all_agree(true)?);
		}
		let publication_scratch = 16
			* (size_of::<quest_qsvt_io::sharded_matching::MatchingBucketReceipt>() + 64)
			+ limits.io.max_buffer_bytes
			+ produced.edges().len() * size_of::<usize>();
		assert!(
			publish_produced(
				&comm,
				&produced,
				&rejected,
				rejected.join("manifest.json"),
				8,
				PersistenceLimits {
					max_bytes: publication_scratch,
					..limits
				}
			)
			.is_err()
		);
		assert!(!rejected.join("manifest.json").exists());
		// An owner IO failure never publishes a manifest.
		let failed_owner = if rank == 0 {
			directory.join("absent-owner")
		} else {
			rejected
		};
		let failed_manifest = directory.join("failed-owner.json");
		assert!(
			publish_produced(&comm, &produced, failed_owner, &failed_manifest, 8, limits).is_err()
		);
		assert!(!failed_manifest.exists());
		publish_produced(&comm, &produced, &directory, &path, 8, limits)?;
		drop(produced);
	}
	let loaded = load_matching(&comm, &path, &directory, limits)?;
	assert!(
		loaded
			.local_records()
			.iter()
			.all(|r| r.column.source % parts == rank)
	);
	assert!(loaded.retained_bytes() < 16_384);
	let targets = [3, 0, 2, 1, 4];
	let matrix = portable(&comm, &loaded, &targets, 1 << 6, 0, false)?;
	let reverse = portable(&comm, &loaded, &targets, 1 << 6, 0, true)?;
	for row in 0..128 {
		for col in 0..128 {
			assert!((reverse[(row, col)] - matrix[(col, row)].conj()).norm() < 2e-12);
		}
	}
	// Independent serial matching construction supplies the whole-unitary semantic reference.
	let sparse = quest_numerics::SparseMatrix::from_triplets(
		2,
		3,
		quest_numerics::SparseFormat::Csr,
		vec![
			(0, 0, Complex64::new(1.0, 1.0)),
			(0, 1, Complex64::new(-1.0, 0.0)),
			(0, 2, Complex64::new(0.0, 2.0)),
			(1, 0, Complex64::new(0.2, 0.0)),
		],
		quest_numerics::SparseLimits::default(),
	)?;
	let reference = MatchingEncoding::from_sparse(&sparse, NumericalPolicy::default())?;
	let local_matrix = quest_qsvt::materialize_oracle(
		&reference.to_oracle(NumericalPolicy::default())?,
		NumericalPolicy::default(),
	)?;
	for row in 0..128 {
		for col in 0..128 {
			let expected = if row & (1 << 6) != 0 {
				Complex64::new(f64::from(row == col), 0.0)
			} else if row & 0b110_0000 != col & 0b110_0000 {
				Complex64::new(0.0, 0.0)
			} else {
				let extract = |index: usize| {
					targets
						.iter()
						.enumerate()
						.fold(0, |v, (bit, target)| v | (((index >> target) & 1) << bit))
				};
				local_matrix[(extract(row), extract(col))]
			};
			assert!((matrix[(row, col)] - expected).norm() < 3e-12);
		}
	}
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(8_388_608))
		.build()?;
	let mut register = environment.state_vector_local(QubitCount::new(7)?)?;
	let mut primitive = CollectiveReplayGateExecutor::new(&register)?;
	// Identity and malformed primitive checks do not mutate the initial state.
	register.init_zero()?;
	let initial_zero =
		register.read_local_amplitudes(0, register.deployment().local_amplitudes())?;
	assert!(
		primitive
			.apply(
				&mut register,
				quest_qsvt::ReplayGate {
					kind: ReplayKind::Ry(f64::NAN),
					target: Some(0),
					control_mask: 0,
					control_value: 0
				}
			)
			.is_err()
	);
	assert!(
		primitive
			.apply(
				&mut register,
				quest_qsvt::ReplayGate {
					kind: ReplayKind::X,
					target: Some(0),
					control_mask: 1,
					control_value: 1
				}
			)
			.is_err()
	);
	assert_eq!(
		register.read_local_amplitudes(0, initial_zero.len())?,
		initial_zero
	);
	let mut narrow = environment.state_vector_local(QubitCount::new(6)?)?;
	assert!(
		primitive
			.apply(
				&mut narrow,
				quest_qsvt::ReplayGate {
					kind: ReplayKind::X,
					target: Some(0),
					control_mask: 0,
					control_value: 0
				}
			)
			.is_err()
	);
	drop(narrow);
	drop(primitive);
	let mut fused = environment.prepare_matching(
		loaded.matching_shard(NumericalPolicy::default())?,
		QubitCount::new(7)?,
		targets.to_vec(),
	)?;
	let local = register.deployment().local_amplitudes();
	let start = rank * local;
	let state: Vec<_> = (0..128_u32)
		.map(|i| Complex64::new(f64::from(i % 13) - 6.0, f64::from(i % 7) - 3.0))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|v| v / norm).collect();
	for outer_value in [0, 1 << 6] {
		for adjoint in [false, true] {
			register.write_local_amplitudes(0, &state[start..start + local])?;
			loaded.apply_native(&mut register, &targets, 1 << 6, outer_value, adjoint)?;
			let actual = register.read_local_amplitudes(0, local)?;
			let unitary = if outer_value == 0 {
				if adjoint { &reverse } else { &matrix }
			} else {
				// Build the same independent portable production stream for signed-one control.
				&portable(&comm, &loaded, &targets, 1 << 6, outer_value, adjoint)?
			};
			for (row, a) in (start..start + local).zip(&actual) {
				let expected: Complex64 =
					(0..128).map(|col| unitary[(row, col)] * state[col]).sum();
				assert!((*a - expected).norm() < 3e-12);
			}
			register.write_local_amplitudes(0, &state[start..start + local])?;
			fused.apply(&mut register, adjoint, 1 << 6, outer_value)?;
			for (a, b) in register
				.read_local_amplitudes(0, local)?
				.iter()
				.zip(&actual)
			{
				assert!((*a - *b).norm() < 3e-12);
			}
		}
	}
	assert!(
		loaded
			.apply_native(&mut register, &[3, 0, 2, 1, 1], 1 << 6, 0, false)
			.is_err()
	);
	assert!(
		load_matching(
			&comm,
			&path,
			&directory,
			PersistenceLimits {
				replay: quest_qsvt::matching_resource::ResourceReplayLimits {
					max_work: 1,
					..limits.replay
				},
				..limits
			}
		)
		.is_err()
	);
	assert!(
		load_matching(
			&comm,
			&path,
			&directory,
			PersistenceLimits {
				max_communication_bytes: 0,
				..limits
			}
		)
		.is_err()
	);
	// File replacement cannot change the already owned resource. A new load rejects it.
	if mode.is_empty() {
		let malformed = directory.join("matching-0000000000000000.h5");
		let original = if rank == 0 {
			let bytes = std::fs::read(&malformed)?;
			std::fs::write(&malformed, b"malformed matching bucket")?;
			Some(bytes)
		} else {
			None
		};
		{
			let mut lane = comm.collective_lane()?;
			assert!(lane.all_agree(true)?);
		}
		assert!(load_matching(&comm, &path, &directory, limits).is_err());
		assert!(load_matching_resource(&comm, &path, &directory, limits.into()).is_err());
		if let Some(bytes) = original {
			std::fs::write(&malformed, bytes)?;
		}
		{
			let mut lane = comm.collective_lane()?;
			assert!(lane.all_agree(true)?);
		}
		if rank == 0 {
			std::fs::remove_file(malformed)?;
		}

		{
			let mut lane = comm.collective_lane()?;
			assert!(lane.all_agree(true)?);
		}
		assert!(load_matching(&comm, &path, &directory, limits).is_err());
		assert!(load_matching_resource(&comm, &path, &directory, limits.into()).is_err());
		let replayed = portable(&comm, &loaded, &targets, 1 << 6, 0, false)?;
		for row in 0..128 {
			for col in 0..128 {
				assert!((replayed[(row, col)] - matrix[(row, col)]).norm() < 1e-14);
			}
		}
	}
	if rank == 0 {
		println!(
			"RESOURCE_ID {}:{}",
			loaded.header().source_identity,
			loaded.header().record_digest
		);
	}
	Ok(())
}
