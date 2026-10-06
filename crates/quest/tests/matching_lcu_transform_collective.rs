#![cfg(all(feature = "qsvt", feature = "mpi", quest_native_mpi))]
use googletest::prelude::*;
use quest::collective::{CollectiveEnvironment, MpiRuntime};
use quest::qsvt::matching_lcu_transform::TransformExecutionLimits;
use quest::qsvt::{
	matching::preprocess::{ProducerLimits, produce_matching},
	matching_lcu::MatchingLcuLimits,
};
use quest::{Complex64, MemoryBudget, QubitCount};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix, sparse_stream::SparseEntry};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::replay_transform::{ReplayTransform, TransformSchedule};
use quest_qsvt::{
	MatchingEncoding, NumericalPolicy,
	portfolio::{PortfolioLimits, WeightedLcu},
};
use std::ops::{Div, Mul, Sub};
#[gtest]
#[allow(
	clippy::arithmetic_side_effects,
	reason = "the independent differential uses fixed dimensions at most 512 and eight ranks"
)]
fn sharded_lcu_transform_whole_register_and_standalone_adjoint() -> googletest::Result<()> {
	if std::env::var("QUEST_LCU_CHILD").is_err() {
		for (ranks, split, edge) in [
			(1, 0, 0),
			(2, 0, 0),
			(4, 0, 0),
			(8, 0, 0),
			(4, 1, 0),
			(1, 0, 1),
			(2, 0, 1),
		] {
			let status = quest_test_support::mpi::MpiTest::new(
				usize::try_from(ranks)?,
				std::time::Duration::from_secs(120),
			)?
			.args([
				"--exact",
				"sharded_lcu_transform_whole_register_and_standalone_adjoint",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_LCU_CHILD", "1")
			.env("QUEST_LCU_SPLIT", split.to_string())
			.env("QUEST_LCU_EDGE_INPUTS", edge.to_string())
			.status()?;
			expect_true!(status.success());
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let mut world = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(world.size()?)?;
	let comm = if std::env::var("QUEST_LCU_SPLIT").as_deref() == Ok("1") {
		world.split_power_of_two(2)?
	} else {
		world.duplicate()?
	};
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let env = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(64 * 1024 * 1024))
		.build()?;
	let p = NumericalPolicy::default();
	let count = QubitCount::new(9)?;
	let targets = vec![2, 0, 4, 1, 5];
	let edge = std::env::var("QUEST_LCU_EDGE_INPUTS").as_deref() == Ok("1");
	let weights = if edge {
		vec![
			Complex64::new(0., 2.),
			Complex64::new(0., 0.),
			Complex64::new(-0.25, 0.),
			Complex64::new(0.1, -0.2),
			Complex64::new(0., 1e-320),
		]
	} else {
		vec![
			Complex64::new(0., 2.),
			Complex64::new(-0.25, 0.),
			Complex64::new(0.1, -0.2),
		]
	};
	let owners = weights.len();
	let mut producer_seconds = 0.;
	let mut child_seconds = 0.;
	let mut producer_work = 0usize;
	let mut producer_wire = 0usize;

	let mut terms = Vec::new();
	let mut references = Vec::new();
	for (term, weight) in weights.into_iter().enumerate() {
		let index = f64::from(u32::try_from(term)?);
		let scale = if edge && term == 4 { 1e-100 } else { 1. };
		// Construct only the origin-rank entries; no full source/model in production.
		let entries = (0..4_u64)
			.filter(|ordinal| {
				usize::try_from(*ordinal).is_ok_and(|i| i.checked_rem(parts) == Some(rank))
			})
			.map(|ordinal| {
				let (row, column, value) = match ordinal {
					0 => (0, 0, Complex64::new(1. + index, 1.).mul(scale)),
					1 => (0, 1, Complex64::new(-1., index).mul(scale)),
					2 => (0, 2, Complex64::new(0.3, 2. + index).mul(scale)),
					_ => (1, 0, Complex64::new(0.2, 0.).mul(scale)),
				};
				Ok(SparseEntry {
					row,
					column,
					ordinal,
					value,
				})
			});
		let start = std::time::Instant::now();
		let produced = produce_matching(&comm, 2, 3, entries, ProducerLimits::default())?;
		producer_seconds += start.elapsed().as_secs_f64();
		let (shard, edges, reverse, stats) = produced.into_parts();
		producer_work = producer_work
			.checked_add(stats.work)
			.ok_or(quest::Error::Overflow)?;
		producer_wire = producer_wire
			.checked_add(stats.sent_bytes)
			.ok_or(quest::Error::Overflow)?;
		drop(edges);
		drop(reverse);
		let start = std::time::Instant::now();
		terms.push((weight, env.prepare_matching(shard, count, targets.clone())?));
		child_seconds += start.elapsed().as_secs_f64();
		// Separate tiny complete cold references are used solely by the differential.
		let sparse = SparseMatrix::from_triplets(
			2,
			3,
			SparseFormat::Csr,
			vec![
				(0, 0, Complex64::new(1. + index, 1.).mul(scale)),
				(0, 1, Complex64::new(-1., index).mul(scale)),
				(0, 2, Complex64::new(0.3, 2. + index).mul(scale)),
				(1, 0, Complex64::new(0.2, 0.).mul(scale)),
			],
			SparseLimits::default(),
		)?;
		references.push((weight, MatchingEncoding::from_sparse(&sparse, p)?));
	}
	let sequence = PhaseSequence::<WxSymmetric>::builder(vec![0.12, -0.3, -0.3, 0.12]).build()?;
	let portable = ReplayTransform::new(
		WeightedLcu::new(references, PortfolioLimits::default())?,
		sequence.clone(),
		p,
	)?;
	let mut limits = MatchingLcuLimits::default();
	limits.plan.max_bytes = 1024 * 1024;
	limits.ranks_per_node = parts;
	limits.node_budget = MemoryBudget::new(64 * 1024 * 1024);
	let started = std::time::Instant::now();
	let source = env.prepare_matching_lcu(terms, vec![8, 3], limits)?;
	let schedule =
		TransformSchedule::from_phase_sequence(source.plan().descriptor().clone(), sequence, p)?;
	let mut prepared = env.prepare_matching_lcu_transform(
		source,
		7,
		schedule,
		TransformExecutionLimits {
			ranks_per_node: parts,
			..Default::default()
		},
	)?;
	if edge {
		expect_eq!(prepared.source_plan().surviving_indices(), &[0, 2, 3, 4]);
		expect_eq!(prepared.source_plan().resources().selected_terms, 4);
	}
	let composition_seconds = started.elapsed().as_secs_f64();
	println!(
		"LCU_PIPELINE rank={rank} parts={parts} owners={owners} edge_inputs={edge} producer_seconds={producer_seconds} producer_work={producer_work} producer_application_sent_bytes={producer_wire} child_prepare_seconds={child_seconds} composition_seconds={composition_seconds} composition_compile_work={}",
		prepared.constructor_work()
	);
	println!(
		"LCU_COMPOSITION_COMPARISON rank={rank} parts={parts} {:?}",
		prepared.constructor_communication()
	);
	let mut register = env.state_vector_local(count)?;
	let state: Vec<_> = (0..512)
		.map(|i| Complex64::new(f64::from(i % 13) - 6., f64::from(i % 7) - 3.))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|a| a.div(norm)).collect();
	let local = register.deployment().local_amplitudes();
	let start = rank.checked_mul(local).ok_or(quest::Error::Overflow)?;
	for positive in [false, true] {
		for adjoint in [false, true] {
			register.write_local_amplitudes(0, &state[start..start + local])?;
			let bytes = env.view().allocated_bytes();
			expect_true!(prepared.admit_apply(&register, adjoint, 1 << 2, 0).is_err());
			let _admitted =
				prepared.admit_apply(&register, adjoint, 1 << 6, usize::from(positive) << 6)?;
			expect_eq!(
				register.read_local_amplitudes(0, local)?,
				state[start..start + local]
			);
			prepared.apply(&mut register, adjoint, 1 << 6, usize::from(positive) << 6)?;
			let mut expected = state.clone();
			portable.apply_mapped_reference(
				&mut expected,
				&[2, 0, 4, 1, 5, 8, 3, 7],
				1 << 6,
				usize::from(positive) << 6,
				adjoint,
				p,
			)?;
			for (value, expected) in register
				.read_local_amplitudes(0, local)?
				.iter()
				.zip(&expected[start..start + local])
			{
				expect_true!(value.sub(*expected).norm() < 2e-12);
			}

			expect_eq!(env.view().allocated_bytes(), bytes);
		}
	}
	Ok(())
}
