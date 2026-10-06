#![cfg(feature = "qsvt")]
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Small whole-unitary differentials use explicitly bounded arrays"
)]
use googletest::prelude::*;
use quest::{Complex64, Environment, QubitCount};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsp::{PhaseSequence, WxSymmetric};
use quest_qsvt::{
	MatchingEncoding, MatchingShard, NumericalPolicy, OperandLayout, TransformBuilder,
	materialize_program,
	replay_transform::{MatchingSchedule, MatchingTransform},
};

fn source() -> quest_qsvt::Result<MatchingEncoding> {
	let matrix = SparseMatrix::from_triplets(
		2,
		3,
		SparseFormat::Csr,
		vec![
			(0, 1, Complex64::new(0.3, 0.2)),
			(1, 0, Complex64::new(-0.7, 0.1)),
			(1, 2, Complex64::new(0.1, 0.0)),
		],
		SparseLimits::default(),
	)?;
	MatchingEncoding::from_sparse(&matrix, NumericalPolicy::default())
}
fn initial(count: usize) -> Vec<Complex64> {
	let mut values: Vec<_> = (0..count)
		.map(|i| {
			Complex64::new(
				f64::from(u32::try_from(i % 17).unwrap_or(0)) - 8.0,
				f64::from(u32::try_from(i % 7).unwrap_or(0)) - 3.0,
			)
		})
		.collect();
	let norm = values.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	for value in &mut values {
		*value /= norm;
	}
	values
}
fn canonical(basis: usize, targets: &[usize], response: usize) -> usize {
	let mut packed = ((basis >> response) & 1) << targets.len();
	for (position, &target) in targets.iter().enumerate() {
		packed |= ((basis >> target) & 1) << position;
	}
	packed
}

#[gtest]
fn matching_transform_native_matches_portable_with_padding_spectators_and_adjoint()
-> googletest::Result<()> {
	let policy = NumericalPolicy::default();
	let encoding = source()?;
	let width = encoding.num_qubits();
	let count = QubitCount::new(width + 2)?;
	let targets: Vec<_> = (1..=width).rev().collect();
	let response = 0;
	let environment = Environment::builder().build()?;
	let shard = MatchingShard::from_encoding(&encoding, 0, 1, policy)?;
	let schedule = MatchingSchedule::from_parts(shard.header(), vec![0.1], 0.0, policy)?;
	expect_true!(
		environment
			.prepare_matching_transform(shard, count, targets.clone(), targets[0], schedule)
			.is_err()
	);
	let shard = MatchingShard::from_encoding(&encoding, 0, 1, policy)?;
	let mut wrong_source = shard.header();
	wrong_source.source_identity ^= 1;
	let schedule = MatchingSchedule::from_parts(wrong_source, vec![0.1], 0.0, policy)?;
	expect_true!(
		environment
			.prepare_matching_transform(shard, count, targets, response, schedule)
			.is_err()
	);
	for (case, phases) in [
		vec![0.17],
		vec![0.23, 0.23],
		vec![0.21, -0.3, 0.21],
		vec![0.17, -0.23, -0.23, 0.17],
		vec![0.17, -0.23, -0.23, 0.17],
	]
	.into_iter()
	.enumerate()
	{
		let response = if case == 4 { width } else { 0 };
		let targets: Vec<_> = if case == 4 {
			(0..width).collect()
		} else if case.is_multiple_of(2) {
			(1..=width).collect()
		} else {
			(1..=width).rev().collect()
		};
		let sequence = PhaseSequence::<WxSymmetric>::builder(phases).build()?;
		let transform = MatchingTransform::new(encoding.clone(), sequence.clone(), policy)?;
		let portable = TransformBuilder::new()
			.encoding(encoding.projected_encoding(policy)?)
			.operands(OperandLayout::new(
				width + 1,
				(0..width).collect(),
				width,
				None,
			)?)
			.standard(sequence)
			.build()?;
		let unitary = materialize_program(portable.main(), policy)?;
		let shard = MatchingShard::from_encoding(&encoding, 0, 1, policy)?;
		let mut prepared = environment.prepare_matching_transform(
			shard,
			count,
			targets.clone(),
			response,
			transform.schedule(policy)?,
		)?;
		let mut register = environment.state_vector(count)?;
		let state = initial(count.dimension());
		register.init_pure(&state)?;
		let bytes = environment.allocated_bytes();
		prepared.apply(&mut register, false)?;
		let actual = register.amplitudes(0, count.dimension())?;
		for row in 0..actual.len() {
			let expected: Complex64 = (0..actual.len())
				.filter(|&col| col >> (width + 1) == row >> (width + 1))
				.map(|col| {
					unitary[(
						canonical(row, &targets, response),
						canonical(col, &targets, response),
					)] * state[col]
				})
				.sum();
			expect_true!((actual[row] - expected).norm() < 1e-11);
		}
		prepared.apply(&mut register, true)?;
		let actual = register.amplitudes(0, count.dimension())?;
		expect_true!(
			actual
				.iter()
				.zip(state)
				.all(|(a, b)| (*a - b).norm() < 1e-11)
		);
		expect_eq!(environment.allocated_bytes(), bytes);
	}
	Ok(())
}

