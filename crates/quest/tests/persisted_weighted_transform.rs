#![allow(
	dead_code,
	reason = "The cold integration imports consumer support modules without running its publication campaign entry point"
)]
#![cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	clippy::panic_in_result_fn,
	reason = "Fixed N4 persisted whole-U differential imports the consumer and keeps bounded independent reference storage only in tests"
)]
#[path = "../examples/persisted_weighted_transform/execution.rs"]
mod execution;
#[path = "../examples/persisted_weighted_transform/phases.rs"]
mod phases;
#[path = "../examples/persisted_weighted_transform/policy.rs"]
mod policy;
#[path = "../examples/persisted_weighted_transform/runtime.rs"]
mod runtime;
use quest::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
};
use quest_qsvt::{
	EncodingDescriptor, MatchingHeader, NumericalPolicy, ReplayEncoding, ReplayGate,
	matching_resource::{MatchingResource, ResourceMatchingEncoding, ResourceMatchingRecord},
	portfolio::{PortfolioLimits, WeightedLcu},
	replay_transform::{ReplayTransform, TransformSchedule},
};
use quest_qsvt_io::sharded_matching::{MatchingManifest, PersistedMatchingRecord};
use std::path::Path;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn readout_fixture_amplitude(physical: usize, adjoint: bool) -> Complex64 {
	match physical {
		0 => Complex64::new(10. / 13., 0.),
		512 => Complex64::new(0., if adjoint { 2. / 13. } else { -2. / 13. }),
		1 => Complex64::new((5_f64 / 13.).sqrt(), 0.),
		_ => Complex64::new(0., 0.),
	}
}

// Independent bounded whole-register scalar reference. The logical system bits
// occupy physical bits 9..5; all five low bits must vanish in the success sector.
// This test reference creates no matching source or inverse schedule.
fn readout_reference(adjoint: bool) -> [f64; 6] {
	let mut result = [0.; 6];
	for physical in 0_usize..1024 {
		let z = readout_fixture_amplitude(physical, adjoint);
		result[0] += z.norm_sqr();
		if physical % 32 != 0 {
			continue;
		}
		let mut j = 0;
		for bit in 0..5 {
			j |= ((physical >> (9 - bit)) & 1) << bit;
		}
		let x = z * 2.;
		let paired = readout_fixture_amplitude(physical ^ 512, adjoint) * 2.;
		let expected = match j {
			0 => Complex64::new(20. / 13., 0.),
			1 => Complex64::new(0., if adjoint { 4. / 13. } else { -4. / 13. }),
			_ => Complex64::new(0., 0.),
		};
		let residual = 0.625 * x
			+ Complex64::new(0., if adjoint { -0.125 } else { 0.125 }) * paired
			- Complex64::new(f64::from(j == 0), 0.);
		result[1] += z.norm_sqr();
		result[2] += (x - expected).norm_sqr();
		result[3] += residual.norm_sqr();
		result[4] += x.norm_sqr();
		result[5] += 1.;
	}
	result
}

#[test]
fn consumer_pair_packet_rejects_incomplete_status_before_decode() -> Result {
	let mut packet = [0_u8; 1024];
	packet[..8].copy_from_slice(&1.25_f64.to_le_bytes());
	packet[8..16].copy_from_slice(&(-0.75_f64).to_le_bytes());
	for count in [0, 1, 1023, 1025, usize::MAX] {
		assert!(execution::decode_pair_packet(&packet, count).is_err());
	}
	let decoded = execution::decode_pair_packet(&packet, packet.len())?;
	assert_eq!(decoded.len(), 64);
	assert_eq!(decoded[0], Complex64::new(1.25, -0.75));
	assert!(decoded[1..].iter().all(|z| z.norm_sqr() == 0.));
	Ok(())
}

