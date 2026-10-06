//! Cargo-owned temporary work directories for independent tooling invocations.
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::generate::DynError;

pub fn native_work_directory(
	workspace: &Path,
	prefix: &str,
) -> Result<tempfile::TempDir, DynError> {
	let mut cargo = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
	cargo
		.args([
			"metadata",
			"--format-version=1",
			"--no-deps",
			"--offline",
			"--manifest-path",
		])
		.arg(workspace.join("Cargo.toml"));
	let target = target_directory(&mut cargo)?;
	std::fs::create_dir_all(&target)?;
	Ok(tempfile::Builder::new().prefix(prefix).tempdir_in(target)?)
}

#[derive(serde::Deserialize)]
struct Metadata {
	target_directory: PathBuf,
}

fn target_directory(cargo: &mut Command) -> Result<PathBuf, DynError> {
	let output = cargo.output()?;
	if !output.status.success() {
		return Err(format!(
			"could not resolve Cargo target directory: {}",
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}
	let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
	Ok(metadata.target_directory)
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;

	#[gtest]
	fn cargo_target_configuration_controls_tooling_directory() -> googletest::Result<()> {
		let temporary = tempfile::tempdir().or_fail()?;
		let workspace = temporary.path().join("fixture");
		std::fs::create_dir_all(workspace.join(".cargo")).or_fail()?;
		std::fs::write(workspace.join("Cargo.toml"), "[package]\nname = 'tooling-fixture'\nversion = '0.1.0'\n[[bin]]\nname = 'fixture'\npath = 'main.rs'\n[workspace]\n").or_fail()?;
		std::fs::write(workspace.join("main.rs"), "fn main() {}\n").or_fail()?;
		std::fs::write(
			workspace.join(".cargo/config.toml"),
			"[build]\ntarget-dir = 'configured target'\n",
		)
		.or_fail()?;
		let mut cargo = Command::new("cargo");
		cargo
			.current_dir(&workspace)
			.args(["metadata", "--format-version=1", "--no-deps", "--offline"])
			.env_remove("CARGO_TARGET_DIR")
			.env_remove("CARGO_BUILD_TARGET_DIR");
		expect_that!(
			target_directory(&mut cargo).or_fail()?,
			// Cargo resolves a relative configuration against its canonical cwd.
			eq(&workspace
				.canonicalize()
				.or_fail()?
				.join("configured target"))
		);
		cargo.env("CARGO_TARGET_DIR", temporary.path().join("external target"));
		verify_that!(
			target_directory(&mut cargo).or_fail()?,
			eq(&temporary.path().join("external target"))
		)
	}
}
