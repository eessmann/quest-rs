//! Bounded, argument-vector MPI test supervision.
//!
//! Run the test harness once, outside any MPI rank. `QUEST_MPI_LAUNCHER` selects
//! `local` (the default, `mpiexec`) or `slurm` (`srun` within an allocation).
//! `QUEST_MPI_LAUNCHER_ARGS` is a JSON array of additional arguments, never shell
//! text. `QUEST_MPI_LAUNCHER_EXECUTABLE` optionally overrides the launcher path.
//! Slurm cancellation uses only the job/step identity emitted by our rank wrapper.
use std::{
	ffi::{OsStr, OsString},
	fmt,
	io::{self, Read},
	path::PathBuf,
	process::{Child, Command, ExitStatus, Stdio},
	sync::atomic::{AtomicU64, Ordering},
	time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const EXPECTED_RANKS: &str = "QUEST_MPI_EXPECTED_RANKS";
const CHILD_MARKER: &str = "QUEST_MPI_SUPERVISED_CHILD";
const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;
static NEXT_LAUNCH: AtomicU64 = AtomicU64::new(0);

/// The program and extra argv used to launch ranks. No arguments are shell-evaluated.
#[derive(Clone, Debug)]
pub enum Launcher {
	Local {
		program: PathBuf,
		args: Vec<OsString>,
	},
	Slurm {
		program: PathBuf,
		args: Vec<OsString>,
		job_id: String,
		scancel: PathBuf,
	},
}
impl Launcher {
	/// # Errors
	/// Returns an error for malformed configuration or a missing Slurm allocation.
	pub fn from_env() -> io::Result<Self> {
		let args: Vec<OsString> = match std::env::var("QUEST_MPI_LAUNCHER_ARGS") {
			Ok(value) => serde_json::from_str::<Vec<String>>(&value)
				.map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
				.into_iter()
				.map(OsString::from)
				.collect(),
			Err(std::env::VarError::NotPresent) => vec![],
			Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidInput, error)),
		};
		let backend = std::env::var("QUEST_MPI_LAUNCHER").unwrap_or_else(|_| "local".into());
		let program = std::env::var_os("QUEST_MPI_LAUNCHER_EXECUTABLE").map(PathBuf::from);
		match backend.as_str() {
			"local" => Ok(Self::Local {
				program: program.unwrap_or_else(|| "mpiexec".into()),
				args,
			}),
			"slurm" => {
				let job_id = std::env::var("SLURM_JOB_ID").map_err(|_| {
					io::Error::other("Slurm MPI tests require an existing SLURM_JOB_ID allocation")
				})?;
				if !decimal(&job_id) {
					return Err(io::Error::other("invalid SLURM_JOB_ID"));
				}
				Ok(Self::Slurm {
					program: program.unwrap_or_else(|| "srun".into()),
					args,
					job_id,
					scancel: "scancel".into(),
				})
			}
			_ => Err(io::Error::other(
				"QUEST_MPI_LAUNCHER must be local or slurm",
			)),
		}
	}
}

/// Check the actual communicator size before any test-specific collective work.
/// # Errors
/// Returns an error when the supervisor count is missing or differs from MPI.
pub fn assert_rank_count(actual: i32) -> io::Result<()> {
	let expected = std::env::var(EXPECTED_RANKS)
		.map_err(|_| io::Error::other("missing supervised MPI rank count"))?
		.parse::<i32>()
		.map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
	if actual != expected {
		return Err(io::Error::other(format!(
			"MPI rank count mismatch: expected {expected}, received {actual}"
		)));
	}
	Ok(())
}

/// Slurm batch scripts may inherit rank-zero task metadata without being an srun step.
fn is_slurm_batch_coordinator() -> bool {
	let Some(job) = std::env::var_os("SLURM_JOB_ID") else {
		return false;
	};
	job.to_str().is_some_and(decimal)
		&& std::env::var_os("SLURM_JOBID").is_none_or(|alias| alias == job)
		&& std::env::var_os("SLURM_PROCID").as_deref() == Some(OsStr::new("0"))
		&& ["SLURM_LOCALID", "SLURM_NODEID"]
			.into_iter()
			.all(|key| std::env::var_os(key).is_none_or(|value| value == OsStr::new("0")))
		&& ["SLURM_STEP_ID", "SLURM_STEPID"]
			.into_iter()
			.all(|key| std::env::var_os(key).is_none_or(|value| value == OsStr::new("batch")))
}

