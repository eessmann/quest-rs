#![cfg(unix)]
#![allow(
	clippy::expect_used,
	clippy::unwrap_used,
	clippy::panic_in_result_fn,
	reason = "Tests use assertions to diagnose violations of the supervisor contract"
)]
use quest_test_support::mpi::{Launcher, MpiTest};
use std::{
	fs, io,
	path::PathBuf,
	sync::atomic::{AtomicUsize, Ordering},
	time::{Duration, Instant},
};
static SERIAL: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
	fn new() -> io::Result<Self> {
		let path = std::env::temp_dir().join(format!(
			"quest-supervisor-{}-{}",
			std::process::id(),
			SERIAL.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir(&path)?;
		Ok(Self(path))
	}
	fn script(&self, name: &str, body: &str) -> io::Result<PathBuf> {
		use std::os::unix::fs::PermissionsExt;
		let path = self.0.join(name);
		fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
		fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
		Ok(path)
	}
	fn local(&self, deadline: Duration) -> io::Result<MpiTest> {
		let launcher = self.script(
			"fake mpiexec",
			"test \"$1\" = -n && test \"$2\" = 3 || exit 81\nshift 2\nexec \"$@\"",
		)?;
		MpiTest::with_launcher(
			Launcher::Local {
				program: launcher,
				args: vec![],
			},
			3,
			deadline,
		)
	}
}
impl Drop for Fixture {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

#[test]
fn launcher_preserves_argument_boundaries_and_nonzero_status() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let child = fixture.script("child with spaces", "test \"$1\" = 'argument with spaces' || exit 82\nprintf 'stdout witness'\nprintf 'stderr witness' >&2\nexit 7")?;
	let output = fixture
		.local(Duration::from_secs(5))?
		.executable(child)
		.args(["argument with spaces"])
		.output()?;
	assert_eq!(output.status.exit.code(), Some(7));
	assert!(!output.status.timed_out);
	assert!(!output.status.success());
	assert_eq!(output.stdout, b"stdout witness");
	assert_eq!(output.stderr, b"stderr witness");
	Ok(())
}

#[test]
fn both_output_streams_are_drained_but_capture_is_bounded() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let child = fixture.script(
		"flood",
		"i=0; while test $i -lt 20000; do printf 'abcdefghij'; printf '0123456789' >&2; i=$((i+1)); done",
	)?;
	let output = fixture
		.local(Duration::from_secs(10))?
		.executable(child)
		.output_limit(1024)
		.output()?;
	assert!(output.status.success(), "{output:?}");
	assert_eq!(output.stdout.len(), 1024);
	assert_eq!(output.stderr.len(), 1024);
	assert!(output.stdout_truncated && output.stderr_truncated);
	Ok(())
}

#[test]
fn deadline_terminates_process_group_including_pipe_holding_descendants() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let witness = fixture.0.join("descendant-survived");
	let child = fixture.script(
		"stubborn",
		"trap '' TERM\n(sleep 1; touch \"$WITNESS\") &\nwait",
	)?;
	let started = Instant::now();
	let output = fixture
		.local(Duration::from_millis(100))?
		.executable(child)
		.env("WITNESS", &witness)
		.output()?;
	assert!(output.status.timed_out);
	assert!(!output.status.success());
	assert!(started.elapsed() < Duration::from_secs(3));
	std::thread::sleep(Duration::from_millis(1200));
	assert!(!witness.exists(), "descendant survived supervisor cleanup");
	Ok(())
}

#[test]
fn exited_launcher_cannot_leave_pipe_holding_children_running() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let witness = fixture.0.join("descendant-survived");
	let child = fixture.script("orphan", "(sleep 1; touch \"$WITNESS\") &\nexit 0")?;
	let output = fixture
		.local(Duration::from_millis(100))?
		.executable(child)
		.env("WITNESS", &witness)
		.output()?;
	assert!(
		output.status.timed_out,
		"open pipes are part of the deadline"
	);
	std::thread::sleep(Duration::from_millis(1200));
	assert!(!witness.exists());
	Ok(())
}

