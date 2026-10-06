//! One captured native configuration for Cargo and standalone tooling.
use crate::{BridgeInputs, NativePackage, Result, absolute, invalid, run, validate_target};
use std::{
	collections::BTreeMap,
	env,
	ffi::OsString,
	path::{Path, PathBuf},
	process::Command,
};

/// Native target, profile and environment used throughout one `CMake` evaluation.
/// Compiler/search variables and module state are captured before configuration.
#[derive(Clone, Debug)]
pub struct NativeBuildContext {
	pub host: String,
	pub target: String,
	pub work_directory: PathBuf,
	pub profile: String,
	pub(crate) environment: BTreeMap<OsString, OsString>,
	pub(crate) cargo: bool,
}

impl NativeBuildContext {
	/// Capture a Cargo build script's native build context.
	/// # Errors
	/// Returns an error for missing Cargo variables or unsupported targets.
	pub fn from_cargo_env() -> Result<Self> {
		crate::watch_environment();
		let host = env::var("HOST").map_err(|_| {
			invalid("HOST is missing; use NativeBuildContext::for_tooling outside Cargo")
		})?;
		let target = env::var("TARGET").map_err(|_| {
			invalid("TARGET is missing; use NativeBuildContext::for_tooling outside Cargo")
		})?;
		let work = env::var_os("OUT_DIR").ok_or_else(|| invalid("OUT_DIR is missing"))?;
		Self::capture(
			&PathBuf::from(work).join("quest-native"),
			&host,
			&target,
			true,
		)
	}

	/// Capture a standalone tooling context, with no Cargo output on stdout.
	/// # Errors
	/// Returns an error for unsupported targets or a failed Rust host query.
	pub fn for_tooling(work_directory: impl AsRef<Path>, target: Option<&str>) -> Result<Self> {
		let output =
			run(Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())).arg("-vV"))?;
		let version = String::from_utf8_lossy(&output.stdout);
		let host = version
			.lines()
			.find_map(|line| line.strip_prefix("host: "))
			.ok_or_else(|| invalid("rustc -vV did not identify its host"))?;
		Self::capture(work_directory.as_ref(), host, target.unwrap_or(host), false)
	}

	pub(crate) fn capture(work: &Path, host: &str, target: &str, cargo: bool) -> Result<Self> {
		validate_target(host, target)?;
		let environment = env::vars_os().collect::<BTreeMap<_, _>>();
		crate::reject_obsolete_environment(|name| {
			environment.get(std::ffi::OsStr::new(name)).cloned()
		})?;
		crate::reject_compiler_overrides(target, |name| {
			environment.get(std::ffi::OsStr::new(name)).cloned()
		})?;
		let profile = if cargo {
			match (
				env::var("OPT_LEVEL")
					.unwrap_or_else(|_| "0".into())
					.as_str(),
				env::var("DEBUG").as_deref() == Ok("true"),
			) {
				("0", _) => "Debug",
				("s" | "z", _) => "MinSizeRel",
				(_, true) => "RelWithDebInfo",
				_ => "Release",
			}
		} else {
			"Release"
		};
		if cargo {
			for name in crate::compiler_override_variables(target) {
				println!("cargo:rerun-if-env-changed={name}");
			}
			for name in environment
				.keys()
				.filter_map(|key| key.to_str())
				.filter(|name| {
					name.starts_with("CRAY") || name.starts_with("PE_") || name.starts_with("LMOD_")
				}) {
				println!("cargo:rerun-if-env-changed={name}");
			}
		}
		Ok(Self {
			host: host.into(),
			target: target.into(),
			work_directory: absolute(work)?,
			profile: profile.into(),
			environment,
			cargo,
		})
	}

	/// Evaluate the installed package and compile its ABI admission source.
	/// # Errors
	/// Returns an error for failed `CMake` evaluation, ABI or link validation.
	pub fn discover(&self) -> Result<NativePackage> {
		crate::probe::discover_context(self, None)
	}

	/// Compile a generated bridge in this exact native context.
	/// # Errors
	/// Returns an error for failed discovery or bridge compilation.
	pub fn build_bridge(&self, inputs: &BridgeInputs) -> Result<NativePackage> {
		crate::probe::discover_context(self, Some(inputs))
	}

	pub(crate) fn value(&self, name: &str) -> Option<&std::ffi::OsStr> {
		self.environment
			.get(std::ffi::OsStr::new(name))
			.map(OsString::as_os_str)
	}
	/// Apply the captured native environment to a child process.
	pub fn apply_environment(&self, command: &mut Command) {
		command.env_clear().envs(&self.environment);
	}
	pub(crate) fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
		let mut command = Command::new(program);
		self.apply_environment(&mut command);
		command
	}
	pub(crate) fn cmake(&self) -> PathBuf {
		PathBuf::from(
			self.value("CMAKE")
				.unwrap_or_else(|| std::ffi::OsStr::new("cmake")),
		)
	}
}