/// Reject accidental invocation from an MPI rank; allow an unambiguous batch coordinator.
fn require_coordinator() -> io::Result<()> {
	for key in [
		CHILD_MARKER,
		"OMPI_COMM_WORLD_RANK",
		"PMI_RANK",
		"PMIX_RANK",
		"MV2_COMM_WORLD_RANK",
	] {
		if std::env::var_os(key).is_some() {
			return Err(io::Error::other(format!(
				"nested MPI launch refused: {key} is set; run one coordinator outside mpiexec/srun"
			)));
		}
	}
	if [
		"SLURM_PROCID",
		"SLURM_LOCALID",
		"SLURM_NODEID",
		"SLURM_STEP_ID",
		"SLURM_STEPID",
	]
	.into_iter()
	.any(|key| std::env::var_os(key).is_some())
		&& !is_slurm_batch_coordinator()
	{
		return Err(io::Error::other(
			"nested MPI launch refused: Slurm task metadata is not an unambiguous rank-zero batch coordinator",
		));
	}
	Ok(())
}

#[derive(Debug)]
pub struct RunStatus {
	pub exit: ExitStatus,
	pub timed_out: bool,
}
impl RunStatus {
	#[must_use]
	pub fn success(&self) -> bool {
		self.exit.success() && !self.timed_out
	}
	#[must_use]
	pub fn code(&self) -> Option<i32> {
		self.exit.code()
	}
}
impl fmt::Display for RunStatus {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}, timed_out={}", self.exit, self.timed_out)
	}
}
#[derive(Debug)]
pub struct Cancellation {
	pub step: String,
	/// The cancellation command succeeded; this alone is not termination evidence.
	pub success: bool,
	/// A bounded successful scheduler query no longer listed the owned step.
	pub termination_confirmed: bool,
	pub detail: String,
}
#[derive(Debug)]
pub struct RunOutput {
	pub status: RunStatus,
	pub stdout: Vec<u8>,
	pub stderr: Vec<u8>,
	pub stdout_truncated: bool,
	pub stderr_truncated: bool,
	/// Owned remote step, retained even after a normal completion.
	pub slurm_step: Option<String>,
	pub cancellation: Option<Cancellation>,
}

