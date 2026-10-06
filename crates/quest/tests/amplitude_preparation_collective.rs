#![cfg(all(feature = "qsvt", feature = "mpi", quest_native_mpi))]
use googletest::prelude::*;
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::qsvt::amplitude_preparation::{
	AmplitudeShard, DistributedPreparationLimits, PreparationOutcome,
};
use quest::{Complex64, QubitCount};
use quest_qsvt::{
	ReplayGate, ReplayKind,
	state_preparation::{AmplitudePreparation, PreparationLimits},
};
use std::ops::{Add, Div, Mul, Sub};
fn rhs(index: usize) -> quest::Result<Complex64> {
	Ok(Complex64::new(
		f64::from(u32::try_from(index % 7).map_err(|_| quest::Error::Overflow)?) - 3.,
		f64::from(u32::try_from(index % 5).map_err(|_| quest::Error::Overflow)?) - 2.,
	))
}
#[allow(
	clippy::indexing_slicing,
	reason = "The independent bounded reference has a fixed power-of-two state and verified mapped targets"
)]
fn reference(state: &mut [Complex64], gate: ReplayGate) {
	let phase = match gate.kind {
		ReplayKind::Phase(a) => Some(Complex64::from_polar(1., a)),
		_ => None,
	};
	if let Some(phase) = phase {
		for (index, value) in state.iter_mut().enumerate() {
			if index & gate.control_mask == gate.control_value {
				*value = value.mul(phase);
			}
		}
		return;
	}
	let target = gate.target.unwrap_or(0);
	for left in 0..state.len() {
		if left & (1 << target) != 0 || left & gate.control_mask != gate.control_value {
			continue;
		}
		let right = left | (1 << target);
		let (a, b) = (state[left], state[right]);
		let (c, s) = match gate.kind {
			ReplayKind::Ry(angle) => ((0.5 * angle).cos(), (0.5 * angle).sin()),
			_ => (0., 1.),
		};
		(state[left], state[right]) = match gate.kind {
			ReplayKind::Ry(_) => (a.mul(c).sub(b.mul(s)), a.mul(s).add(b.mul(c))),
			_ => (b, a),
		};
	}
}
#[gtest]
fn sharded_preparation_matches_complex_whole_unitary_and_adjoint() -> googletest::Result<()> {
	if std::env::var("QUEST_PREPARATION_RANKS").is_err() {
		for (ranks, split) in [("1", "0"), ("2", "0"), ("4", "0"), ("8", "0"), ("4", "1")] {
			let status = quest_test_support::mpi::MpiTest::new(
				ranks.parse()?,
				std::time::Duration::from_secs(90),
			)?
			.args([
				"--exact",
				"sharded_preparation_matches_complex_whole_unitary_and_adjoint",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_PREPARATION_RANKS", ranks)
			.env("QUEST_PREPARATION_SPLIT", split)
			.status()?;
			expect_true!(status.success());
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_PREPARATION_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	{
		let env = CollectiveEnvironment::builder(&comm)?.build()?;
		let rank = usize::try_from(comm.rank()?)?;
		let parts = usize::try_from(comm.size()?)?;
		let group = if std::env::var("QUEST_PREPARATION_SPLIT").as_deref() == Ok("1") {
			usize::try_from(world.rank()?)?
				.checked_div(2)
				.ok_or(quest::Error::Overflow)?
		} else {
			0
		};
		let input = |index: usize| {
			rhs(index
				.checked_add(group.checked_mul(17).ok_or(quest::Error::Overflow)?)
				.ok_or(quest::Error::Overflow)?)
		};
		let limits = DistributedPreparationLimits {
			chunk_elements: 1,
			..DistributedPreparationLimits::default()
		};
		let before = env.view().allocated_bytes();
		let mut generated = 0usize;
		let mut prepared = match env.prepare_amplitudes_from_fn(13, limits, |index| {
			generated = generated.saturating_add(1);
			input(index)
		})? {
			PreparationOutcome::Prepared(p) => p,
			PreparationOutcome::Zero { .. } => {
				return Err(quest::Error::Value("unexpected zero norm").into());
			}
		};
		let extent = 16usize.checked_div(parts).ok_or(quest::Error::Overflow)?;
		let local_start = rank.checked_mul(extent).ok_or(quest::Error::Overflow)?;
		expect_eq!(generated, 13usize.saturating_sub(local_start).min(extent));
		if parts > 1 {
			expect_lt!(generated, 13);
		}
		let cold = (0..13).map(input).collect::<quest::Result<Vec<_>>>()?;
		let model = AmplitudePreparation::new(&cold, PreparationLimits::default())?;
		expect_that!(prepared.norm(), near(model.norm(), 1e-12));
		expect_eq!(
			prepared.resources().elementary_gates,
			model.resources().elementary_gates
		);
		expect_eq!(prepared.resources().max_packet_bytes, 16);
		expect_eq!(
			prepared.resources().butterfly_sent_bytes,
			prepared
				.resources()
				.butterfly_messages
				.checked_mul(16)
				.ok_or(quest::Error::Overflow)?
		);
		expect_eq!(prepared.resources().coefficient_queries, 30);
		if parts > 1 {
			expect_lt!(prepared.resources().local_coefficients, 30);
			expect_gt!(prepared.resources().butterfly_messages, 0);
		}
		let targets = [5, 0, 3, 1];
		let mut state = env.state_vector_local(QubitCount::new(6)?)?;
		let local = state.deployment().local_amplitudes();
		let start = local.checked_mul(rank).ok_or(quest::Error::Overflow)?;
		let end = start.checked_add(local).ok_or(quest::Error::Overflow)?;
		state.init_zero()?;
		prepared.apply(&mut state, &targets, 1 << 4, 0, false)?;
		let actual = state.read_local_amplitudes(0, local)?;
		for (offset, a) in actual.iter().enumerate() {
			let row = start.checked_add(offset).ok_or(quest::Error::Overflow)?;
			let mut logical = 0usize;
			for (bit, target) in targets.iter().enumerate() {
				logical |= ((row >> target) & 1) << bit;
			}
			let expected = if row & ((1 << 4) | (1 << 2)) == 0 {
				cold.get(logical)
					.copied()
					.unwrap_or_default()
					.div(model.norm())
			} else {
				Complex64::new(0., 0.)
			};
			expect_true!(a.sub(expected).norm() < 1e-12);
		}
		let full = (0..64).map(rhs).collect::<quest::Result<Vec<_>>>()?;
		let norm = full.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
		let original: Vec<_> = full.into_iter().map(|v| v.div(norm)).collect();
		state.write_local_amplitudes(0, original.get(start..end).ok_or(quest::Error::Overflow)?)?;
		let mut expected = original.clone();
		model.visit_mapped_gates(&targets, 1 << 4, 0, false, &mut |gate| {
			reference(&mut expected, gate);
			Ok(())
		})?;
		let bytes = env.view().allocated_bytes();
		prepared.admit_apply(&state, &targets, 1 << 4, 0, false)?;
		expect_eq!(env.view().allocated_bytes(), bytes);
		expect_eq!(
			state.read_local_amplitudes(0, local)?,
			original.get(start..end).ok_or(quest::Error::Overflow)?
		);
		prepared.apply(&mut state, &targets, 1 << 4, 0, false)?;
		for (a, b) in state
			.read_local_amplitudes(0, local)?
			.iter()
			.zip(expected.get(start..end).ok_or(quest::Error::Overflow)?)
		{
			expect_true!(a.sub(*b).norm() < 1e-12);
		}
		prepared.apply(&mut state, &targets, 1 << 4, 0, true)?;
		for (a, b) in state
			.read_local_amplitudes(0, local)?
			.iter()
			.zip(original.get(start..end).ok_or(quest::Error::Overflow)?)
		{
			expect_true!(a.sub(*b).norm() < 1e-12);
		}
		let mut expected = original.clone();
		model.visit_mapped_gates(&targets, 1 << 4, 1 << 4, false, &mut |gate| {
			reference(&mut expected, gate);
			Ok(())
		})?;
		prepared.apply(&mut state, &targets, 1 << 4, 1 << 4, false)?;
		for (a, b) in state
			.read_local_amplitudes(0, local)?
			.iter()
			.zip(expected.get(start..end).ok_or(quest::Error::Overflow)?)
		{
			expect_true!(a.sub(*b).norm() < 1e-12);
		}
		prepared.apply(&mut state, &targets, 1 << 4, 1 << 4, true)?;
		for (a, b) in state
			.read_local_amplitudes(0, local)?
			.iter()
			.zip(original.get(start..end).ok_or(quest::Error::Overflow)?)
		{
			expect_true!(a.sub(*b).norm() < 1e-12);
		}
		expect_eq!(env.view().allocated_bytes(), bytes);
		drop(state);
		drop(prepared);
		expect_eq!(env.view().allocated_bytes(), before);
	}
	Ok(())
}
fn child(name: &str) -> googletest::Result<bool> {
	if std::env::var("QUEST_PREPARATION_CASE").as_deref() == Ok(name) {
		return Ok(true);
	}
	let status = quest_test_support::mpi::MpiTest::new(4, std::time::Duration::from_secs(60))?
		.args(["--exact", name, "--nocapture", "--test-threads=1"])
		.env("QUEST_PREPARATION_CASE", name)
		.status()?;
	expect_true!(status.success());
	Ok(false)
}
#[gtest]
fn distributed_preparation_admission_is_collective_and_preserves_live_state()
-> googletest::Result<()> {
	if !child("distributed_preparation_admission_is_collective_and_preserves_live_state")? {
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	{
		let env = CollectiveEnvironment::builder(&comm)?.build()?;
		let rank = usize::try_from(comm.rank()?)?;
		let parts = usize::try_from(comm.size()?)?;
		let before = env.view().allocated_bytes();
		expect_true!(
			env.prepare_amplitudes_from_fn(13, DistributedPreparationLimits::default(), |index| {
				if rank == 0 {
					Err(quest::Error::Allocation)
				} else {
					rhs(index)
				}
			})
			.is_err()
		);
		expect_eq!(env.view().allocated_bytes(), before);
		let zero = AmplitudeShard::from_fn(13, rank, parts, 1024, |_| Ok(Complex64::new(0., 0.)))?;
		expect_true!(matches!(
			env.prepare_amplitudes(zero, DistributedPreparationLimits::default())?,
			PreparationOutcome::Zero { .. }
		));
		expect_eq!(env.view().allocated_bytes(), before);
		for (logical, owner) in [(if rank == 0 { 13 } else { 14 }, rank), (13, 0)] {
			let shard = AmplitudeShard::from_fn(logical, owner, parts, 1024, rhs)?;
			expect_true!(
				env.prepare_amplitudes(shard, DistributedPreparationLimits::default())
					.is_err()
			);
			expect_eq!(env.view().allocated_bytes(), before);
		}
		let default = DistributedPreparationLimits::default();
		for limits in [
			DistributedPreparationLimits {
				max_dimension: 8,
				..default
			},
			DistributedPreparationLimits {
				max_compile_work: 0,
				..default
			},
			DistributedPreparationLimits {
				max_gates: 0,
				..default
			},
			DistributedPreparationLimits {
				max_query_work: 0,
				..default
			},
			DistributedPreparationLimits {
				max_transport_bytes: 0,
				..default
			},
			DistributedPreparationLimits {
				max_local_bytes: 1,
				..default
			},
			DistributedPreparationLimits {
				chunk_elements: 0,
				..default
			},
			DistributedPreparationLimits {
				chunk_elements: usize::MAX,
				..default
			},
			DistributedPreparationLimits {
				node_budget: quest::MemoryBudget::new(0),
				..default
			},
		] {
			expect_true!(
				env.prepare_amplitudes(
					AmplitudeShard::from_fn(13, rank, parts, 1024, rhs)?,
					limits
				)
				.is_err()
			);
			expect_eq!(env.view().allocated_bytes(), before);
		}
		let start = rank.checked_mul(4).ok_or(quest::Error::Overflow)?;
		let count = 13usize.saturating_sub(start).min(4);
		let mut values = Vec::with_capacity(if rank == 0 { 1024 } else { count });
		for index in start..start.checked_add(count).ok_or(quest::Error::Overflow)? {
			values.push(rhs(index)?);
		}
		let inflated = AmplitudeShard::from_parts(13, 16, rank, parts, start, values)?;
		expect_true!(
			env.prepare_amplitudes(
				inflated,
				DistributedPreparationLimits {
					max_local_bytes: 12_000,
					..default
				}
			)
			.is_err()
		);
		expect_eq!(env.view().allocated_bytes(), before);
		let mut state = env.state_vector_local(QubitCount::new(6)?)?;
		state.init_plus()?;
		let local = state.deployment().local_amplitudes();
		let original = state.read_local_amplitudes(0, local)?;
		let PreparationOutcome::Prepared(mut prepared) = env.prepare_amplitudes(
			AmplitudeShard::from_fn(13, rank, parts, 1024, rhs)?,
			default,
		)?
		else {
			return Err(quest::Error::Value("unexpected zero").into());
		};
		for (targets, mask, value, adjoint) in [
			([0, 1, 2, if rank == 0 { 2 } else { 3 }], 0, 0, false),
			([0, 1, 2, 3], 0, usize::from(rank == 0), false),
			([0, 1, 2, 3], 0, 0, rank == 0),
			([0, 1, 2, if rank == 0 { 5 } else { 3 }], 0, 0, false),
		] {
			let bytes = env.view().allocated_bytes();
			expect_true!(
				prepared
					.admit_apply(&state, &targets, mask, value, adjoint)
					.is_err()
			);
			expect_eq!(env.view().allocated_bytes(), bytes);
			expect_eq!(state.read_local_amplitudes(0, local)?, original);
			expect_true!(
				prepared
					.apply(&mut state, &targets, mask, value, adjoint)
					.is_err()
			);
			expect_eq!(env.view().allocated_bytes(), bytes);
			expect_eq!(state.read_local_amplitudes(0, local)?, original);
		}
		drop(prepared);
		for limits in [
			DistributedPreparationLimits {
				max_native_dispatches: 0,
				..default
			},
			DistributedPreparationLimits {
				max_query_work: 816,
				..default
			},
		] {
			let PreparationOutcome::Prepared(mut prepared) = env
				.prepare_amplitudes(AmplitudeShard::from_fn(13, rank, parts, 1024, rhs)?, limits)?
			else {
				return Err(quest::Error::Value("unexpected zero").into());
			};
			let bytes = env.view().allocated_bytes();
			expect_true!(
				prepared
					.admit_apply(&state, &[0, 1, 2, 3], 0, 0, false)
					.is_err()
			);
			expect_eq!(env.view().allocated_bytes(), bytes);
			expect_eq!(state.read_local_amplitudes(0, local)?, original);
			expect_true!(
				prepared
					.apply(&mut state, &[0, 1, 2, 3], 0, 0, false)
					.is_err()
			);
			expect_eq!(env.view().allocated_bytes(), bytes);
			expect_eq!(state.read_local_amplitudes(0, local)?, original);
		}
		drop(state);
		let PreparationOutcome::Prepared(probe) = env.prepare_amplitudes(
			AmplitudeShard::from_fn(13, rank, parts, 1024, rhs)?,
			default,
		)?
		else {
			return Err(quest::Error::Value("unexpected zero").into());
		};
		let own_peak = probe.resources().construction_peak_bytes;
		drop(probe);
		let mut maximum = 0usize;
		{
			let mut lane = comm.collective_lane()?;
			for peer in 0..comm.size()? {
				let mut packet = u64::try_from(own_peak)?.to_le_bytes();
				lane.broadcast_bytes(peer, &mut packet)?;
				maximum = maximum.max(usize::try_from(u64::from_le_bytes(packet))?);
			}
		}
		let limits = DistributedPreparationLimits {
			node_budget: quest::MemoryBudget::new(
				maximum.checked_mul(parts).ok_or(quest::Error::Overflow)?,
			),
			..default
		};
		let PreparationOutcome::Prepared(mut prepared) =
			env.prepare_amplitudes(AmplitudeShard::from_fn(13, rank, parts, 1024, rhs)?, limits)?
		else {
			return Err(quest::Error::Value("unexpected zero").into());
		};
		let mut state = env.state_vector_local(QubitCount::new(12)?)?;
		state.init_zero()?;
		let original = state.read_local_amplitudes(0, 16)?;
		let bytes = env.view().allocated_bytes();
		expect_true!(
			prepared
				.admit_apply(&state, &[0, 1, 2, 3], 0, 0, false)
				.is_err()
		);
		expect_eq!(env.view().allocated_bytes(), bytes);
		expect_eq!(state.read_local_amplitudes(0, 16)?, original);
		expect_true!(
			prepared
				.apply(&mut state, &[0, 1, 2, 3], 0, 0, false)
				.is_err()
		);
		expect_eq!(env.view().allocated_bytes(), bytes);
		expect_eq!(state.read_local_amplitudes(0, 16)?, original);
		drop(state);
		drop(prepared);
		expect_eq!(env.view().allocated_bytes(), before);
	}
	Ok(())
}
#[gtest]
fn local_input_creation_rejects_malformed_ownership_and_storage() {
	expect_true!(AmplitudeShard::from_fn(0, 0, 1, 1024, rhs).is_err());
	expect_true!(AmplitudeShard::from_fn(13, 0, 3, 1024, rhs).is_err());
	expect_true!(AmplitudeShard::from_fn(1, 0, 2, 1024, rhs).is_err());
	expect_true!(AmplitudeShard::from_fn(13, 4, 4, 1024, rhs).is_err());
	expect_true!(AmplitudeShard::from_fn(13, 0, 4, 0, rhs).is_err());
	expect_true!(
		AmplitudeShard::from_fn(13, 0, 4, 1024, |_| Ok(Complex64::new(f64::NAN, 0.))).is_err()
	);
	expect_true!(AmplitudeShard::from_parts(13, 32, 0, 4, 0, Vec::new()).is_err());
	expect_true!(
		AmplitudeShard::from_parts(13, 16, 0, 4, 1, vec![Complex64::new(1., 0.); 4]).is_err()
	);
}
#[gtest]
fn distributed_preparation_aborts_after_visitor_emission_failure() -> googletest::Result<()> {
	let name = "distributed_preparation_aborts_after_visitor_emission_failure";
	if std::env::var("QUEST_PREPARATION_FATAL").is_err() {
		let witnesses = quest_test_support::witness::WitnessDirectory::new()?;
		let status = quest_test_support::mpi::MpiTest::new(2, std::time::Duration::from_secs(20))?
			.args(["--exact", name, "--nocapture", "--test-threads=1"])
			.env("QUEST_PREPARATION_FATAL", "1")
			.env("QUEST_PREPARATION_FATAL_WITNESSES", witnesses.path())
			.status()?;
		expect_false!(status.success());
		expect_false!(status.timed_out);
		for rank in 0..2 {
			expect_eq!(
				std::fs::read(witnesses.path().join(format!("prepared-{rank}")))?,
				b"prepared before visitor execution\n".to_vec()
			);
		}
		expect_eq!(
			std::fs::read(witnesses.path().join("visitor-failure"))?,
			b"rank zero rejected second emission\n".to_vec()
		);
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let PreparationOutcome::Prepared(mut prepared) = env.prepare_amplitudes(
		AmplitudeShard::from_fn(13, rank, parts, 1024, rhs)?,
		DistributedPreparationLimits::default(),
	)?
	else {
		return Err(quest::Error::Value("unexpected zero").into());
	};
	let directory = std::path::PathBuf::from(
		std::env::var_os("QUEST_PREPARATION_FATAL_WITNESSES")
			.ok_or_else(|| std::io::Error::other("missing fatal witness directory"))?,
	);
	quest_test_support::witness::write(
		&directory,
		&format!("prepared-{rank}"),
		b"prepared before visitor execution\n",
	)?;
	let mut lane = comm.collective_lane()?;
	expect_true!(lane.all_agree(true)?);
	drop(lane);
	let mut emitted = 0usize;
	prepared.visit_mapped_gates(&[0, 1, 2, 3], 0, 0, false, &mut |_| {
		emitted = emitted.saturating_add(1);
		if rank == 0 && emitted == 2 {
			quest_test_support::witness::write(
				&directory,
				"visitor-failure",
				b"rank zero rejected second emission\n",
			)
			.map_err(|_| {
				quest_qsvt::Error::Encoding("could not persist visitor failure witness")
			})?;
			Err(quest_qsvt::Error::Encoding(
				"injected visitor failure after emission",
			))
		} else {
			Ok(())
		}
	})?;
	Err(quest::Error::Value("fatal boundary unexpectedly recovered").into())
}