#[test]
fn distributed_consumer_readout_uses_matching_packets() -> Result {
	if std::env::var("QUEST_PERSISTED_READOUT_CHILD").is_err() {
		for (parts, split) in [(1, false), (2, false), (4, false), (8, false), (4, true)] {
			let output = quest_test_support::mpi::MpiTest::new(
				usize::try_from(parts)?,
				std::time::Duration::from_secs(10),
			)?
			.args([
				"--exact",
				"distributed_consumer_readout_uses_matching_packets",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PERSISTED_READOUT_CHILD", "1")
			.env(
				"QUEST_PERSISTED_READOUT_SPLIT",
				if split { "1" } else { "0" },
			)
			.output()?;
			assert!(
				output.status.success(),
				"readout parts={parts} split={split} status={}: {} {}",
				output.status,
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
			eprintln!("Readout fixture parts={parts} split={split}: passed");
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let mut comm = if std::env::var("QUEST_PERSISTED_READOUT_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let readout_comm = comm.duplicate()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(policy::RANK_BYTES))
		.build()?;
	let _guard = env.reserve_external_bytes(policy::READOUT_BYTES)?;
	let mut state = env.state_vector_local(QubitCount::new(10)?)?;
	let local = state.deployment().local_amplitudes();
	// Synthetic one-rank short-status fault: the malformed owner rejects before
	// decoding; all owners return an error before downstream readout arithmetic.
	assert!(
		runtime::local(&readout_comm, || {
			execution::decode_pair_packet(&[0_u8; 1024], if rank == 0 { 1023 } else { 1024 })
		})
		.is_err()
	);
	for adjoint in [false, true] {
		// Direct simulator initialization is test-only and is not coherent RHS preparation.
		let values: Vec<_> = (rank * local..(rank + 1) * local)
			.map(|physical| readout_fixture_amplitude(physical, adjoint))
			.collect();
		state.write_local_amplitudes(0, &values)?;
		eprintln!("READOUT_ENTER rank={rank} parts={parts} adjoint={adjoint}");
		let actual = execution::readout(&state, &readout_comm, 2., adjoint)?;
		let expected = readout_reference(adjoint);
		for (name, value) in [
			("total_probability", expected[0]),
			("success_probability", expected[1]),
			("relative_vector_error", (expected[2] / (32. / 13.)).sqrt()),
			("relative_residual", expected[3].sqrt()),
			("recovered_norm_squared", expected[4]),
			("system_coordinates_visited", expected[5]),
		] {
			let reported = actual
				.get(name)
				.and_then(serde_json::Value::as_f64)
				.ok_or(name)?;
			assert!(
				(reported - value).abs() < 2e-14,
				"{name}: {reported} != {value}"
			);
		}
		assert_eq!(actual["local_chunks"], local / 64);
		assert_eq!(
			actual["local_pair_payload_sent_bytes"],
			if parts == 1 { 0 } else { local * 16 }
		);
	}
	Ok(())
}
#[derive(Clone, Debug)]
struct FullResource {
	header: MatchingHeader,
	id: u64,
	records: Vec<ResourceMatchingRecord>,
}
impl MatchingResource for FullResource {
	fn header(&self) -> MatchingHeader {
		self.header
	}
	fn frozen_identity(&self) -> u64 {
		self.id
	}
	fn retained_bytes(&self) -> quest_qsvt::Result<usize> {
		Ok(self.records.capacity() * size_of::<ResourceMatchingRecord>() + size_of::<Self>())
	}
	fn query_work_bound(&self) -> usize {
		64
	}
	fn query_communication_bound(&self) -> usize {
		0
	}
	fn next_record(
		&self,
		color: usize,
		bound: Option<usize>,
		reverse: bool,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		let candidates = self.records.iter().filter(|r| {
			r.column.color == color
				&& bound.is_none_or(|b| {
					if reverse {
						r.column.source < b
					} else {
						r.column.source > b
					}
				})
		});
		Ok(if reverse {
			candidates.max_by_key(|r| r.column.source)
		} else {
			candidates.min_by_key(|r| r.column.source)
		}
		.copied())
	}
	fn forward(
		&self,
		color: usize,
		source: usize,
	) -> quest_qsvt::Result<Option<ResourceMatchingRecord>> {
		Ok(self
			.records
			.iter()
			.find(|r| r.column.color == color && r.column.source == source)
			.copied())
	}
	fn reverse(&self, color: usize, destination: usize) -> quest_qsvt::Result<Option<usize>> {
		Ok(self
			.records
			.iter()
			.find(|r| r.column.color == color && r.column.destination == destination)
			.map(|r| r.column.source))
	}
}
#[derive(Clone, Debug)]
struct Oracle {
	resource: ResourceMatchingEncoding<FullResource>,
	descriptor: EncodingDescriptor,
}
impl ReplayEncoding for Oracle {
	fn descriptor(&self) -> quest_qsvt::Result<EncodingDescriptor> {
		Ok(self.descriptor.clone())
	}
	fn retained_bytes(&self) -> quest_qsvt::Result<usize> {
		self.resource.retained_bytes()
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		visitor: &mut dyn FnMut(ReplayGate) -> quest_qsvt::Result<()>,
	) -> quest_qsvt::Result<()> {
		self.resource.visit_replay(adjoint, visitor)
	}
}
fn oracle(directory: &Path, term: usize) -> Result<Oracle> {
	let path = directory.join(format!("term-{term}"));
	let io = runtime::persistence().io;
	let manifest = MatchingManifest::open(path.join("manifest.json"), io)?;
	let header = manifest.header()?;
	let mut records = Vec::new();
	for bucket in 0..8 {
		manifest
			.open_bucket(&path, bucket, io)?
			.visit_records(false, |chunk| {
				records.extend(chunk.iter().map(|r: &PersistedMatchingRecord| {
					ResourceMatchingRecord {
						column: r.column,
						theta: r.theta,
						phase_angle: r.phase_angle,
						is_edge: r.is_edge,
					}
				}));
				Ok(())
			})?;
	}
	assert_eq!(records.len(), 8);
	let descriptor = EncodingDescriptor::from_matching_header(header)?;
	let resource = ResourceMatchingEncoding::new(
		FullResource {
			header,
			id: u64::from_le_bytes(manifest.semantic_sha256()[..8].try_into()?),
			records,
		},
		runtime::persistence().replay,
	)?;
	// Resource admission independently validates the actual immutable colored records/angles.
	// Native uses the header completion identity; the portable resource adds file provenance.
	// This test adapter uses native completion metadata only after checking identical records.
	assert_eq!(
		resource.recipe().descriptor().source_identity,
		descriptor.source_identity
	);
	Ok(Oracle {
		resource,
		descriptor,
	})
}
fn consumer_admission(
	env: &CollectiveEnvironment<'_, '_>,
	state: &quest::collective::CollectiveRegister<'_, '_, '_>,
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	directory: &Path,
) -> Result {
	let rank = usize::try_from(comm.rank()?)?;
	let before = state.read_local_amplitudes(0, state.deployment().local_amplitudes())?;
	let bytes = env.view().allocated_bytes();
	assert!(
		runtime::common(
			comm,
			if rank == 0 {
				b"mutated-mode"
			} else {
				b"replay"
			}
		)
		.is_err()
	);
	assert!(
		runtime::common(
			comm,
			if rank == 0 {
				b"mutated-phase-descriptor"
			} else {
				b"frozen-phase-descriptor"
			}
		)
		.is_err()
	);
	{
		let guard = env.reserve_external_bytes(policy::READOUT_BYTES)?;
		let first =
			serde_json::Value::String(String::with_capacity(if rank == 0 { 400_000 } else { 0 }));
		let second =
			serde_json::Value::String(String::with_capacity(if rank == 0 { 400_000 } else { 0 }));
		assert!(
			runtime::local(comm, || {
				runtime::admit_json_overlap(
					&[&first, &second],
					&[1, 1],
					262_144 + 65_536,
					policy::READOUT_BYTES,
				)?;
				Ok(())
			})
			.is_err()
		);
		drop(first);
		drop(second);
		drop(guard);
	}
	assert!(
		runtime::local(comm, || {
			execution::readout_floor(
				512,
				2,
				1.,
				policy::READOUT_BYTES,
				if rank == 0 { 1 } else { policy::READOUT_WORK },
			)?;
			Ok(())
		})
		.is_err()
	);
	let path = directory.join("invalid-phase.json");
	runtime::local(comm, || {
		if rank == 0 {
			std::fs::write(&path, b"{\"phase\":1,\"phase\":2}")?;
		}
		Ok(())
	})?;
	assert!(
		runtime::local(comm, || {
			if rank == 0 {
				runtime::canonical(&path)?;
			}
			Ok(())
		})
		.is_err()
	);
	assert_eq!(env.view().allocated_bytes(), bytes);
	assert_eq!(
		state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
		before
	);
	Ok(())
}
#[test]
#[allow(
	clippy::too_many_lines,
	reason = "One bounded cold rank matrix preserves one immutable eight-origin publication across all U/U† requests"
)]
fn immutable_persisted_three_term_cold_whole_u_and_literal_adjoint() -> Result {
	if std::env::var("QUEST_PERSISTED_WEIGHTED_TEST_CHILD").is_err() {
		let directory = std::env::temp_dir().join(format!(
			"quest-persisted-weighted-cold-{}",
			std::process::id()
		));
		std::fs::create_dir(&directory)?;
		for (ranks, mode) in [
			(8, "publish"),
			(1, "replay"),
			(2, "replay"),
			(4, "replay"),
			(8, "replay"),
			(4, "split"),
			(2, "admission"),
		] {
			let output = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(90),
			)?
			.args([
				"--exact",
				"immutable_persisted_three_term_cold_whole_u_and_literal_adjoint",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PERSISTED_WEIGHTED_TEST_CHILD", mode)
			.env("QUEST_PERSISTED_WEIGHTED_TEST_DIRECTORY", &directory)
			.output()?;
			assert!(
				output.status.success(),
				"{ranks}/{mode}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		std::fs::remove_dir_all(directory)?;
		return Ok(());
	}
	let mode = std::env::var("QUEST_PERSISTED_WEIGHTED_TEST_CHILD")?;
	let directory =
		std::path::PathBuf::from(std::env::var("QUEST_PERSISTED_WEIGHTED_TEST_DIRECTORY")?);
	let mpi = MpiRuntime::initialize()?;
	let mut world = mpi.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if mode == "split" {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(policy::RANK_BYTES))
		.build()?;
	if mode == "publish" {
		runtime::publish(&env, &directory, 4)?;
		return Ok(());
	}
	let reference_guard = env.reserve_external_bytes(1024 * 1024)?;
	let mut reference_terms = Vec::new();
	for term in 0..3 {
		reference_terms.push((
			Complex64::new(policy::WEIGHTS[term], 0.),
			oracle(&directory, term)?,
		));
	}
	let reference = WeightedLcu::new(reference_terms, PortfolioLimits::default())?;
	let (source, _) = runtime::lcu(&env, &directory, 4, 8)?;
	assert_eq!(source.plan().descriptor(), &reference.descriptor()?);
	let sequence =
		quest_qsp::PhaseSequence::<quest_qsp::WxSymmetric>::builder(vec![0.12, -0.3, -0.3, 0.12])
			.build()?;
	let schedule = TransformSchedule::from_phase_sequence(
		source.plan().descriptor().clone(),
		sequence.clone(),
		NumericalPolicy {
			max_bytes: policy::LOAD_BYTES,
		},
	)?;
	let portable = ReplayTransform::new(
		reference,
		sequence,
		NumericalPolicy {
			max_bytes: policy::LOAD_BYTES,
		},
	)?;
	let mut prepared =
		env.prepare_matching_lcu_transform(source, 4, schedule, runtime::transform_limits(parts))?;
	let mut state = env.state_vector_local(QubitCount::new(8)?)?;
	let initial: Vec<_> = (0..256)
		.map(|i| Complex64::new(f64::from(i % 13) - 6., f64::from(i % 7) - 3.))
		.collect();
	let norm = initial.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let initial: Vec<_> = initial.iter().map(|a| a / norm).collect();
	let local = state.deployment().local_amplitudes();
	let start = rank * local;
	if mode == "admission" {
		state.write_local_amplitudes(0, &initial[start..start + local])?;
		consumer_admission(&env, &state, &comm, &directory)?;
	}
	for adjoint in [false, true] {
		for value in [0, 32] {
			state.write_local_amplitudes(0, &initial[start..start + local])?;
			prepared.admit_apply(&state, adjoint, 32, value)?;
			prepared.apply(&mut state, adjoint, 32, value)?;
			let mut expected = initial.clone();
			portable.apply_mapped_reference(
				&mut expected,
				&[0, 7, 6, 1, 2, 3, 4],
				32,
				value,
				adjoint,
				NumericalPolicy {
					max_bytes: policy::LOAD_BYTES,
				},
			)?;
			let actual = state.read_local_amplitudes(0, local)?;
			for (a, b) in actual.iter().zip(&expected[start..start + local]) {
				assert!((*a - *b).norm() < 3e-12);
			}
			let receipt = prepared.last_apply_telemetry().ok_or("receipt")?;
			assert!(receipt.exact);
			assert_eq!(receipt.source_queries, 6);
			assert_eq!(receipt.child_events, 18);
			if parts > 1 {
				assert!(receipt.routing.point_to_point_sent_bytes > 0);
			}
		}
	}
	drop(reference_guard);
	Ok(())
}
