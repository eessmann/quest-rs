//! Rank-local admission injection lives only in the library test executable.
#![allow(
	clippy::panic_in_result_fn,
	reason = "Subprocess assertions verify collective rejection before mutation"
)]
use crate::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
};
use quest_numerics::{SparseFormat, SparseLimits, SparseMatrix};
use quest_qsvt::{MatchingEncoding, MatchingShard, NumericalPolicy};
use std::{
	cell::Cell,
	ops::{Div, Sub},
};

thread_local! {
	static FAIL_ADMISSION: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn inject_admission_failure(rank: usize) -> crate::Result<()> {
	if rank == 1 && FAIL_ADMISSION.get() {
		return Err(crate::Error::Value(
			"injected matching owned scratch admission",
		));
	}
	Ok(())
}

struct AdmissionFailure;
impl AdmissionFailure {
	fn enable() -> Self {
		assert!(!FAIL_ADMISSION.replace(true));
		Self
	}
}
impl Drop for AdmissionFailure {
	fn drop(&mut self) {
		FAIL_ADMISSION.set(false);
	}
}

fn launch() -> googletest::Result<()> {
	for ranks in [2, 4] {
		let output = quest_test_support::mpi::MpiTest::new(ranks, std::time::Duration::from_secs(60))?
			.args([
				"--exact",
				"qsvt::matching::collective::buffer_tests::rank_local_buffer_admission_rejects_before_opening_hadamards",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_MATCHING_BUFFER_ADMISSION_CHILD", "1")
			.output()?;
		assert!(
			output.status.success(),
			"MPI {ranks}: {}\n{}\n{}",
			output.status,
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
	}
	Ok(())
}

#[test]
fn rank_local_buffer_admission_rejects_before_opening_hadamards() -> googletest::Result<()> {
	if std::env::var_os("QUEST_MATCHING_BUFFER_ADMISSION_CHILD").is_none() {
		return launch();
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let rank = usize::try_from(comm.rank()?)?;
	let parts = usize::try_from(comm.size()?)?;
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(1024 * 1024))
		.build()?;
	let policy = NumericalPolicy::default();
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
	let encoding = MatchingEncoding::from_sparse(&matrix, policy)?;
	let shard = MatchingShard::from_encoding(&encoding, rank, parts, policy)?;
	let mut prepared = environment.prepare_matching(shard, QubitCount::new(6)?, vec![0, 1, 5])?;
	assert!(prepared.scratch_deployment().is_distributed());
	let mut register = environment.state_vector_local(QubitCount::new(6)?)?;
	let deployment = register.deployment();
	let local = deployment.local_amplitudes();
	assert!(
		local <= 32,
		"color target 5 must be in the distributed prefix"
	);
	let state: Vec<_> = (0..64)
		.map(|i| Complex64::new(f64::from(i % 13) - 6.0, f64::from(i % 7) - 3.0))
		.collect();
	let norm = state.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
	let state: Vec<_> = state.into_iter().map(|value| value.div(norm)).collect();
	let start = rank.checked_mul(local).ok_or(crate::Error::Overflow)?;
	let end = start.checked_add(local).ok_or(crate::Error::Overflow)?;
	let initial = state.get(start..end).ok_or(crate::Error::Overflow)?;
	register.write_local_amplitudes(0, initial)?;
	let baseline = environment.view().allocated_bytes();
	let retained = prepared.retained_bytes()?;
	for scalar in [false, true] {
		{
			let _injection = AdmissionFailure::enable();
			let rejected = if scalar {
				prepared.apply_scalar(&mut register, false, 1 << 3, 0)
			} else {
				prepared.apply(&mut register, false, 1 << 3, 0)
			};
			assert!(
				rejected.is_err(),
				"rank 1 buffer admission must reject on every rank"
			);
		}
		assert_eq!(register.read_local_amplitudes(0, local)?, initial);
		assert_eq!(register.deployment(), deployment);
		assert_eq!(environment.view().allocated_bytes(), baseline);
		assert_eq!(prepared.retained_bytes()?, retained);
	}
	prepared.apply(&mut register, false, 1 << 3, 0)?;
	prepared.apply_scalar(&mut register, true, 1 << 3, 0)?;
	for (actual, expected) in register
		.read_local_amplitudes(0, local)?
		.iter()
		.zip(initial)
	{
		assert!(actual.sub(*expected).norm() < 1e-12);
	}
	assert_eq!(environment.view().allocated_bytes(), baseline);
	assert_eq!(prepared.retained_bytes()?, retained);
	Ok(())
}
