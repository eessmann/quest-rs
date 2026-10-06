//! Bounded subprocess checks of the actual wrapper's post-emission fatal boundary.
#![allow(
	clippy::panic_in_result_fn,
	clippy::too_many_lines,
	reason = "subprocess fixtures assert bounded rejection/abort outcomes and deliberately inject caught panics"
)]
#[allow(
	clippy::manual_assert,
	reason = "this helper deliberately injects either an error or panic"
)]
pub(super) fn inject(panic: bool) -> super::Result<()> {
	if panic {
		panic!("injected native weighted matching fault");
	}
	Err(crate::Error::Value("injected native weighted matching error").into())
}
use super::*;
use crate::collective::MpiRuntime;
use crate::qsvt::matching::preprocess::{ProducerLimits, produce_matching};
use quest_numerics::sparse_stream::SparseEntry;
use quest_qsvt::ReplayGate;
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
#[test]
fn weighted_matching_post_emission_errors_and_panics_abort_without_hanging() -> TestResult {
	if std::env::var("QUEST_LCU_FATAL_CHILD").is_err() {
		for ranks in [1, 2] {
			for mode in ["error", "panic"] {
				let output=quest_test_support::mpi::MpiTest::new(usize::try_from(ranks)?, std::time::Duration::from_secs(30))?
				.args(["--exact","qsvt::matching_lcu::collective::failure_tests::weighted_matching_post_emission_errors_and_panics_abort_without_hanging","--nocapture","--test-threads=1"])
				.env("QUEST_LCU_FATAL_CHILD",mode).output()?;
				assert!(!output.status.success());
				assert!(!output.status.timed_out);
				assert!(String::from_utf8(output.stdout)?.contains("LCU_POST_EMISSION_FAILURE"));
			}
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = world.duplicate()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	let count = QubitCount::new(6)?;
	let mut children = Vec::new();
	for weight in [1., 2., 3.] {
		let entries = (0..2_u64)
			.filter(|i| usize::try_from(*i).is_ok_and(|i| i.checked_rem(parts) == Some(rank)))
			.map(|i| {
				let index = usize::try_from(i).map_err(|_| quest_numerics::Error::Overflow)?;
				Ok(SparseEntry {
					row: index,
					column: index,
					ordinal: i,
					value: Complex64::new(1., 0.),
				})
			});
		let (shard, edges, reverse, _) =
			produce_matching(&comm, 2, 2, entries, ProducerLimits::default())?.into_parts();
		drop(edges);
		drop(reverse);
		children.push((
			Complex64::new(weight, 0.),
			env.prepare_matching(shard, count, vec![1, 0])?,
		));
	}
	let mut limits = MatchingLcuLimits::default();
	limits.plan.max_bytes = 1024 * 1024;
	limits.ranks_per_node = parts;
	let mut prepared = env.prepare_matching_lcu(children, vec![3, 2], limits)?;
	let mut state = env.state_vector_local(count)?;
	state.init_zero()?;
	if rank == 0 {
		prepared.test_post_emission_failure =
			Some(std::env::var("QUEST_LCU_FATAL_CHILD")? == "panic");
	}
	prepared.apply(&mut state, false, 0, 0)?;
	Err("post-emission fault unexpectedly returned".into())
}
#[test]
fn weighted_matching_local_partition_and_late_admission_reject_before_prep() -> TestResult {
	if std::env::var("QUEST_LCU_ADMISSION_CHILD").is_err() {
		for (ranks, mode) in [
			(2, "error"),
			(2, "panic"),
			(2, "compiled"),
			(2, "raw"),
			(2, "selectors"),
			(2, "budget"),
			(2, "live"),
			(2, "childcap"),
			(8, "partition"),
		] {
			let status=quest_test_support::mpi::MpiTest::new(usize::try_from(ranks)?, std::time::Duration::from_secs(40))?
				.args(["--exact","qsvt::matching_lcu::collective::failure_tests::weighted_matching_local_partition_and_late_admission_reject_before_prep","--nocapture","--test-threads=1"])
				.env("QUEST_LCU_ADMISSION_CHILD",mode).status()?;
			assert!(status.success());
		}
		return Ok(());
	}
	let mode = std::env::var("QUEST_LCU_ADMISSION_CHILD")?;
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = world.duplicate()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?.build()?;
	let count = QubitCount::new(if mode == "partition" { 3 } else { 6 })?;
	let mut children = Vec::new();
	for weight in [1., 2., 3.] {
		let entries = (rank == 0)
			.then_some(Ok(SparseEntry {
				row: 0,
				column: 0,
				ordinal: 0,
				value: Complex64::new(1., 0.),
			}))
			.into_iter();
		let (shard, edges, reverse, _) =
			produce_matching(&comm, 1, 1, entries, ProducerLimits::default())?.into_parts();
		drop(edges);
		drop(reverse);
		let child = if mode == "childcap" && weight > 2.5 {
			let probe = env.prepare_matching(shard.clone(), count, vec![0])?;
			let peak = env.view().allocated_bytes();
			drop(probe);
			let mut maximum = peak;
			let mut lane = comm.collective_lane()?;
			for peer in 0..parts {
				let mut bytes = u64::try_from(peak)?.to_le_bytes();
				lane.broadcast_bytes(i32::try_from(peer)?, &mut bytes)?;
				maximum = maximum.max(usize::try_from(u64::from_le_bytes(bytes))?);
			}
			drop(lane);
			env.prepare_matching_with_capacity(
				shard,
				count,
				vec![0],
				crate::qsvt::matching::collective::RoutingCapacity {
					ranks_per_node: parts,
					node_budget: crate::MemoryBudget::new(
						maximum
							.checked_add(512)
							.and_then(|n| n.checked_mul(parts))
							.ok_or(Error::Overflow)?,
					),
				},
			)?
		} else {
			env.prepare_matching(shard, count, vec![0])?
		};
		children.push((Complex64::new(weight, 0.), child));
	}
	let mut limits = MatchingLcuLimits::default();
	limits.plan.max_bytes = 1024 * 1024;
	limits.ranks_per_node = parts;
	limits.max_local_bytes = 4 * 1024 * 1024;
	if mode == "budget" {
		limits.max_rank_work = 0;
	}
	let mut state = env.state_vector_local(count)?;
	state.init_zero()?;
	if matches!(mode.as_str(), "raw" | "selectors") {
		if rank == 0 && mode == "raw" {
			children.last_mut().ok_or(Error::Value("test child"))?.0 = Complex64::new(-3., 0.);
		}
		let before = state.read_local_amplitudes(0, state.deployment().local_amplitudes())?;
		let selectors = if mode == "selectors" && rank == 0 {
			vec![0; 262_144]
		} else {
			vec![1, 2]
		};
		assert!(
			env.prepare_matching_lcu(children, selectors, limits)
				.is_err()
		);
		assert_eq!(
			state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
			before
		);
		return Ok(());
	}
	let mut prepared = env.prepare_matching_lcu(children, vec![1, 2], limits)?;
	let _external = if mode == "live" {
		Some(env.reserve_external_bytes(if rank == 0 { 4 * 1024 * 1024 } else { 0 })?)
	} else {
		None
	};
	let before = state.read_local_amplitudes(0, state.deployment().local_amplitudes())?;
	let bytes = env.view().allocated_bytes();
	if mode == "childcap" {
		for index in 0..2 {
			prepared
				.terms
				.get(index)
				.ok_or(Error::Value("test child"))?
				.1
				.admit_apply(&state, false, 0, 0)?;
		}
		assert!(
			prepared
				.terms
				.get(2)
				.ok_or(Error::Value("test child"))?
				.1
				.admit_apply(&state, false, 0, 0)
				.is_err()
		);
	}
	if mode == "compiled" {
		let descriptors = prepared
			.terms
			.iter()
			.enumerate()
			.map(|(i, (_, child))| {
				let weight = if rank == 0 && i == 2 {
					Complex64::new(0., 3.)
				} else {
					Complex64::new(3., 0.)
				};
				Ok((
					weight,
					EncodingDescriptor::from_matching_header(child.shard().header())?,
				))
			})
			.collect::<Result<Vec<_>>>()?;
		let divergent = LcuPlan::new(descriptors, limits.plan)?;
		let mut lane = env.begin(0x4c43_55f0, env.identifier(), 0, 0)?;
		assert!(compiled_equal(&mut lane, &divergent, prepared.targets()).is_err());
		drop(lane);
		assert_eq!(
			state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
			before
		);
		return Ok(());
	}
	if mode == "partition" {
		let mut executor = ReplayGateExecutor::with_resources(&env.resources, count)?;
		for kind in [ReplayKind::H, ReplayKind::Ry(0.2), ReplayKind::X] {
			assert!(
				executor
					.validate(
						&state.inner,
						ReplayGate {
							kind,
							target: Some(0),
							control_mask: 0,
							control_value: 0
						}
					)
					.is_err()
			);
		}
		executor.validate(
			&state.inner,
			ReplayGate {
				kind: ReplayKind::Phase(0.2),
				target: None,
				control_mask: 1,
				control_value: 1,
			},
		)?;
		assert!(
			executor
				.validate(
					&state.inner,
					ReplayGate {
						kind: ReplayKind::Phase(0.2),
						target: None,
						control_mask: 1,
						control_value: 0
					}
				)
				.is_err()
		);
		executor.apply(
			&mut state.inner,
			ReplayGate {
				kind: ReplayKind::Phase(0.2),
				target: None,
				control_mask: 1,
				control_value: 1,
			},
		)?;
		executor.apply(
			&mut state.inner,
			ReplayGate {
				kind: ReplayKind::Phase(-0.2),
				target: None,
				control_mask: 1,
				control_value: 1,
			},
		)?;
		drop(executor);
		// The color-free custom indexed matching route genuinely supports one local amplitude.
		prepared.terms[0].1.apply(&mut state, false, 0, 0)?;
		prepared.terms[0].1.apply(&mut state, true, 0, 0)?;
		state.init_zero()?;
		let entries = (0..3_u64)
			.filter(|i| usize::try_from(*i).is_ok_and(|i| i.checked_rem(parts) == Some(rank)))
			.map(|i| {
				Ok(SparseEntry {
					row: usize::from(i == 2),
					column: usize::from(i == 1),
					ordinal: i,
					value: Complex64::new(1., 0.),
				})
			});
		let (shard, edges, reverse, _) =
			produce_matching(&comm, 2, 2, entries, ProducerLimits::default())?.into_parts();
		drop(edges);
		drop(reverse);
		let colored = env.prepare_matching(shard, count, vec![0, 1, 2])?;
		assert!(colored.admit_apply(&state, false, 0, 0).is_err());
		drop(colored);
	} else if rank == 0 && matches!(mode.as_str(), "error" | "panic") {
		prepared.test_preflight_failure = Some(mode == "panic");
	}
	assert!(prepared.apply(&mut state, false, 0, 0).is_err());
	assert_eq!(
		state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
		before
	);
	assert_eq!(env.view().allocated_bytes(), bytes);
	Ok(())
}
