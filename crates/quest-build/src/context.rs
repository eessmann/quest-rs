//! One captured native configuration for Cargo and standalone tooling.
use crate::{BridgeInputs, NativePackage, Result, absolute, invalid, validate_target};
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
	pub(crate) host: String,
	pub(crate) target: String,
	pub(crate) work_directory: PathBuf,
	pub(crate) profile: String,
	pub(crate) environment: BTreeMap<OsString, OsString>,
}

/// Editable native configuration. Evaluate a request to obtain a read-only context.
#[derive(Clone, Debug)]
pub struct NativeBuildRequest {
	pub host: String,
	pub target: String,
	pub work_directory: PathBuf,
	pub profile: String,
	pub environment: BTreeMap<OsString, OsString>,
}

impl NativeBuildRequest {
	/// Validate and capture this request without emitting Cargo directives.
	/// # Errors
	/// Rejects unsupported targets, profiles, and native overrides.
	pub fn capture(self) -> Result<NativeBuildContext> {
		validate_target(&self.host, &self.target)?;
		if !matches!(
			self.profile.as_str(),
			"Debug" | "Release" | "RelWithDebInfo" | "MinSizeRel"
		) {
			return Err(invalid(
				"native profile must be a supported CMake configuration",
			));
		}
		crate::reject_obsolete_environment(|name| {
			self.environment.get(std::ffi::OsStr::new(name)).cloned()
		})?;
		crate::reject_compiler_overrides(&self.target, |name| {
			self.environment.get(std::ffi::OsStr::new(name)).cloned()
		})?;
		Ok(NativeBuildContext {
			host: self.host,
			target: self.target,
			work_directory: absolute(&self.work_directory)?,
			profile: self.profile,
			environment: self.environment,
		})
	}
}

impl NativeBuildContext {
	/// Read the captured compiler host.
	#[must_use]
	pub fn host(&self) -> &str {
		&self.host
	}
	/// Read the admitted target.
	#[must_use]
	pub fn target(&self) -> &str {
		&self.target
	}
	/// Read the captured `CMake` profile.
	#[must_use]
	pub fn profile(&self) -> &str {
		&self.profile
	}
	/// Read the native work directory.
	#[must_use]
	pub fn work_directory(&self) -> &Path {
		&self.work_directory
	}
	/// Obtain an editable copy. Edits require a fresh evaluation.
	#[must_use]
	pub fn request(&self) -> NativeBuildRequest {
		NativeBuildRequest {
			host: self.host.clone(),
			target: self.target.clone(),
			work_directory: self.work_directory.clone(),
			profile: self.profile.clone(),
			environment: self.environment.clone(),
		}
	}
	pub(crate) fn validate_current_environment(&self) -> Result<()> {
		if self.environment != env::vars_os().collect::<BTreeMap<_, _>>() {
			return Err(invalid(
				"native context environment changed since discovery; capture and evaluate a fresh request before MPI discovery",
			));
		}
		Ok(())
	}

	/// Capture a Cargo build script's native build context.
	/// # Errors
	/// Returns an error for missing Cargo variables or unsupported targets.
	pub fn from_cargo_env() -> Result<Self> {
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
		let compiler = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
		let metadata = rustc_version::VersionMeta::for_command(compiler)
			.map_err(|error| invalid(format!("selected Rust compiler metadata: {error}")))?;
		Self::capture(
			work_directory.as_ref(),
			&metadata.host,
			target.unwrap_or(&metadata.host),
			false,
		)
	}

	pub(crate) fn capture(work: &Path, host: &str, target: &str, cargo: bool) -> Result<Self> {
		let environment = env::vars_os().collect::<BTreeMap<_, _>>();
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

		NativeBuildRequest {
			host: host.into(),
			target: target.into(),
			work_directory: work.to_owned(),
			profile: profile.into(),
			environment,
		}
		.capture()
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

	pub(crate) fn emit_environment_watches(&self) {
		crate::watch_environment();
		for name in crate::compiler_override_variables(&self.target) {
			println!("cargo:rerun-if-env-changed={name}");
		}
		for name in self
			.environment
			.keys()
			.filter_map(|key| key.to_str())
			.filter(|name| {
				name.starts_with("CRAY") || name.starts_with("PE_") || name.starts_with("LMOD_")
			}) {
			println!("cargo:rerun-if-env-changed={name}");
		}
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

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;

	#[gtest]
	fn edited_requests_require_fresh_configuration_and_cannot_change_snapshots()
	-> googletest::Result<()> {
		let directory = tempfile::tempdir()?;
		let context = NativeBuildContext::for_tooling(directory.path(), None)?;
		let mut request = context.request();
		request.profile = "Debug".into();
		request
			.environment
			.insert("MPICC".into(), "/opt/other mpi/bin/mpicc".into());
		let edited = request.capture()?;
		expect_eq!(context.profile(), "Release");
		expect_eq!(edited.profile(), "Debug");
		expect_true!(context.validate_current_environment().is_ok());
		expect_that!(
			edited
				.validate_current_environment()
				.unwrap_err()
				.to_string(),
			contains_substring("changed since discovery")
		);
		let mut invalid = context.request();
		invalid.target = "unsupported-cross-target".into();
		expect_true!(invalid.capture().is_err());
		let mut invalid = context.request();
		invalid.profile = "arbitrary".into();
		expect_true!(invalid.capture().is_err());
		Ok(())
	}

	#[cfg(unix)]
	#[gtest]
	fn selected_rustc_must_supply_complete_version_metadata() -> googletest::Result<()> {
		use std::os::unix::fs::PermissionsExt as _;
		const CHILD: &str = "QUEST_RUSTC_SELECTION_CHILD";
		if let Some(work) = env::var_os(CHILD) {
			expect_true!(NativeBuildContext::for_tooling(PathBuf::from(work), None).is_err());
			return Ok(());
		}
		let fixture = tempfile::tempdir()?;
		let compiler = fixture.path().join("selected rustc");
		std::fs::write(
			&compiler,
			"#!/bin/sh\nprintf 'host: x86_64-unknown-linux-gnu\\n'\n",
		)?;
		std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755))?;
		let output = Command::new(env::current_exe()?)
			.args([
				"--exact",
				"context::tests::selected_rustc_must_supply_complete_version_metadata",
				"--nocapture",
			])
			.env(CHILD, fixture.path())
			.env("RUSTC", &compiler)
			.output()?;
		expect_true!(
			output.status.success(),
			"{} {}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		Ok(())
	}
}
