//! Actual owner subprocesses: common unchanged-state rejection and bounded fatal replay.
#![allow(
	clippy::too_many_lines,
	clippy::panic_in_result_fn,
	reason = "Bounded subprocess fixtures deliberately inject caught errors/panics and compare preflight state"
)]
use super::*;
use crate::qsvt::{
	matching::preprocess::{ProducerLimits, produce_matching},
	matching_lcu::MatchingLcuLimits,
};
use crate::{Complex64, QubitCount, collective::MpiRuntime};
use quest_numerics::sparse_stream::SparseEntry;
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::NumericalPolicy;
use std::ops::Mul;
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
#[allow(
	clippy::manual_assert,
	reason = "Intentional error/panic injection exercises both failure contracts"
)]
pub(super) fn inject(panic: bool) -> Result<()> {
	if panic {
		panic!("injected transform fault");
	}
	Err(Error::Value("injected transform error").into())
}
#[test]
fn transform_faults_reject_before_h_or_abort_after_emission() -> TestResult {
	if std::env::var("QUEST_TRANSFORM_FAULT_CHILD").is_err() {
		for (ranks, mode) in [
			(2, "error"),
			(2, "panic"),
			(2, "compiled"),
			(2, "overflow"),
			(2, "querybudget"),
			(2, "workbudget"),
			(2, "dispatchbudget"),
			(2, "payloadbudget"),
			(2, "live"),
			(2, "childcap"),
			(2, "constructorcap"),
			(2, "response"),
			(8, "partition"),
			(1, "posterror"),
			(2, "posterror"),
			(1, "postpanic"),
			(2, "postpanic"),
			(1, "lateerror"),
			(2, "lateerror"),
			(1, "latepanic"),
			(2, "latepanic"),
		] {
			let output=quest_test_support::mpi::MpiTest::new(usize::try_from(ranks)?, std::time::Duration::from_secs(35))?.args(["--exact","qsvt::matching_lcu_transform::collective::failure_tests::transform_faults_reject_before_h_or_abort_after_emission","--nocapture","--test-threads=1"]).env("QUEST_TRANSFORM_FAULT_CHILD",mode).output()?;
			if mode.starts_with("post") || mode.starts_with("late") {
				assert!(!output.status.success());
				assert!(!output.status.timed_out);
				assert!(
					String::from_utf8(output.stdout)?.contains(if mode.starts_with("late") {
						"LCU_TRANSFORM_LATE_QUERY_FAILURE"
					} else {
						"LCU_TRANSFORM_POST_EMISSION_FAILURE"
					})
				);
			} else {
				assert!(
					output.status.success(),
					"{mode}: {}",
					String::from_utf8(output.stderr)?
				);
			}
		}
		return Ok(());
	}
	let mode = std::env::var("QUEST_TRANSFORM_FAULT_CHILD")?;
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = world.duplicate()?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(crate::MemoryBudget::new(if mode == "overflow" {
			usize::MAX
		} else {
			64 * 1024 * 1024
		}))
		.build()?;
	let count = QubitCount::new(if mode == "partition" { 3 } else { 6 })?;
	let mut children = Vec::new();
	for weight in if mode == "partition" {
		vec![1.]
	} else {
		vec![1., 2., 3.]
	} {
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
			let own = env.view().allocated_bytes();
			drop(probe);
			let mut maximum = own;
			let mut lane = comm.collective_lane()?;
			for peer in 0..parts {
				let mut bytes = u64::try_from(own)?.to_le_bytes();
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
	let source = env.prepare_matching_lcu(
		children,
		if mode == "partition" {
			vec![]
		} else {
			vec![1, 2]
		},
		MatchingLcuLimits {
			plan: quest_qsvt::portfolio::LcuPlanLimits {
				max_bytes: 1024 * 1024,
				..Default::default()
			},
			ranks_per_node: parts,
			..Default::default()
		},
	)?;
	let values = if mode == "compiled" && rank == 0 {
		vec![0.1, 0.2, 0.2, 0.1]
	} else {
		vec![0., 0., 0., 0.]
	};
	let schedule = TransformSchedule::from_phase_sequence(
		source.plan().descriptor().clone(),
		PhaseSequence::<WxSymmetric>::builder(values).build()?,
		NumericalPolicy::default(),
	)?;
	let mut state = env.state_vector_local(count)?;
	let local = state.deployment().local_amplitudes();
	let base = rank.checked_mul(local).ok_or(Error::Overflow)?;
	let magnitude = f64::from(u32::try_from(count.dimension())?).sqrt().recip();
	let values = (0..local)
		.map(|i| -> std::result::Result<_, crate::Error> {
			let index = base.checked_add(i).ok_or(Error::Overflow)?;
			Ok(Complex64::from_polar(
				magnitude,
				f64::from(u32::try_from(index).map_err(|_| Error::Overflow)?).mul(0.13),
			))
		})
		.collect::<crate::Result<Vec<_>>>()?;
	state.write_local_amplitudes(0, &values)?;
	let before = state.read_local_amplitudes(0, state.deployment().local_amplitudes())?;
	let mut limits = TransformExecutionLimits {
		ranks_per_node: parts,
		max_local_bytes: 4 * 1024 * 1024,
		..Default::default()
	};
	match mode.as_str() {
		"constructorcap" => limits.max_local_bytes = 0,
		"querybudget" => limits.max_queries = 5,
		"workbudget" => limits.max_preflight_work = 0,
		"dispatchbudget" => limits.max_native_dispatches = 0,
		"payloadbudget" => limits.max_application_bytes = 0,
		_ => {}
	}
	let _construction_external = if mode == "overflow" {
		Some(env.reserve_external_bytes(if rank == 0 {
			usize::MAX
				.checked_div(2)
				.and_then(|n| n.checked_add(1024))
				.ok_or(Error::Overflow)?
		} else {
			0
		})?)
	} else {
		None
	};
	if mode == "overflow" {
		limits.max_local_bytes = usize::MAX;
		limits.node_budget = crate::MemoryBudget::new(usize::MAX);
		limits.ranks_per_node = if rank == 0 { 1 } else { 2 };
	}
	let response = if mode == "partition" {
		1
	} else if mode == "response" && rank == 0 {
		0
	} else {
		3
	};
	if matches!(
		mode.as_str(),
		"compiled" | "constructorcap" | "response" | "overflow"
	) {
		assert!(
			env.prepare_matching_lcu_transform(source, response, schedule, limits)
				.is_err()
		);
		assert_eq!(
			state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
			before
		);
		return Ok(());
	}
	let mut prepared = env.prepare_matching_lcu_transform(source, response, schedule, limits)?;
	let _external = if mode == "live" {
		Some(env.reserve_external_bytes(if rank == 0 { 4 * 1024 * 1024 } else { 0 })?)
	} else {
		None
	};
	let bytes = env.view().allocated_bytes();
	if rank == 0 {
		match mode.as_str() {
			"error" | "panic" => prepared.test_preflight_failure = Some(mode == "panic"),
			"posterror" | "postpanic" => {
				prepared.test_post_emission_failure = Some(mode == "postpanic");
			}
			"lateerror" | "latepanic" => prepared.test_query_failure = Some(mode == "latepanic"),
			_ => {}
		}
	}
	let result = prepared.apply(&mut state, false, 0, 0);
	assert!(result.is_err());
	assert_eq!(
		state.read_local_amplitudes(0, state.deployment().local_amplitudes())?,
		before
	);
	assert_eq!(env.view().allocated_bytes(), bytes);
	Ok(())
}