pub struct MpiTest {
	launcher: Launcher,
	ranks: usize,
	deadline: Duration,
	executable: PathBuf,
	args: Vec<OsString>,
	env: Vec<(OsString, OsString)>,
	output_limit: usize,
	suppress_stderr: bool,
}
impl MpiTest {
	/// # Errors
	/// Rejects invalid configuration, nested launch, or unavailable current executable.
	pub fn new(ranks: usize, deadline: Duration) -> io::Result<Self> {
		Self::with_launcher(Launcher::from_env()?, ranks, deadline)
	}
	/// # Errors
	/// Rejects zero ranks/deadline, invalid allocation identity, and nested launch.
	pub fn with_launcher(launcher: Launcher, ranks: usize, deadline: Duration) -> io::Result<Self> {
		require_coordinator()?;
		if ranks == 0 || i32::try_from(ranks).is_err() || deadline.is_zero() {
			return Err(io::Error::new(
				io::ErrorKind::InvalidInput,
				"rank count and deadline must be positive, with rank count fitting MPI int",
			));
		}
		if let Launcher::Slurm { job_id, .. } = &launcher
			&& !decimal(job_id)
		{
			return Err(io::Error::other("invalid Slurm allocation identity"));
		}
		Ok(Self {
			launcher,
			ranks,
			deadline,
			executable: std::env::current_exe()?,
			args: vec![],
			env: vec![],
			output_limit: DEFAULT_OUTPUT_LIMIT,
			suppress_stderr: false,
		})
	}
	pub fn executable(&mut self, executable: impl Into<PathBuf>) -> &mut Self {
		self.executable = executable.into();
		self
	}
	pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
		self.args.push(arg.as_ref().into());
		self
	}
	pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
		self.args
			.extend(args.into_iter().map(|v| v.as_ref().into()));
		self
	}
	pub fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
		self.env.push((key.as_ref().into(), value.as_ref().into()));
		self
	}
	pub const fn output_limit(&mut self, bytes: usize) -> &mut Self {
		self.output_limit = bytes;
		self
	}
	pub const fn suppress_stderr(&mut self) -> &mut Self {
		self.suppress_stderr = true;
		self
	}
	/// # Errors
	/// Returns launcher spawn, pipe, or process supervision errors.
	pub fn status(&mut self) -> io::Result<RunStatus> {
		use std::io::Write;
		let output = self.output()?;
		// Preserve Command::status-style diagnostics, including successful scientific receipts.
		// output() has already applied the capture bounds and optional stderr suppression.
		std::io::stdout().lock().write_all(&output.stdout)?;
		std::io::stderr().lock().write_all(&output.stderr)?;
		Ok(output.status)
	}
	/// # Errors
	/// Returns launcher spawn, pipe, or process supervision errors.
	pub fn output(&mut self) -> io::Result<RunOutput> {
		require_coordinator()?;
		let token = format!(
			"QUEST_MPI_STEP_{}_{}_{}=",
			std::process::id(),
			NEXT_LAUNCH.fetch_add(1, Ordering::Relaxed),
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.unwrap_or_default()
				.as_nanos()
		);
		let (mut command, cleanup) = match &self.launcher {
			Launcher::Local { program, args } => {
				let mut cmd = Command::new(program);
				cmd.args(args)
					.args(["-n", &self.ranks.to_string()])
					.arg(&self.executable);
				(cmd, None)
			}
			Launcher::Slurm {
				program,
				args,
				job_id,
				scancel,
			} => {
				let mut cmd = Command::new(program);
				cmd.args(args).arg(format!("--ntasks={}", self.ranks))
                    .args(["/bin/sh", "-c", "if [ \"${SLURM_PROCID:-}\" = 0 ]; then printf '%s%s.%s\\n' \"$QUEST_MPI_STEP_TOKEN\" \"$SLURM_JOB_ID\" \"$SLURM_STEP_ID\" >&2; fi; exec \"$@\"", "quest-mpi-child"])
                    .arg(&self.executable).env("QUEST_MPI_STEP_TOKEN", &token);
				(
					cmd,
					Some(SlurmCleanup {
						token,
						job_id: job_id.clone(),
						scancel: scancel.clone(),
						env: self.env.clone(),
					}),
				)
			}
		};
		command
			.args(&self.args)
			.envs(self.env.iter().cloned())
			.env(EXPECTED_RANKS, self.ranks.to_string())
			.env(CHILD_MARKER, "1");
		let started = Instant::now();
		let mut output = supervise(command, self.deadline, self.output_limit, cleanup.as_ref())?;
		if self.suppress_stderr {
			output.stderr.clear();
		}
		eprintln!(
			"MPI supervisor: launcher={:?}, ranks={}, elapsed={:?}, status={}, stdout_truncated={}, stderr_truncated={}, slurm_step={:?}, cancellation={:?}",
			self.launcher,
			self.ranks,
			started.elapsed(),
			output.status,
			output.stdout_truncated,
			output.stderr_truncated,
			output.slurm_step,
			output.cancellation
		);
		Ok(output)
	}
}
fn decimal(value: &str) -> bool {
	!value.is_empty() && value.bytes().all(|c| c.is_ascii_digit())
}
struct SlurmCleanup {
	token: String,
	job_id: String,
	scancel: PathBuf,
	env: Vec<(OsString, OsString)>,
}