#[test]
fn slurm_timeout_cancels_only_the_owned_step_and_records_the_result() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let cancelled = fixture.0.join("cancelled");
	let launcher = fixture.script("fake srun", "test \"$1\" = --ntasks=3 || exit 83\nshift\nexport SLURM_JOB_ID=731 SLURM_STEP_ID=9 SLURM_PROCID=0\nexec \"$@\"")?;
	let scancel = fixture.script("fake scancel", "printf '%s\\n' \"$@\" > \"$CANCELLED\"")?;
	fixture.script("squeue", "test \"$1\" = --local || exit 81\nwhile test \"$#\" -gt 0; do\n  if test \"$1\" = --jobs; then test \"$2\" = 731 || exit 82; fi\n  shift\ndone\nprintf '731.10\\n'")?;
	let child = fixture.script("hang", "sleep 5")?;
	let output = MpiTest::with_launcher(
		Launcher::Slurm {
			program: launcher,
			args: vec![],
			job_id: "731".into(),
			scancel,
		},
		3,
		Duration::from_millis(150),
	)?
	.executable(child)
	.env("CANCELLED", &cancelled)
	.env("PATH", format!("{}:/usr/bin:/bin", fixture.0.display()))
	.output()?;
	assert!(output.status.timed_out);
	let cancellation = output.cancellation.expect("owned step cancellation record");
	assert_eq!(cancellation.step, "731.9");
	assert!(cancellation.success);
	assert!(cancellation.termination_confirmed);
	assert_eq!(fs::read_to_string(cancelled)?, "--signal=KILL\n731.9\n");
	Ok(())
}

#[test]
fn successful_cancel_does_not_prove_remote_termination() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let launcher = fixture.script(
		"srun",
		"shift\nexport SLURM_JOB_ID=731 SLURM_STEP_ID=9 SLURM_PROCID=0\nexec \"$@\"",
	)?;
	let scancel = fixture.script("scancel", "exit 0")?;
	fixture.script("squeue", "printf '731.9\\n'")?;
	let child = fixture.script("hang", "sleep 5")?;
	let started = Instant::now();
	let output = MpiTest::with_launcher(
		Launcher::Slurm {
			program: launcher,
			args: vec![],
			job_id: "731".into(),
			scancel,
		},
		3,
		Duration::from_millis(100),
	)?
	.executable(child)
	.env("PATH", format!("{}:/usr/bin:/bin", fixture.0.display()))
	.output()?;
	assert!(output.status.timed_out);
	let cancellation = output.cancellation.expect("owned step cancellation");
	assert!(cancellation.success);
	assert!(!cancellation.termination_confirmed);
	assert!(started.elapsed() < Duration::from_secs(4));
	Ok(())
}

#[test]
fn unknown_slurm_step_never_cancels_the_allocation() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let cancelled = fixture.0.join("cancelled");
	let launcher = fixture.script("unstarted srun", "sleep 5")?;
	let scancel = fixture.script("fake scancel", "touch \"$CANCELLED\"")?;
	let output = MpiTest::with_launcher(
		Launcher::Slurm {
			program: launcher,
			args: vec![],
			job_id: "731".into(),
			scancel,
		},
		3,
		Duration::from_millis(100),
	)?
	.env("CANCELLED", &cancelled)
	.output()?;
	assert!(output.status.timed_out);
	assert!(output.cancellation.is_none());
	assert!(!cancelled.exists());
	Ok(())
}

#[test]
fn nested_launcher_probe() {
	if std::env::var_os("QUEST_NESTED_PROBE").is_none() {
		return;
	}
	let result = MpiTest::new(2, Duration::from_secs(1));
	assert!(
		result.is_err(),
		"an MPI rank must not create another launcher"
	);
	assert!(
		result
			.err()
			.unwrap()
			.to_string()
			.contains("nested MPI launch refused")
	);
}

#[test]
fn mpi_rank_environment_cannot_start_another_coordinator() -> io::Result<()> {
	for marker in [
		"PMI_RANK",
		"OMPI_COMM_WORLD_RANK",
		"SLURM_PROCID",
		"QUEST_MPI_SUPERVISED_CHILD",
	] {
		let output = std::process::Command::new(std::env::current_exe()?)
			.env_clear()
			.args(["--exact", "nested_launcher_probe", "--nocapture"])
			.env("QUEST_NESTED_PROBE", "1")
			.env(marker, "0")
			.output()?;
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stdout)
		);
	}
	Ok(())
}

#[test]
fn coordinator_environment_probe() -> io::Result<()> {
	if std::env::var_os("QUEST_COORDINATOR_PROBE").is_none() {
		return Ok(());
	}
	let output = MpiTest::with_launcher(
		Launcher::Local {
			program: "/bin/sh".into(),
			args: ["-c", "shift 2; exec \"$@\"", "coordinator-probe"]
				.into_iter()
				.map(Into::into)
				.collect(),
		},
		1,
		Duration::from_secs(1),
	)?
	.executable("/bin/echo")
	.arg("coordinator-ok")
	.output()?;
	assert!(output.status.success());
	assert_eq!(output.stdout, b"coordinator-ok\n");
	Ok(())
}

