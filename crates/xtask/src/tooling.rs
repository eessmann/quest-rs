//! Cargo-owned temporary work directories for independent tooling invocations.
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;

use crate::generate::DynError;

pub fn native_work_directory(
	workspace: &Path,
	prefix: &str,
) -> Result<tempfile::TempDir, DynError> {
	let cargo = metadata_command(workspace);
	let target = target_directory(&cargo)?;
	std::fs::create_dir_all(&target)?;
	Ok(tempfile::Builder::new().prefix(prefix).tempdir_in(target)?)
}

fn metadata_command(workspace: &Path) -> cargo_metadata::MetadataCommand {
	let mut cargo = cargo_metadata::MetadataCommand::new();
	cargo
		.cargo_path(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
		.current_dir(workspace)
		.manifest_path(workspace.join("Cargo.toml"))
		.no_deps()
		.other_options(vec!["--offline".into()]);
	cargo
}

fn target_directory(cargo: &cargo_metadata::MetadataCommand) -> Result<PathBuf, DynError> {
	Ok(cargo.exec()?.target_directory.into_std_path_buf())
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;

	#[gtest]
	fn work_directory_reads_the_selected_workspace_cargo_configuration() -> googletest::Result<()> {
		const CHILD: &str = "QUEST_TOOLING_CONFIG_CHILD";
		if let Some(root) = std::env::var_os(CHILD) {
			let workspace = PathBuf::from(root);
			let work = native_work_directory(&workspace, "probe-").or_fail()?;
			expect_eq!(
				work.path().parent().or_fail()?,
				workspace.canonicalize()?.join("configured target")
			);
			return Ok(());
		}
		let temporary = tempfile::tempdir()?;
		let workspace = temporary.path().join("fixture with spaces");
		std::fs::create_dir_all(workspace.join(".cargo"))?;
		std::fs::write(
			workspace.join("Cargo.toml"),
			"[package]\nname = 'tooling-fixture'\nversion = '0.1.0'\n[[bin]]\nname = 'fixture'\npath = 'main.rs'\n[workspace]\n",
		)?;
		std::fs::write(workspace.join("main.rs"), "fn main() {}\n")?;
		std::fs::write(
			workspace.join(".cargo/config.toml"),
			"[build]\ntarget-dir = 'configured target'\n",
		)?;
		let output = Command::new(std::env::current_exe()?)
			.args([
				"--exact",
				"tooling::tests::work_directory_reads_the_selected_workspace_cargo_configuration",
				"--nocapture",
			])
			.env(CHILD, &workspace)
			.env_remove("CARGO_TARGET_DIR")
			.env_remove("CARGO_BUILD_TARGET_DIR")
			.output()?;
		expect_true!(
			output.status.success(),
			"{} {}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		Ok(())
	}

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
		let mut cargo = metadata_command(&workspace);
		cargo
			.env_remove("CARGO_TARGET_DIR")
			.env_remove("CARGO_BUILD_TARGET_DIR");
		expect_that!(
			target_directory(&cargo).or_fail()?,
			// Cargo resolves a relative configuration against its canonical cwd.
			eq(&workspace
				.canonicalize()
				.or_fail()?
				.join("configured target"))
		);
		cargo.env("CARGO_TARGET_DIR", temporary.path().join("external target"));
		verify_that!(
			target_directory(&cargo).or_fail()?,
			eq(&temporary.path().join("external target"))
		)
	}
}