#[cfg(unix)]
fn supervise(
	mut command: Command,
	deadline: Duration,
	limit: usize,
	cleanup: Option<&SlurmCleanup>,
) -> io::Result<RunOutput> {
	use std::os::{fd::AsFd, unix::process::CommandExt};
	command
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.process_group(0);
	let mut process = ProcessGroup(command.spawn()?);
	let mut stdout = process
		.0
		.stdout
		.take()
		.ok_or_else(|| io::Error::other("missing stdout pipe"))?;
	let mut stderr = process
		.0
		.stderr
		.take()
		.ok_or_else(|| io::Error::other("missing stderr pipe"))?;
	for fd in [stdout.as_fd(), stderr.as_fd()] {
		let flags = rustix::fs::fcntl_getfl(fd)?;
		rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK)?;
	}
	let started = Instant::now();
	let mut out = Capture::new(limit);
	let mut err = Capture::new(limit);
	let mut step = StepIdentity::new(cleanup);
	let mut exit = None;
	let mut timed_out = false;
	let mut cancellation = None;
	loop {
		out.drain(&mut stdout, None)?;
		err.drain(&mut stderr, Some(&mut step))?;
		if exit.is_none() {
			exit = process.0.try_wait()?;
		}
		if exit.is_some() && out.closed && err.closed {
			break;
		}
		if started.elapsed() >= deadline {
			timed_out = true;
			if let (Some(cleanup), Some(owned_step)) = (&cleanup, &step.step) {
				cancellation = Some(cancel_step(cleanup, owned_step));
			}
			// Kill the entire group, including launcher proxies and pipe-holding grandchildren.
			process.kill_group()?;
			if exit.is_none() {
				exit = Some(process.0.wait()?);
			}
			if let (Some(cleanup), Some(cancellation)) = (cleanup, cancellation.as_mut()) {
				cancellation.termination_confirmed =
					confirm_step_termination(cleanup, &cancellation.step);
			}
			// Only a bounded final drain: foreign descendants must not defeat the deadline.
			for _ in 0..8 {
				out.drain(&mut stdout, None)?;
				err.drain(&mut stderr, None)?;
				if out.closed && err.closed {
					break;
				}
				std::thread::sleep(Duration::from_millis(5));
			}
			break;
		}
		std::thread::sleep(Duration::from_millis(5));
	}
	Ok(RunOutput {
		status: RunStatus {
			exit: exit.ok_or_else(|| io::Error::other("launcher exit status missing"))?,
			timed_out,
		},
		stdout: out.bytes,
		stderr: err.bytes,
		stdout_truncated: out.truncated,
		stderr_truncated: err.truncated,
		slurm_step: step.step,
		cancellation,
	})
}

#[cfg(unix)]
fn cleanup_environment(command: &mut Command, cleanup: &SlurmCleanup) {
	command.envs(cleanup.env.iter().cloned());
	// Filters can make a live step disappear from squeue or prevent its cancellation.
	// Remove both inherited values and explicit test overrides after applying them.
	for name in std::env::vars_os()
		.map(|(name, _)| name)
		.chain(cleanup.env.iter().map(|(name, _)| name.clone()))
	{
		if name.to_str().is_some_and(|name| {
			name.starts_with("SQUEUE_") || name.starts_with("SCANCEL_") || name == "SLURM_CLUSTERS"
		}) {
			command.env_remove(name);
		}
	}
}

#[cfg(unix)]
fn cancel_step(cleanup: &SlurmCleanup, step: &str) -> Cancellation {
	let mut cancel = Command::new(&cleanup.scancel);
	cancel.args(["--signal=KILL", step]);
	cleanup_environment(&mut cancel, cleanup);
	match supervise(cancel, Duration::from_secs(2), 8192, None) {
		Ok(result) => Cancellation {
			step: step.to_owned(),
			success: result.status.success(),
			termination_confirmed: false,
			detail: format!(
				"{}; {}",
				result.status,
				String::from_utf8_lossy(&result.stderr)
			),
		},
		Err(error) => Cancellation {
			step: step.to_owned(),
			success: false,
			termination_confirmed: false,
			detail: error.to_string(),
		},
	}
}

