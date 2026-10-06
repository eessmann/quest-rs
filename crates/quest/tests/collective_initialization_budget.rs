#![cfg(all(feature = "mpi", quest_native_mpi))]
#![allow(
	clippy::panic_in_result_fn,
	reason = "Subprocess assertions verify collective memory admission before mutation"
)]

use quest::{
	Complex64, MemoryBudget, QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
};

#[test]
fn root_initialization_accounts_for_three_live_global_buffers() -> googletest::Result<()> {
	if std::env::var_os("QUEST_ROOT_INITIALIZATION_BUDGET_CHILD").is_none() {
		let output = quest_test_support::mpi::MpiTest::new(4, std::time::Duration::from_secs(60))?
			.args([
				"--exact",
				"root_initialization_accounts_for_three_live_global_buffers",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_ROOT_INITIALIZATION_BUDGET_CHILD", "1")
			.output()?;
		assert!(
			output.status.success(),
			"{}\n{}\n{}",
			output.status,
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	assert_eq!(comm.size()?, 4);
	let rank = usize::try_from(comm.rank()?)?;
	let count = QubitCount::new(20)?;
	let entries = count.dimension();
	let global_bytes = entries.checked_mul(16).ok_or(quest::Error::Overflow)?;
	let local = entries.checked_div(4).ok_or(quest::Error::Overflow)?;
	let register_bytes = local.checked_mul(64).ok_or(quest::Error::Overflow)?;
	let scratch_bytes = global_bytes.checked_mul(3).ok_or(quest::Error::Overflow)?;
	let budget = register_bytes
		.checked_add(scratch_bytes)
		.ok_or(quest::Error::Overflow)?;
	let environment = CollectiveEnvironment::builder(&comm)?
		.memory_budget(MemoryBudget::new(budget))
		.build()?;
	let mut register = environment.state_vector_local(count)?;
	assert_eq!(register.deployment().local_amplitudes(), local);
	assert_eq!(environment.view().allocated_bytes(), register_bytes);
	register.init_plus()?;
	let initial = register.read_local_amplitudes(0, local)?;
	let mut source = if rank == 0 {
		vec![Complex64::new(0.0, 0.0); entries]
	} else {
		Vec::new()
	};
	if let Some(last) = source.last_mut() {
		*last = Complex64::new(0.0, 1.0);
	}
	let input = (rank == 0).then_some(source.as_slice());
	// Only one non-root rank is short of the third global conversion buffer.
	// The old two-buffer charge fits and therefore incorrectly mutates every rank.
	let external = environment.reserve_external_bytes(if rank == 1 { global_bytes } else { 0 })?;
	let retained = environment.view().allocated_bytes();
	assert!(
		register.init_pure_from_root(0, input).is_err(),
		"a third live global buffer must cause collective budget rejection"
	);
	assert_eq!(environment.view().allocated_bytes(), retained);
	assert_eq!(register.read_local_amplitudes(0, local)?, initial);
	drop(external);
	assert_eq!(environment.view().allocated_bytes(), register_bytes);
	register.init_pure_from_root(0, input)?;
	assert_eq!(environment.view().allocated_bytes(), register_bytes);
	let output = register.read_local_amplitudes(0, local)?;
	for (index, actual) in output.iter().enumerate() {
		let expected = if rank == 3 && index.checked_add(1) == Some(local) {
			Complex64::new(0.0, 1.0)
		} else {
			Complex64::new(0.0, 0.0)
		};
		assert_eq!(*actual, expected);
	}
	assert_eq!(register.total_probability()?, 1.0);
	assert_eq!(environment.view().allocated_bytes(), register_bytes);
	Ok(())
}