#[cfg(all(feature = "mpi", quest_native_mpi))]
#[gtest]
fn matching_transform_collective_matches_portable_on_local_partitions() -> googletest::Result<()> {
	use quest::collective::{CollectiveEnvironment, MpiRuntime};
	if std::env::var("QUEST_TRANSFORM_RANKS").is_err() {
		let status = quest_test_support::mpi::MpiTest::new(2, std::time::Duration::from_secs(90))?
			.args([
				"--exact",
				"matching_transform_collective_matches_portable_on_local_partitions",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_TRANSFORM_RANKS", "2")
			.status()?;
		expect_true!(status.success());
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let environment = CollectiveEnvironment::builder(&comm)?.build()?;
	let policy = NumericalPolicy::default();
	let encoding = source()?;
	let width = encoding.num_qubits();
	let count = QubitCount::new(width + 2)?;
	let mut targets: Vec<_> = (1..=width).rev().collect();
	// Place a system bit in the native rank prefix so the permutation genuinely
	// crosses state partitions, while retaining a separate spectator bit.
	targets[1] = width + 1;
	let response = 0;
	let spectator_mask = (0..count.get())
		.filter(|&position| position != response && !targets.contains(&position))
		.fold(0, |mask, position| mask | (1 << position));
	let sequence = PhaseSequence::<WxSymmetric>::builder(vec![0.17, -0.23, -0.23, 0.17]).build()?;
	let bad_shard = MatchingShard::from_encoding(
		&encoding,
		usize::try_from(comm.rank()?)?,
		usize::try_from(comm.size()?)?,
		policy,
	)?;
	let mismatched = MatchingSchedule::from_parts(
		bad_shard.header(),
		vec![if comm.rank()? == 0 { 0.1 } else { 0.2 }],
		0.0,
		policy,
	)?;
	expect_true!(
		environment
			.prepare_matching_transform(bad_shard, count, targets.clone(), response, mismatched)
			.is_err()
	);
	let transform = MatchingTransform::new(encoding.clone(), sequence.clone(), policy)?;
	let portable = TransformBuilder::new()
		.encoding(encoding.projected_encoding(policy)?)
		.operands(OperandLayout::new(
			width + 1,
			(0..width).collect(),
			width,
			None,
		)?)
		.standard(sequence)
		.build()?;
	let unitary = materialize_program(portable.main(), policy)?;
	let shard = MatchingShard::from_encoding(
		&encoding,
		usize::try_from(comm.rank()?)?,
		usize::try_from(comm.size()?)?,
		policy,
	)?;
	let schedule = transform.schedule(policy)?;
	drop(transform);
	drop(encoding);
	let mut prepared = environment.prepare_matching_transform(
		shard,
		count,
		targets.clone(),
		response,
		schedule,
	)?;
	let mut register = environment.state_vector_local(count)?;
	let local = register.deployment().local_amplitudes();
	let start = register.deployment().rank() * local;
	let state = initial(count.dimension()); // Explicitly bounded independent reference only.
	register.write_local_amplitudes(0, &state[start..start + local])?;
	let bytes = environment.view().allocated_bytes();
	prepared.apply(&mut register, false)?;
	let actual = register.read_local_amplitudes(0, local)?;
	for (offset, &value) in actual.iter().enumerate() {
		let expected: Complex64 = (0..count.dimension())
			.filter(|&col| col & spectator_mask == (start + offset) & spectator_mask)
			.map(|col| {
				unitary[(
					canonical(start + offset, &targets, response),
					canonical(col, &targets, response),
				)] * state[col]
			})
			.sum();
		expect_true!((value - expected).norm() < 1e-11);
	}
	prepared.apply(&mut register, true)?;
	let actual = register.read_local_amplitudes(0, local)?;
	expect_true!(
		actual
			.iter()
			.zip(&state[start..start + local])
			.all(|(a, b)| (*a - *b).norm() < 1e-11)
	);
	expect_eq!(environment.view().allocated_bytes(), bytes);
	Ok(())
}