#[cfg(unix)]
fn confirm_step_termination(cleanup: &SlurmCleanup, step: &str) -> bool {
	let started = Instant::now();
	let job_prefix = format!("{}.", cleanup.job_id);
	while started.elapsed() < Duration::from_secs(2) {
		let mut query = Command::new("squeue");
		query.args([
			"--local",
			"--steps",
			"--noheader",
			"--jobs",
			&cleanup.job_id,
			"--format=%i",
		]);
		cleanup_environment(&mut query, cleanup);
		let Ok(result) = supervise(query, Duration::from_millis(500), 8192, None) else {
			return false;
		};
		if !result.status.success() || result.stdout_truncated {
			return false;
		}
		let Ok(listing) = std::str::from_utf8(&result.stdout) else {
			return false;
		};
		let mut found = false;
		for line in listing
			.lines()
			.map(str::trim)
			.filter(|line| !line.is_empty())
		{
			let Some(suffix) = line.strip_prefix(&job_prefix) else {
				return false;
			};
			if !decimal(suffix) && !matches!(suffix, "batch" | "extern" | "interactive") {
				return false;
			}
			found |= line == step;
		}
		if !found {
			return true;
		}
		std::thread::sleep(Duration::from_millis(25));
	}
	false
}
#[cfg(not(unix))]
fn supervise(_: Command, _: Duration, _: usize, _: Option<&SlurmCleanup>) -> io::Result<RunOutput> {
	Err(io::Error::new(
		io::ErrorKind::Unsupported,
		"MPI test process-group supervision requires Unix",
	))
}

#[cfg(unix)]
struct ProcessGroup(Child);
#[cfg(unix)]
impl ProcessGroup {
	fn kill_group(&self) -> io::Result<()> {
		let group = rustix::process::Pid::from_child(&self.0);
		// CommandExt::process_group creates a group owned by this child.
		if let Err(error) =
			rustix::process::kill_process_group(group, rustix::process::Signal::KILL)
			&& error != rustix::io::Errno::SRCH
		{
			return Err(error.into());
		}
		Ok(())
	}
}
#[cfg(unix)]
impl Drop for ProcessGroup {
	fn drop(&mut self) {
		let _ = self.kill_group();
		let _ = self.0.wait();
	}
}

struct Capture {
	bytes: Vec<u8>,
	limit: usize,
	truncated: bool,
	closed: bool,
}
impl Capture {
	const fn new(limit: usize) -> Self {
		Self {
			bytes: Vec::new(),
			limit,
			truncated: false,
			closed: false,
		}
	}
	fn drain(
		&mut self,
		pipe: &mut impl Read,
		mut identity: Option<&mut StepIdentity>,
	) -> io::Result<()> {
		let mut buffer = [0u8; 8192];
		// Bound work per poll so continuous output cannot postpone the deadline.
		for _ in 0..8 {
			match pipe.read(&mut buffer) {
				Ok(0) => {
					self.closed = true;
					break;
				}
				Ok(count) => {
					let bytes = buffer
						.get(..count)
						.ok_or_else(|| io::Error::other("invalid pipe read size"))?;
					if let Some(identity) = identity.as_mut() {
						identity.feed(bytes);
					}
					let keep = count.min(self.limit.saturating_sub(self.bytes.len()));
					self.bytes.extend(bytes.iter().take(keep).copied());
					self.truncated |= keep < count;
				}
				Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
				Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
				Err(error) => return Err(error),
			}
		}
		Ok(())
	}
}
struct StepIdentity {
	prefix: Option<String>,
	pending: Vec<u8>,
	oversized: bool,
	step: Option<String>,
}
impl StepIdentity {
	fn new(cleanup: Option<&SlurmCleanup>) -> Self {
		Self {
			prefix: cleanup.map(|c| format!("{}{}.", c.token, c.job_id)),
			pending: vec![],
			oversized: false,
			step: None,
		}
	}
	fn feed(&mut self, bytes: &[u8]) {
		let Some(prefix) = &self.prefix else {
			return;
		};
		for &byte in bytes {
			if byte == b'\n' {
				if !self.oversized
					&& let Ok(line) = std::str::from_utf8(&self.pending)
					&& let Some((_, step)) = line.split_once(prefix.as_str())
					&& decimal(step)
				{
					self.step = Some(format!(
						"{}.{}",
						prefix
							.split('=')
							.next_back()
							.unwrap_or("")
							.trim_end_matches('.'),
						step
					));
				}
				self.pending.clear();
				self.oversized = false;
			} else if self.pending.len() < 4096 {
				self.pending.push(byte);
			} else {
				self.oversized = true;
			}
		}
	}
}