const COORDINATOR_CASES: &[(&[(&str, &str)], bool)] = &[
	(&[], true),
	(&[("SLURM_JOB_ID", "731")], true),
	(&[("SLURM_JOB_ID", "731"), ("SLURM_PROCID", "0")], true),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_JOBID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_LOCALID", "0"),
			("SLURM_NODEID", "0"),
		],
		true,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEP_ID", "batch"),
		],
		true,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEPID", "batch"),
		],
		true,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEP_ID", "batch"),
			("SLURM_STEPID", "batch"),
		],
		true,
	),
	(&[("SLURM_PROCID", "0")], false),
	(&[("SLURM_JOB_ID", "731"), ("SLURM_PROCID", "1")], false),
	(&[("SLURM_JOB_ID", "731"), ("SLURM_PROCID", "00")], false),
	(&[("SLURM_JOB_ID", "invalid"), ("SLURM_PROCID", "0")], false),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_JOBID", "732"),
			("SLURM_PROCID", "0"),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_JOBID", "invalid"),
			("SLURM_PROCID", "0"),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEP_ID", "0"),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEPID", "0"),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEP_ID", "batch"),
			("SLURM_STEPID", "0"),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEP_ID", ""),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_STEPID", "extern"),
		],
		false,
	),
	(&[("SLURM_JOB_ID", "731"), ("SLURM_STEP_ID", "0")], false),
	(
		&[("SLURM_JOB_ID", "731"), ("SLURM_STEP_ID", "batch")],
		false,
	),
	(&[("SLURM_JOB_ID", "731"), ("SLURM_LOCALID", "0")], false),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_LOCALID", "1"),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_NODEID", "1"),
		],
		false,
	),
	(
		&[
			("SLURM_JOB_ID", "731"),
			("SLURM_PROCID", "0"),
			("SLURM_LOCALID", "invalid"),
		],
		false,
	),
];

#[test]
fn coordinator_accepts_batch_rank_zero_but_rejects_launched_or_ambiguous_steps() -> io::Result<()> {
	for &(environment, accepted) in COORDINATOR_CASES {
		let output = std::process::Command::new(std::env::current_exe()?)
			.env_clear()
			.args(["--exact", "coordinator_environment_probe", "--nocapture"])
			.env("QUEST_COORDINATOR_PROBE", "1")
			.envs(environment.iter().copied())
			.output()?;
		assert_eq!(
			output.status.success(),
			accepted,
			"environment={environment:?}; stdout={}; stderr={}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
	}
	for marker in [
		"PMI_RANK",
		"PMIX_RANK",
		"OMPI_COMM_WORLD_RANK",
		"MV2_COMM_WORLD_RANK",
		"QUEST_MPI_SUPERVISED_CHILD",
	] {
		let output = std::process::Command::new(std::env::current_exe()?)
			.env_clear()
			.args(["--exact", "coordinator_environment_probe", "--nocapture"])
			.env("QUEST_COORDINATOR_PROBE", "1")
			.env("SLURM_JOB_ID", "731")
			.env("SLURM_PROCID", "0")
			.env(marker, "0")
			.output()?;
		assert!(
			!output.status.success(),
			"batch exception admitted {marker}"
		);
	}
	Ok(())
}

#[test]
fn rank_count_probe() {
	if std::env::var_os("QUEST_RANK_COUNT_PROBE").is_none() {
		return;
	}
	assert!(quest_test_support::mpi::assert_rank_count(4).is_ok());
	assert!(quest_test_support::mpi::assert_rank_count(1).is_err());
}

#[test]
fn actual_communicator_size_must_match_requested_ranks() -> io::Result<()> {
	let output = std::process::Command::new(std::env::current_exe()?)
		.args(["--exact", "rank_count_probe", "--nocapture"])
		.env("QUEST_RANK_COUNT_PROBE", "1")
		.env("QUEST_MPI_EXPECTED_RANKS", "4")
		.output()?;
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stdout)
	);
	Ok(())
}

#[test]
fn failed_slurm_cancellation_is_visible_in_the_report() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let launcher = fixture.script(
		"srun",
		"shift\nexport SLURM_JOB_ID=731 SLURM_STEP_ID=9 SLURM_PROCID=0\nexec \"$@\"",
	)?;
	let scancel = fixture.script("scancel", "printf 'cancel denied' >&2\nexit 19")?;
	let child = fixture.script("hang", "sleep 5")?;
	let output = MpiTest::with_launcher(
		Launcher::Slurm {
			program: launcher,
			args: vec![],
			job_id: "731".into(),
			scancel,
		},
		3,
		Duration::from_millis(100),
	)?
	.executable(child)
	.output()?;
	let cancellation = output.cancellation.expect("cancellation report");
	assert!(!cancellation.success);
	assert!(cancellation.detail.contains("cancel denied"));
	assert!(output.status.timed_out);
	Ok(())
}

