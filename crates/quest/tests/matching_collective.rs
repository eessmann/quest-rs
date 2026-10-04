#![cfg(all(feature = "qsvt", feature = "mpi", quest_native_mpi))]
use googletest::prelude::*;
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::{Complex64, MemoryBudget, QubitCount};
use quest_compile::{Control, ControlState, QuantumRegionBuilder};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsvt::{MatchingEncoding, MatchingShard, NumericalPolicy, materialize_program};
use std::ops::{Div, Mul, Sub};

#[gtest]
fn collective_matching_shards_route_all_sectors_without_global_storage() -> googletest::Result<()> {
	if std::env::var("QUEST_MATCHING_RANKS").is_err() {
		for (count, split) in [("1", "0"), ("2", "0"), ("4", "0"), ("4", "1"), ("8", "0")] {
			let status = std::process::Command::new("timeout")
				.args(["90s", "mpiexec", "-n", count])
				.arg(std::env::current_exe()?)
				.args([
					"--exact",
					"collective_matching_shards_route_all_sectors_without_global_storage",
					"--nocapture",
					"--test-threads=1",
				])
				.env("QUEST_MATCHING_RANKS", count)
				.env("QUEST_MATCHING_SPLIT", split)
				.status()?;
			expect_true!(status.success());
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	{
		let split = std::env::var("QUEST_MATCHING_SPLIT").as_deref() == Ok("1");
		let comm = if split && world.size()? >= 4 {
			world.split_power_of_two(2)?
		} else {
			world.duplicate()?
		};
		let environment = CollectiveEnvironment::builder(&comm)?
			.memory_budget(MemoryBudget::new(128 * 1024))
			.build()?;
		if comm.size()? > 1 {
			expect_true!(environment.state_vector(QubitCount::new(12)?).is_err());
			let partitioned = environment.state_vector_local(QubitCount::new(12)?)?;
			expect_eq!(
				partitioned.deployment().local_amplitudes(),
				4096usize
					.checked_div(usize::try_from(comm.size()?)?)
					.ok_or(quest::Error::Overflow)?
			);
			drop(partitioned);
		}
		let policy = NumericalPolicy::default();
		let matrix = SparseMatrix::from_triplets(
			2,
			3,
			SparseFormat::Csr,
			vec![
				(0, 0, Complex64::new(1.0, 1.0)),
				(0, 1, Complex64::new(-1.0, 0.0)),
				(0, 2, Complex64::new(0.0, 2.0)),
				(1, 0, Complex64::new(0.2, 0.0)),
			],
			SparseLimits::default(),
		)?;
		// Complete objects below are small explicit cold references for this differential.
		// The prepared execution receives only the owned partition and scalar header.
		let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
		let shard = MatchingShard::from_encoding(
			&encoding,
			usize::try_from(comm.rank()?)?,
			usize::try_from(comm.size()?)?,
			policy,
		)?;
		let targets = vec![2, 0, 4, 1, 5];
		let mut builder = QuantumRegionBuilder::new(9, 0)?;
		let mapped = targets
			.iter()
			.map(|&position| builder.qubit(position))
			.collect::<quest_compile::Result<Vec<_>>>()?;
		builder.oracle(
			&encoding.to_oracle(policy)?,
			&mapped,
			&[Control::new(builder.qubit(6)?, ControlState::One)],
		)?;
		let unitary = materialize_program(&builder.finish()?.bind(&[])?, policy)?;
		drop(encoding);
		drop(matrix);
		reject_malformed(&environment, &comm, &shard, &targets, policy)?;
		let mut prepared = environment.prepare_matching(shard, QubitCount::new(9)?, targets)?;
		exercise(&environment, &comm, &mut prepared, &unitary)?;
	}
	Ok(())
}

fn report_times(
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	local_times: [f64; 2],
	statistics: quest::qsvt::matching::collective::RoutingStatistics,
) -> googletest::Result<()> {
	let mut max_times = [0.0f64; 2];
	let mut sum_sent = 0u64;
	let mut sum_received = 0u64;
	let mut max_sent = 0u64;
	let mut max_received = 0u64;
	let mut lane = comm.collective_lane()?;
	for peer in 0..comm.size()? {
		let mut packet = [0u8; 32];
		packet[..8].copy_from_slice(&local_times[0].to_le_bytes());
		packet[8..16].copy_from_slice(&local_times[1].to_le_bytes());
		packet[16..24]
			.copy_from_slice(&u64::try_from(statistics.point_to_point_sent_bytes)?.to_le_bytes());
		packet[24..].copy_from_slice(
			&u64::try_from(statistics.point_to_point_received_bytes)?.to_le_bytes(),
		);
		lane.broadcast_bytes(peer, &mut packet)?;
		max_times[0] = max_times[0].max(f64::from_le_bytes(packet[..8].try_into()?));
		max_times[1] = max_times[1].max(f64::from_le_bytes(packet[8..16].try_into()?));
		let sent = u64::from_le_bytes(packet[16..24].try_into()?);
		let received = u64::from_le_bytes(packet[24..].try_into()?);
		sum_sent = sum_sent.checked_add(sent).ok_or(quest::Error::Overflow)?;
		sum_received = sum_received
			.checked_add(received)
			.ok_or(quest::Error::Overflow)?;
		max_sent = max_sent.max(sent);
		max_received = max_received.max(received);
	}
	drop(lane);
	expect_eq!(sum_sent, sum_received);
	if comm.rank()? == 0 {
		eprintln!(
			"matching parts={} max_scalar_pair_seconds={} max_batch_pair_seconds={} total_sent_bytes={} total_received_bytes={} max_sent_bytes={} max_received_bytes={} rank0_stats={:?}",
			comm.size()?,
			max_times[0],
			max_times[1],
			sum_sent,
			sum_received,
			max_sent,
			max_received,
			statistics
		);
	}

	Ok(())
}

fn exercise(
	environment: &CollectiveEnvironment<'_, '_>,
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	prepared: &mut quest::qsvt::matching::collective::PreparedMatching<'_, '_, '_>,
	unitary: &faer::Mat<Complex64>,
) -> googletest::Result<()> {
	let mut register = environment.state_vector_local(QubitCount::new(9)?)?;
	let local_count = register.deployment().local_amplitudes();
	let start = register
		.deployment()
		.rank()
		.checked_mul(local_count)
		.ok_or(quest::Error::Overflow)?;
	let state: Vec<_> = (0..512)
		.map(|i| Complex64::new(f64::from(i % 13) - 6.0, f64::from(i % 7) - 3.0))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|value| value.div(norm)).collect();
	let end = start
		.checked_add(local_count)
		.ok_or(quest::Error::Overflow)?;
	let local_state = state.get(start..end).ok_or(quest::Error::Overflow)?;
	let expected: Vec<_> = (start..end)
		.map(|row| {
			state
				.iter()
				.enumerate()
				.map(|(col, value)| unitary[(row, col)].mul(*value))
				.sum::<Complex64>()
		})
		.collect();
	register.init_zero()?;
	for (chunk, values) in local_state.chunks(8).enumerate() {
		register
			.write_local_amplitudes(chunk.checked_mul(8).ok_or(quest::Error::Overflow)?, values)?;
	}
	let bytes = environment.view().allocated_bytes();
	let invalid_mask = if comm.rank()? == 0 { 1 << 2 } else { 1 << 6 };
	expect_true!(
		prepared
			.apply(&mut register, false, invalid_mask, 0)
			.is_err()
	);
	let untouched = register.read_local_amplitudes(0, local_count)?;
	for (actual, expected) in untouched.iter().zip(local_state) {
		expect_true!(actual.sub(*expected).norm() < 1e-12);
	}
	let scalar_start = std::time::Instant::now();
	prepared.apply_scalar(&mut register, false, 1 << 6, 1 << 6)?;
	prepared.apply_scalar(&mut register, true, 1 << 6, 1 << 6)?;
	let scalar_time = scalar_start.elapsed();
	let batch_start = std::time::Instant::now();
	prepared.apply(&mut register, false, 1 << 6, 1 << 6)?;
	expect_eq!(prepared.last_statistics().batches, 2);
	expect_le!(prepared.last_statistics().maximum_batch_pairs, 64);
	expect_le!(prepared.last_statistics().maximum_routed_amplitudes, 128);
	let actual = register.read_local_amplitudes(0, local_count)?;
	for (actual, expected) in actual.iter().zip(&expected) {
		expect_true!(actual.sub(*expected).norm() < 1e-12);
	}
	prepared.apply(&mut register, true, 1 << 6, 1 << 6)?;
	let batch_time = batch_start.elapsed();
	report_times(
		comm,
		[scalar_time.as_secs_f64(), batch_time.as_secs_f64()],
		prepared.last_statistics(),
	)?;

	let actual = register.read_local_amplitudes(0, local_count)?;
	for (actual, expected) in actual.iter().zip(local_state) {
		expect_true!(actual.sub(*expected).norm() < 1e-12);
	}
	expect_eq!(environment.view().allocated_bytes(), bytes);
	Ok(())
}

fn reject_malformed(
	environment: &CollectiveEnvironment<'_, '_>,
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	shard: &MatchingShard,
	targets: &[usize],
	policy: NumericalPolicy,
) -> googletest::Result<()> {
	let mut changed = shard.records().to_vec();
	if comm.rank()? == 0
		&& let Some(first) = changed.first_mut()
	{
		first.phase = first.phase.mul(-1.0);
	}
	if shard.parts() == 1 {
		expect_true!(MatchingShard::from_parts(shard.header(), 0, 1, changed, policy).is_err());
		expect_true!(MatchingShard::from_parts(shard.header(), 0, 1, vec![], policy).is_err());
		return Ok(());
	}
	let changed =
		MatchingShard::from_parts(shard.header(), shard.rank(), shard.parts(), changed, policy)?;
	expect_true!(
		environment
			.prepare_matching(changed, QubitCount::new(9)?, targets.to_vec())
			.is_err()
	);
	let dropped =
		MatchingShard::from_parts(shard.header(), shard.rank(), shard.parts(), vec![], policy)?;
	expect_true!(
		environment
			.prepare_matching(dropped, QubitCount::new(9)?, targets.to_vec())
			.is_err()
	);
	let mut mismatched_header = shard.header();
	if comm.rank()? == 0 {
		mismatched_header.source_identity ^= 1;
	}
	let mismatched = MatchingShard::from_parts(
		mismatched_header,
		shard.rank(),
		shard.parts(),
		shard.records().to_vec(),
		policy,
	)?;
	if comm.size()? > 1 {
		expect_true!(
			environment
				.prepare_matching(mismatched, QubitCount::new(9)?, targets.to_vec())
				.is_err()
		);
	}
	let mut malformed_records = shard.records().to_vec();
	if comm.rank()? == 0
		&& let Some(first) = malformed_records.first_mut()
	{
		first.destination = 3;
	}
	let malformed = MatchingShard::from_parts(
		shard.header(),
		shard.rank(),
		shard.parts(),
		malformed_records,
		policy,
	)?;
	let malformed = rebind_summary(comm, &malformed, policy)?;
	expect_true!(
		environment
			.prepare_matching(malformed, QubitCount::new(9)?, targets.to_vec())
			.is_err()
	);

	Ok(())
}

fn rebind_summary(
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	shard: &MatchingShard,
	policy: NumericalPolicy,
) -> googletest::Result<MatchingShard> {
	let own = shard.payload_summary()?;
	let mut header = shard.header();
	header.record_count = 0;
	header.record_digest = 0;
	let mut lane = comm.collective_lane()?;
	for peer in 0..comm.size()? {
		let mut packet = [0u8; 16];
		packet[..8].copy_from_slice(&u64::try_from(own.0)?.to_le_bytes());
		packet[8..].copy_from_slice(&own.1.to_le_bytes());
		lane.broadcast_bytes(peer, &mut packet)?;
		header.record_count = header
			.record_count
			.checked_add(usize::try_from(u64::from_le_bytes(
				packet[..8].try_into()?,
			))?)
			.ok_or(quest::Error::Overflow)?;
		header.record_digest = header
			.record_digest
			.wrapping_add(u64::from_le_bytes(packet[8..].try_into()?));
	}
	Ok(MatchingShard::from_parts(
		header,
		shard.rank(),
		shard.parts(),
		shard.records().to_vec(),
		policy,
	)?)
}
