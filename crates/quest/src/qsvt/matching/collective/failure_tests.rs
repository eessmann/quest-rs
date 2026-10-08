//! Subprocess proof that the production fatal boundary terminates blocked peers.
use super::fatal;
use crate::{
	QubitCount,
	collective::{CollectiveEnvironment, MpiRuntime},
	error::BackendResult,
};
use googletest::prelude::*;
use std::{
	fs::{File, OpenOptions},
	io::Write,
	path::{Path, PathBuf},
	sync::atomic::{AtomicU64, Ordering},
};

const CHILD_DIRECTORY: &str = "QUEST_MATCHING_NATIVE_FAILURE_DIRECTORY";
const RANGE_ERROR: &str = "local amplitude range exceeds this rank's partition";
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct WitnessDirectory(PathBuf);
impl WitnessDirectory {
	fn create() -> std::io::Result<Self> {
		for _ in 0..1024 {
			let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
			let path = std::env::temp_dir().join(format!(
				"quest-matching-fatal-{}-{serial}",
				std::process::id(),
			));
			let mut builder = std::fs::DirBuilder::new();
			#[cfg(unix)]
			{
				use std::os::unix::fs::DirBuilderExt;
				builder.mode(0o700);
			}
			match builder.create(&path) {
				Ok(()) => return Ok(Self(path)),
				Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
				Err(error) => return Err(error),
			}
		}
		Err(std::io::Error::other(
			"could not create a unique fatal fixture directory",
		))
	}
}
impl Drop for WitnessDirectory {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

fn witness(directory: &Path, name: &str, contents: &[u8]) -> crate::Result<()> {
	let operation = || -> std::io::Result<()> {
		let mut file: File = OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(directory.join(name))?;
		file.write_all(contents)?;
		file.sync_all()
	};
	operation().map_err(|_| crate::Error::Value("could not persist fatal fixture witness"))
}

fn assert_fatal_job(suppress_stderr: bool) -> googletest::Result<()> {
	let directory = WitnessDirectory::create()?;
	let mut command = quest_test_support::mpi::MpiTest::new(2, std::time::Duration::from_secs(20))?;
	command
		.args([
			"--exact",
			"qsvt::matching::collective::failure_tests::native_rank_local_failure_aborts_blocked_peer",
			"--nocapture",
			"--test-threads=1",
		])
		.env("QUEST_MATCHING_NATIVE_FAILURE_CHILD", "1")
		.env(CHILD_DIRECTORY, &directory.0);
	if suppress_stderr {
		command.suppress_stderr();
	}
	let output = command.output()?;
	eprintln!(
		"matching fatal fixture: suppressed_stderr={suppress_stderr}, status={}",
		output.status
	);
	expect_false!(output.status.success());
	expect_false!(output.status.timed_out);
	for rank in 0..2 {
		expect_eq!(
			std::fs::read(directory.0.join(format!("native-entered-{rank}")))?,
			b"native H completed\n".to_vec(),
		);
		expect_false!(
			directory
				.0
				.join(format!("after-fatal-return-{rank}"))
				.exists()
		);
	}
	expect_eq!(
		std::fs::read(directory.0.join("peer-ready"))?,
		b"unmatched receive stage ready\n".to_vec()
	);
	expect_eq!(
		std::fs::read(directory.0.join("checked-native-error"))?,
		RANGE_ERROR.as_bytes().to_vec()
	);
	if suppress_stderr {
		expect_true!(output.stderr.is_empty());
	}
	Ok(())
}

#[gtest]
fn native_rank_local_failure_aborts_blocked_peer() -> googletest::Result<()> {
	if std::env::var_os("QUEST_MATCHING_NATIVE_FAILURE_CHILD").is_none() {
		assert_fatal_job(false)?;
		return assert_fatal_job(true);
	}
	let directory = std::env::var_os(CHILD_DIRECTORY)
		.map(PathBuf::from)
		.ok_or_else(|| std::io::Error::other("fatal fixture directory was not supplied"))?;
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let environment = CollectiveEnvironment::builder(&comm)?.build()?;
	let mut register = environment.state_vector_local(QubitCount::new(2)?)?;
	let mut lane = comm.collective_lane()?;
	let rank = comm.rank()?;
	fatal(|| {
		register.inner.h(0)?;
		witness(
			&directory,
			&format!("native-entered-{rank}"),
			b"native H completed\n",
		)?;
		if rank == 1 {
			witness(&directory, "peer-ready", b"unmatched receive stage ready\n")?;
		}
		// Both native H calls and the peer-ready witness precede this agreement.
		// The peer will enter an unmatched receive; no response is ever sent.
		let entered = lane
			.all_agree(true)
			.context("synchronizing native failure fixture")?;
		if !entered {
			return Err(crate::Error::Value(
				"native failure fixture synchronization",
			));
		}
		if rank == 0 {
			let mut output = [quest_sys::QuestComplex { re: 0.0, im: 0.0 }];
			let failure = quest_sys::read_local_qureg_amps(
				&register.inner.native,
				i64::try_from(register.deployment().local_amplitudes())
					.map_err(|_| crate::Error::Overflow)?,
				&mut output,
			);
			if !matches!(&failure, Err(quest_sys::QuestError::Validation(message)) if message == RANGE_ERROR)
			{
				return Err(crate::Error::Value(
					"checked native range failure was not produced",
				));
			}
			witness(&directory, "checked-native-error", RANGE_ERROR.as_bytes())?;
			// Hand the witnessed original checked error to the unchanged fatal boundary.
			failure.context("injected native local-partition range failure")?;
		} else {
			let mut pending = [0u8; 8];
			lane.receive_bytes(&mut pending, 0, 3050)
				.context("waiting for deliberately absent matching response")?;
		}
		Ok(())
	});
	witness(
		&directory,
		&format!("after-fatal-return-{rank}"),
		b"fatal returned\n",
	)?;
	Err(std::io::Error::other(
		"native execution failure returned without aborting peers",
	))?
}

const BUFFER_RANGE_ERROR: &str = "indexed amplitude exceeds local partition";

fn assert_staged_fatal_job(suppress_stderr: bool) -> googletest::Result<()> {
	let directory = WitnessDirectory::create()?;
	let mut command = quest_test_support::mpi::MpiTest::new(2, std::time::Duration::from_secs(20))?;
	command
		.args([
			"--exact",
			"qsvt::matching::collective::failure_tests::staged_write_failure_aborts_blocked_peer",
			"--nocapture",
			"--test-threads=1",
		])
		.env("QUEST_MATCHING_STAGED_FAILURE_CHILD", "1")
		.env(CHILD_DIRECTORY, &directory.0);
	if suppress_stderr {
		command.suppress_stderr();
	}
	let output = command.output()?;
	eprintln!(
		"matching staged fatal fixture: suppressed_stderr={suppress_stderr}, status={}",
		output.status
	);
	expect_false!(output.status.success());
	expect_false!(output.status.timed_out);
	for rank in 0..2 {
		expect_eq!(
			std::fs::read(directory.0.join(format!("staged-write-{rank}")))?,
			b"distributed color H and first staged write completed\n".to_vec()
		);
		expect_false!(
			directory
				.0
				.join(format!("after-fatal-return-{rank}"))
				.exists()
		);
	}
	expect_eq!(
		std::fs::read(directory.0.join("peer-ready"))?,
		b"unmatched staged receive ready\n".to_vec()
	);
	expect_eq!(
		std::fs::read(directory.0.join("checked-staged-error"))?,
		BUFFER_RANGE_ERROR.as_bytes().to_vec()
	);
	if suppress_stderr {
		expect_true!(output.stderr.is_empty());
	}
	Ok(())
}

#[gtest]
fn staged_write_failure_aborts_blocked_peer() -> googletest::Result<()> {
	if std::env::var_os("QUEST_MATCHING_STAGED_FAILURE_CHILD").is_none() {
		assert_staged_fatal_job(false)?;
		return assert_staged_fatal_job(true);
	}
	let directory = std::env::var_os(CHILD_DIRECTORY)
		.map(PathBuf::from)
		.ok_or_else(|| std::io::Error::other("staged fixture directory was not supplied"))?;
	let runtime = MpiRuntime::initialize()?;
	let comm = runtime.world()?;
	quest_test_support::mpi::assert_rank_count(comm.size()?)?;
	let environment = CollectiveEnvironment::builder(&comm)?.build()?;
	let mut register = environment.state_vector_local(QubitCount::new(2)?)?;
	let mut scratch = environment.state_vector_local(QubitCount::new(2)?)?;
	let mut lane = comm.collective_lane()?;
	let rank = comm.rank()?;
	fatal(|| {
		if register.deployment().local_amplitudes() != 2 {
			return Err(crate::Error::Value(
				"staged fixture requires distributed color bit 1",
			));
		}
		// At P2 the high color bit uses native MPI communication scratch. The
		// exclusive routing state begins only after that native operation returns.
		register.inner.h(1)?;
		let mut state = super::RoutingState::stage(&mut register.inner, &mut scratch.inner)?;
		state.write_local(0, crate::Complex64::new(0.25, -0.5))?;
		witness(
			&directory,
			&format!("staged-write-{rank}"),
			b"distributed color H and first staged write completed\n",
		)?;
		if rank == 1 {
			witness(
				&directory,
				"peer-ready",
				b"unmatched staged receive ready\n",
			)?;
		}
		// Durable witnesses on every rank precede the trigger. Rank 1 enters an
		// unmatched private receive, while rank 0 returns the original staged error.
		if !lane
			.all_agree(true)
			.context("synchronizing staged failure fixture")?
		{
			return Err(crate::Error::Value(
				"staged failure fixture synchronization",
			));
		}
		if rank == 0 {
			let failure =
				state.write_local(state.local_amplitudes(), crate::Complex64::new(1.0, 0.0));
			if !matches!(&failure, Err(crate::Error::Backend {
				source: quest_sys::QuestError::Validation(message), ..
			}) if message == BUFFER_RANGE_ERROR)
			{
				return Err(crate::Error::Value(
					"checked staged range failure was not produced",
				));
			}
			witness(
				&directory,
				"checked-staged-error",
				BUFFER_RANGE_ERROR.as_bytes(),
			)?;
			failure?;
		} else {
			let mut pending = [0u8; 8];
			lane.receive_bytes(&mut pending, 0, 3051)
				.context("waiting for deliberately absent staged response")?;
		}
		Ok(())
	});
	witness(
		&directory,
		&format!("after-fatal-return-{rank}"),
		b"fatal returned\n",
	)?;
	Err(std::io::Error::other(
		"staged execution failure returned without aborting peers",
	))?
}
