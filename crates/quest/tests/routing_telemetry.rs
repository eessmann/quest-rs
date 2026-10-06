#![cfg(all(feature = "qsvt", feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::panic_in_result_fn,
	reason = "Bounded MPI routing counters are compared against independently dispatched native child events"
)]
use quest::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
	qsvt::{
		matching::{
			collective::{PreparedMatching, RoutingStatistics},
			preprocess::{ProducerLimits, produce_matching},
		},
		matching_lcu::MatchingLcuLimits,
		matching_lcu_transform::TransformExecutionLimits,
	},
};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{
	NumericalPolicy,
	portfolio::LcuStep,
	replay_transform::{TransformSchedule, TransformStep},
};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn children<'env, 'comm, 'runtime>(
	environment: &'env CollectiveEnvironment<'comm, 'runtime>,
) -> TestResult<Vec<(Complex64, PreparedMatching<'env, 'comm, 'runtime>)>> {
	let comm = environment.communicator();
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let mut children = Vec::new();
	for weight in [
		Complex64::new(0.25, 0.0),
		Complex64::new(0.0, 0.0),
		Complex64::new(-0.125, 0.0),
		Complex64::new(0.0, 0.25),
	] {
		let input = (0..4)
			.filter(|column: &usize| column.checked_rem(parts) == Some(rank))
			.flat_map(|column| {
				[
					(column, Complex64::new(1.0, 0.0)),
					(column ^ 1, Complex64::new(0.0, 1.0)),
				]
				.into_iter()
				.enumerate()
				.map(move |(edge, (row, value))| {
					let ordinal = column
						.checked_mul(2)
						.and_then(|n| n.checked_add(edge))
						.and_then(|n| u64::try_from(n).ok())
						.ok_or(quest_numerics::Error::Overflow)?;
					Ok(quest_numerics::sparse_stream::SparseEntry {
						row,
						column,
						ordinal,
						value,
					})
				})
			});
		let produced = produce_matching(comm, 4, 4, input, ProducerLimits::default())?;
		let (shard, edges, reverse, _) = produced.into_parts();
		drop(edges);
		drop(reverse);
		children.push((
			weight,
			environment.prepare_matching(shard, QubitCount::new(7)?, vec![0, 6, 5, 1])?,
		));
	}
	Ok(children)
}
const fn counters(s: RoutingStatistics) -> [usize; 9] {
	[
		s.batches,
		s.local_pair_candidates,
		s.maximum_batch_pairs,
		s.maximum_routed_amplitudes,
		s.coordination_calls,
		s.indexed_reads,
		s.indexed_writes,
		s.point_to_point_sent_bytes,
		s.point_to_point_received_bytes,
	]
}
fn accumulate(total: &mut [usize; 9], s: RoutingStatistics) -> TestResult {
	for (i, next) in counters(s).into_iter().enumerate() {
		let current = total.get_mut(i).ok_or("counter field")?;
		*current = if i == 2 || i == 3 {
			(*current).max(next)
		} else {
			current.checked_add(next).ok_or("counter overflow")?
		};
	}
	Ok(())
}
#[test]
#[allow(
	clippy::too_many_lines,
	clippy::arithmetic_side_effects,
	reason = "One fixed 128-amplitude MPI lifecycle compares native child/source totals and receipt reset without shared expected aggregation"
)]
fn successful_apply_receipts_cover_every_child_and_source_query() -> TestResult {
	if std::env::var("QUEST_ROUTING_TELEMETRY_CHILD").is_err() {
		for (parts, split) in [(1, false), (2, false), (4, false), (8, false), (4, true)] {
			let output = quest_test_support::mpi::MpiTest::new(
				usize::try_from(parts)?,
				std::time::Duration::from_secs(60),
			)?
			.args([
				"--exact",
				"successful_apply_receipts_cover_every_child_and_source_query",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_ROUTING_TELEMETRY_CHILD", "1")
			.env(
				"QUEST_ROUTING_TELEMETRY_SPLIT",
				if split { "1" } else { "0" },
			)
			.output()?;
			assert!(
				output.status.success(),
				"{parts}/{split}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_ROUTING_TELEMETRY_SPLIT")? == "1" {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let parts = usize::try_from(comm.size()?)?;
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(8_388_608))
		.build()?;
	let limits = MatchingLcuLimits {
		plan: quest_qsvt::portfolio::LcuPlanLimits {
			max_bytes: 65_536,
			..Default::default()
		},
		max_local_bytes: 1_048_576,
		ranks_per_node: parts,
		node_budget: MemoryBudget::new(67_108_864),
		..Default::default()
	};
	let mut manual = children(&environment)?;
	let mut lcu = environment.prepare_matching_lcu(children(&environment)?, vec![2, 3], limits)?;
	let mut register = environment.state_vector_local(QubitCount::new(7)?)?;
	register.init_zero()?;
	assert!(lcu.last_apply_telemetry().is_none());
	assert_eq!(lcu.plan().surviving_indices(), &[0, 2, 3]);
	for adjoint in [false, true] {
		for value in [0, 16] {
			let mut expected = [0; 9];
			let mut events = 0;
			// Matching routing visits indices from layout/control bits independently of
			// amplitude values; PREP/phase gates are deliberately outside these counters.
			lcu.plan().visit_mapped_steps(
				lcu.targets(),
				16,
				value,
				adjoint,
				|step| -> quest::qsvt::Result<()> {
					if let LcuStep::Child {
						index,
						adjoint,
						control_mask,
						control_value,
					} = step
					{
						let original = *lcu
							.plan()
							.surviving_indices()
							.get(index)
							.ok_or(quest::Error::Overflow)?;
						let child = &mut manual.get_mut(original).ok_or(quest::Error::Overflow)?.1;
						child.apply(&mut register, adjoint, control_mask, control_value)?;
						accumulate(&mut expected, child.last_statistics())
							.map_err(|_| quest::Error::Overflow)?;
						events += 1;
					}
					Ok(())
				},
			)?;
			let retained = environment.view().allocated_bytes();
			lcu.apply(&mut register, adjoint, 16, value)?;
			let receipt = lcu.last_apply_telemetry().ok_or("complete LCU receipt")?;
			assert!(receipt.exact);
			assert_eq!(receipt.child_events, events);
			assert_eq!(receipt.source_queries, 0);
			assert_eq!(counters(receipt.routing), expected);
			assert_eq!(environment.view().allocated_bytes(), retained);
			if parts > 1 {
				assert!(receipt.routing.point_to_point_sent_bytes > 0);
			}
			assert!(lcu.admit_apply(&register, adjoint, 1, 0).is_err());
			assert_eq!(
				counters(
					lcu.last_apply_telemetry()
						.ok_or("standalone admission retains receipt")?
						.routing
				),
				expected
			);
			let state =
				register.read_local_amplitudes(0, register.deployment().local_amplitudes())?;
			let pressure = environment.reserve_external_bytes(1_048_576)?;
			assert!(lcu.admit_apply(&register, adjoint, 16, value).is_err());
			assert!(lcu.last_apply_telemetry().is_some());
			assert!(lcu.apply(&mut register, adjoint, 16, value).is_err());
			assert!(lcu.last_apply_telemetry().is_none());
			assert_eq!(register.read_local_amplitudes(0, state.len())?, state);
			drop(pressure);
		}
	}
	let schedule = TransformSchedule::from_phase_sequence(
		lcu.plan().descriptor().clone(),
		PhaseSequence::<WxSymmetric>::builder(vec![0.12, -0.3, -0.3, 0.12]).build()?,
		NumericalPolicy::default(),
	)?;
	let mut reference = environment.prepare_matching_lcu(manual, vec![2, 3], limits)?;
	let mut transform = environment.prepare_matching_lcu_transform(
		lcu,
		4,
		schedule,
		TransformExecutionLimits {
			max_local_bytes: 1_048_576,
			ranks_per_node: parts,
			node_budget: MemoryBudget::new(67_108_864),
			..Default::default()
		},
	)?;
	assert!(transform.last_apply_telemetry().is_none());
	for adjoint in [false, true] {
		let mut expected = [0; 9];
		let mut queries = 0;
		let mut events = 0;
		transform
			.schedule()
			.visit_steps(adjoint, |step| -> quest::qsvt::Result<()> {
				if let TransformStep::Oracle { adjoint, response } = step {
					reference.apply(&mut register, adjoint, 16, usize::from(response) * 16)?;
					let receipt = reference
						.last_apply_telemetry()
						.ok_or(quest::Error::Overflow)?;
					accumulate(&mut expected, receipt.routing)
						.map_err(|_| quest::Error::Overflow)?;
					queries += 1;
					events += receipt.child_events;
				}
				Ok(())
			})?;
		let retained = environment.view().allocated_bytes();
		let admitted = transform.apply(&mut register, adjoint, 0, 0)?;
		let receipt = transform
			.last_apply_telemetry()
			.ok_or("complete transform receipt")?;
		assert!(receipt.exact);
		assert_eq!(receipt.source_queries, queries);
		assert_eq!(receipt.source_queries, admitted.source_queries);
		assert_eq!(receipt.child_events, events);
		assert_eq!(counters(receipt.routing), expected);
		assert_eq!(environment.view().allocated_bytes(), retained);
		assert!(transform.admit_apply(&register, adjoint, 16, 0).is_err());
		assert!(transform.last_apply_telemetry().is_some());
		let state = register.read_local_amplitudes(0, register.deployment().local_amplitudes())?;
		let pressure = environment.reserve_external_bytes(1_048_576)?;
		assert!(transform.admit_apply(&register, adjoint, 0, 0).is_err());
		assert!(transform.last_apply_telemetry().is_some());
		assert!(transform.apply(&mut register, adjoint, 0, 0).is_err());
		assert!(transform.last_apply_telemetry().is_none());
		assert_eq!(register.read_local_amplitudes(0, state.len())?, state);
		drop(pressure);
	}
	// Degree zero still executes response/projector primitives, but dispatches no source.
	let source = environment.prepare_matching_lcu(children(&environment)?, vec![2, 3], limits)?;
	let schedule = TransformSchedule::from_parts(
		source.plan().descriptor().clone(),
		vec![0.2],
		0.0,
		NumericalPolicy::default(),
	)?;
	let mut zero = environment.prepare_matching_lcu_transform(
		source,
		4,
		schedule,
		TransformExecutionLimits {
			ranks_per_node: parts,
			node_budget: MemoryBudget::new(67_108_864),
			..Default::default()
		},
	)?;
	zero.apply(&mut register, false, 0, 0)?;
	let empty = zero
		.last_apply_telemetry()
		.ok_or("complete no-query receipt")?;
	assert!(empty.exact);
	assert_eq!(empty.source_queries, 0);
	assert_eq!(empty.child_events, 0);
	assert_eq!(counters(empty.routing), [0; 9]);
	Ok(())
}
