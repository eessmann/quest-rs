use super::*;

#[gtest]
fn distributed_matching_reuses_native_buffer_without_width_scaled_scratch() -> googletest::Result<()>
{
	if std::env::var("QUEST_MATCHING_REUSE_RANKS").is_err() {
		for count in [1, 2, 4, 8] {
			let status =
				quest_test_support::mpi::MpiTest::new(count, std::time::Duration::from_secs(90))?
					.args([
						"--exact",
						"buffer_reuse::distributed_matching_reuses_native_buffer_without_width_scaled_scratch",
						"--nocapture",
						"--test-threads=1",
					])
					.env("QUEST_MATCHING_REUSE_RANKS", count.to_string())
					.status()?;
			expect_true!(status.success());
		}
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(8 * 1024 * 1024))
		.build()?;
	let matrix = SparseMatrix::from_triplets(
		2,
		2,
		SparseFormat::Csr,
		vec![
			(0, 0, Complex64::new(0.7, 0.1)),
			(1, 0, Complex64::new(-0.2, 0.05)),
			(0, 1, Complex64::new(0.4, -0.2)),
			(1, 1, Complex64::new(-0.1, -0.3)),
		],
		SparseLimits::default(),
	)?;
	let policy = NumericalPolicy::default();
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	let shard = MatchingShard::from_encoding(
		&encoding,
		usize::try_from(comm.rank()?)?,
		usize::try_from(comm.size()?)?,
		policy,
	)?;
	let baseline = environment.view().allocated_bytes();
	let small = environment.prepare_matching(shard.clone(), QubitCount::new(9)?, vec![0, 1, 5])?;
	if comm.size()? > 1 {
		expect_true!(small.scratch_deployment().is_none());
	} else {
		let deployment = small
			.scratch_deployment()
			.ok_or(quest::Error::Value("missing local scratch fallback"))?;
		expect_false!(deployment.is_distributed());
		expect_eq!(deployment.local_amplitudes(), 512);
		expect_eq!(deployment.host_array_bytes(), 8192);
	}
	let small_bytes = environment.view().allocated_bytes();
	drop(small);
	expect_eq!(environment.view().allocated_bytes(), baseline);
	let large = environment.prepare_matching(shard.clone(), QubitCount::new(13)?, vec![0, 1, 5])?;
	let large_bytes = environment.view().allocated_bytes();
	if comm.size()? > 1 {
		// Only the caller's eventual state width changes: preparation owns the same
		// shard and fixed-size routing payload, with no second native Qureg.
		expect_eq!(large_bytes, small_bytes);
	} else {
		// Native QuEST disables MPI deployment at P=1, so no communication buffer
		// is available and the local scratch-register fallback remains necessary.
		expect_gt!(large_bytes, small_bytes);
	}
	drop(large);
	expect_eq!(environment.view().allocated_bytes(), baseline);
	for control in [ControlState::Zero, ControlState::One] {
		exercise_distributed_color(&environment, &comm, &encoding, &shard, control)?;
	}
	expect_eq!(environment.view().allocated_bytes(), baseline);
	Ok(())
}

fn exercise_distributed_color(
	environment: &CollectiveEnvironment<'_, '_>,
	comm: &quest_sys::mpi::MpiCommunicator<'_>,
	encoding: &MatchingEncoding,
	shard: &MatchingShard,
	control: ControlState,
) -> googletest::Result<()> {
	let targets = vec![0, 1, 5];
	let policy = NumericalPolicy::default();
	let mut builder = QuantumRegionBuilder::new(6, 0)?;
	let mapped = targets
		.iter()
		.map(|&position| builder.qubit(position))
		.collect::<quest_compile::Result<Vec<_>>>()?;
	builder.oracle(
		&encoding.to_oracle(policy)?,
		&mapped,
		&[Control::new(builder.qubit(3)?, control)],
	)?;
	let unitary = materialize_program(&builder.finish()?.bind(&[])?, policy)?;
	let mut prepared = environment.prepare_matching(shard.clone(), QubitCount::new(6)?, targets)?;
	let mut register = environment.state_vector_local(QubitCount::new(6)?)?;
	let local = register.deployment().local_amplitudes();
	let start = usize::try_from(comm.rank()?)?
		.checked_mul(local)
		.ok_or(quest::Error::Overflow)?;
	let end = start.checked_add(local).ok_or(quest::Error::Overflow)?;
	if comm.size()? > 1 {
		expect_le!(local, 32); // color target 5 belongs to the distributed prefix.
	}
	let state: Vec<_> = (0..64)
		.map(|i| Complex64::new(f64::from(i % 13) - 6.0, f64::from(i % 7) - 3.0))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|value| value.div(norm)).collect();
	let owned = state.get(start..end).ok_or(quest::Error::Overflow)?;
	let expected: Vec<_> = (start..end)
		.map(|row| {
			state
				.iter()
				.enumerate()
				.map(|(col, value)| unitary[(row, col)].mul(*value))
				.sum::<Complex64>()
		})
		.collect();
	register.write_local_amplitudes(0, owned)?;
	let retained = environment.view().allocated_bytes();
	let value = if control == ControlState::One {
		1 << 3
	} else {
		0
	};
	// Native Hadamards on the distributed color use MPI scratch themselves.
	// Alternating scalar/batched calls also catches stale borrowed-buffer state.
	for scalar_first in [false, true] {
		if scalar_first {
			prepared.apply_scalar(&mut register, false, 1 << 3, value)?;
		} else {
			prepared.apply(&mut register, false, 1 << 3, value)?;
		}
		for (actual, expected) in register
			.read_local_amplitudes(0, local)?
			.iter()
			.zip(&expected)
		{
			expect_true!(actual.sub(*expected).norm() < 1e-12);
		}
		expect_true!((register.total_probability()? - 1.0).abs() < 1e-12);
		if scalar_first {
			prepared.apply(&mut register, true, 1 << 3, value)?;
		} else {
			prepared.apply_scalar(&mut register, true, 1 << 3, value)?;
		}
		for (actual, expected) in register.read_local_amplitudes(0, local)?.iter().zip(owned) {
			expect_true!(actual.sub(*expected).norm() < 1e-12);
		}
		expect_eq!(environment.view().allocated_bytes(), retained);
	}
	Ok(())
}
