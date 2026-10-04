//! Subprocess proof that the production fatal boundary terminates blocked peers.
use super::fatal;
use crate::{
	QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
	error::BackendResult,
};
use googletest::prelude::*;

#[gtest]
fn native_rank_local_failure_aborts_blocked_peer() -> googletest::Result<()> {
	if std::env::var_os("QUEST_MATCHING_NATIVE_FAILURE_CHILD").is_none() {
		let output = std::process::Command::new("timeout")
			.args(["20s", "mpiexec", "-n", "2"])
			.arg(std::env::current_exe()?)
			.args([
				"--exact",
				"qsvt::matching::collective::failure_tests::native_rank_local_failure_aborts_blocked_peer",
				"--nocapture",
				"--test-threads=1",
			])
			.env("QUEST_MATCHING_NATIVE_FAILURE_CHILD", "1")
			.output()?;
		expect_false!(output.status.success());
		expect_ne!(output.status.code(), Some(124));
		expect_that!(
			String::from_utf8_lossy(&output.stderr),
			contains_substring("unrecoverable distributed MPI operation or cleanup failure")
		);
		expect_that!(
			String::from_utf8_lossy(&output.stderr),
			contains_substring(
				"injecting checked matching partition failure after native execution"
			)
		);
		return Ok(());
	}
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	let environment = CollectiveEnvironment::builder(&comm)?.build()?;
	let mut register = environment.state_vector_local(QubitCount::new(2)?)?;
	let mut lane = comm.collective_lane()?;
	let rank = comm.rank()?;
	fatal(|| {
		// Both ranks have entered native execution and changed their state before
		// either the injected error or the peer's blocking receive begins.
		register.inner.h(0)?;
		let entered = lane
			.all_agree(true)
			.context("synchronizing native failure fixture")?;
		if !entered {
			return Err(crate::Error::Value(
				"native failure fixture synchronization",
			));
		}
		if rank == 0 {
			eprintln!("injecting checked matching partition failure after native execution");
			let mut output = [quest_sys::QuestComplex { re: 0.0, im: 0.0 }];
			quest_sys::read_local_qureg_amps(
				&register.inner.native,
				i64::try_from(register.deployment().local_amplitudes())
					.map_err(|_| crate::Error::Overflow)?,
				&mut output,
			)
			.context("injected native local-partition range failure")?;
		} else {
			let mut pending = [0u8; 8];
			lane.receive_bytes(&mut pending, 0, 3050)
				.context("waiting for deliberately absent matching response")?;
		}
		Ok(())
	});
	Err(std::io::Error::other(
		"native execution failure returned without aborting peers",
	))?
}