#[test]
fn slurm_labelled_step_identity_is_recognized_after_capture_truncation() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let cancelled = fixture.0.join("cancelled");
	let launcher = fixture.script("srun", "printf 'noise longer than retained capture\\n' >&2\nprintf '0: %s731.9\\n' \"$QUEST_MPI_STEP_TOKEN\" >&2\nsleep 5")?;
	let scancel = fixture.script("scancel", "printf '%s\\n' \"$@\" > \"$CANCELLED\"")?;
	let output = MpiTest::with_launcher(
		Launcher::Slurm {
			program: launcher,
			args: vec![],
			job_id: "731".into(),
			scancel,
		},
		3,
		Duration::from_millis(100),
	)?
	.env("CANCELLED", &cancelled)
	.output_limit(8)
	.output()?;
	let cancellation = output
		.cancellation
		.expect("labelled owned step cancellation");
	assert_eq!(cancellation.step, "731.9");
	assert!(cancellation.success);
	assert!(output.stderr_truncated);
	assert_eq!(fs::read_to_string(cancelled)?, "--signal=KILL\n731.9\n");
	Ok(())
}

#[test]
fn slurm_confirmation_discards_filters_that_could_hide_a_live_step() -> io::Result<()> {
	let fixture = Fixture::new()?;
	// Isolate inherited environment changes in a child test process rather than
	// mutating the multithreaded test harness's process environment.
	if std::env::var_os("QUEST_SUPERVISOR_FILTER_CHILD").is_none() {
		let output = fixture
			.local(Duration::from_secs(5))?
			.executable("/bin/sh")
			.args([
				"-c",
				"unset QUEST_MPI_SUPERVISED_CHILD; exec \"$@\"",
				"isolated-filter-test",
			])
			.arg(std::env::current_exe()?)
			.args([
				"--exact",
				"slurm_confirmation_discards_filters_that_could_hide_a_live_step",
				"--nocapture",
			])
			.env("QUEST_SUPERVISOR_FILTER_CHILD", "1")
			.env("SQUEUE_STATES", "PENDING")
			.env("SCANCEL_STATE", "PENDING")
			.env("SLURM_CLUSTERS", "different-cluster")
			.output()?;
		assert!(output.status.success(), "{output:?}");
		return Ok(());
	}
	let launcher = fixture.script(
		"srun",
		"shift\nexport SLURM_JOB_ID=731 SLURM_STEP_ID=9 SLURM_PROCID=0\nexec \"$@\"",
	)?;
	let scancel = fixture.script(
		"scancel",
		"test -z \"${SCANCEL_STATE+x}${SCANCEL_NAME+x}${SLURM_CLUSTERS+x}\"",
	)?;
	fixture.script(
		"squeue",
		"if test -n \"${SQUEUE_STATES+x}${SQUEUE_NAMES+x}${SLURM_CLUSTERS+x}\"; then exit 0; fi\nprintf '731.9\\n'",
	)?;
	let child = fixture.script("hang", "sleep 5")?;
	let output = MpiTest::with_launcher(
		Launcher::Slurm {
			program: launcher,
			args: vec![],
			job_id: "731".into(),
			scancel,
		},
		3,
		Duration::from_millis(100),
	)?
	.executable(child)
	.env("PATH", format!("{}:/usr/bin:/bin", fixture.0.display()))
	.env("SQUEUE_NAMES", "another-step")
	.env("SCANCEL_NAME", "another-step")
	.output()?;
	let cancellation = output.cancellation.expect("owned step cancellation");
	assert!(cancellation.success, "filters leaked into scancel");
	assert!(
		!cancellation.termination_confirmed,
		"filtered listing hid the live owned step"
	);
	Ok(())
}

#[test]
fn slurm_confirmation_rejects_successful_but_malformed_listing() -> io::Result<()> {
	let fixture = Fixture::new()?;
	let launcher = fixture.script(
		"srun",
		"shift\nexport SLURM_JOB_ID=731 SLURM_STEP_ID=9 SLURM_PROCID=0\nexec \"$@\"",
	)?;
	let scancel = fixture.script("scancel", "exit 0")?;
	fixture.script(
		"squeue",
		"printf 'scheduler response unavailable\\n732.9\\n'",
	)?;
	let child = fixture.script("hang", "sleep 5")?;
	let output = MpiTest::with_launcher(
		Launcher::Slurm {
			program: launcher,
			args: vec![],
			job_id: "731".into(),
			scancel,
		},
		3,
		Duration::from_millis(100),
	)?
	.executable(child)
	.env("PATH", format!("{}:/usr/bin:/bin", fixture.0.display()))
	.output()?;
	let cancellation = output.cancellation.expect("owned step cancellation");
	assert!(cancellation.success);
	assert!(
		!cancellation.termination_confirmed,
		"unparseable listing was treated as step disappearance"
	);
	Ok(())
}
