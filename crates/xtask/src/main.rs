use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use color_eyre::config::HookBuilder;
use color_eyre::eyre::{Result, eyre};

mod generate;
mod native_consumers;
mod qsvt_catalog;

fn main() -> Result<()> {
	HookBuilder::default().install()?;
	run().map_err(|error| eyre!(error))
}

fn run() -> Result<(), generate::DynError> {
	match parse_command(env::args_os().skip(1))? {
		CommandKind::GenerateQuestBindings { check } => generate::run(check),
		CommandKind::CheckNativeConsumers { work_dir, backends } => {
			native_consumers::run(work_dir, &backends)
		}
		CommandKind::GenerateQsvtCatalog { source, check } => qsvt_catalog::run(&source, check),
		CommandKind::Help => {
			eprintln!("{USAGE}");
			Ok(())
		}
	}
}

const USAGE: &str = "usage: cargo run -p xtask -- generate-quest-bindings [--check]\n       cargo run -p xtask -- check-native-consumers [--work-dir PATH] [--backends cpu,omp,gpu]\n       cargo run -p xtask -- generate-qsvt-catalog --source CPP_REPOSITORY [--check]";

#[derive(Debug, Eq, PartialEq)]
enum CommandKind {
	GenerateQuestBindings {
		check: bool,
	},
	CheckNativeConsumers {
		work_dir: Option<PathBuf>,
		backends: Vec<native_consumers::Backend>,
	},
	GenerateQsvtCatalog {
		source: PathBuf,
		check: bool,
	},
	Help,
}

fn parse_command(
	arguments: impl IntoIterator<Item = OsString>,
) -> Result<CommandKind, generate::DynError> {
	let mut arguments = arguments.into_iter();
	let Some(command) = arguments.next() else {
		return Ok(CommandKind::Help);
	};
	match command.to_str() {
		Some("generate-qsvt-catalog") => {
			if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--source")) {
				return Err(format!(
					"catalog generation requires --source CPP_REPOSITORY\n{USAGE}"
				)
				.into());
			}
			let source = arguments
				.next()
				.map(PathBuf::from)
				.ok_or("missing catalog source path")?;
			let check = match arguments.next().as_deref() {
				None => false,
				Some(flag) if flag == "--check" => true,
				Some(_) => return Err("unexpected catalog generation argument".into()),
			};
			if arguments.next().is_some() {
				return Err("unexpected trailing catalog arguments".into());
			}
			Ok(CommandKind::GenerateQsvtCatalog { source, check })
		}
		Some("generate-quest-bindings") => match arguments.next().as_deref() {
			None => Ok(CommandKind::GenerateQuestBindings { check: false }),
			Some(argument) if argument == "--check" && arguments.next().is_none() => {
				Ok(CommandKind::GenerateQuestBindings { check: true })
			}
			Some(argument) => Err(format!(
				"unexpected generate-quest-bindings argument: {}\n{USAGE}",
				argument.to_string_lossy()
			)
			.into()),
		},
		Some("check-native-consumers") => {
			let mut work_dir = None;
			let mut backends = None;
			while let Some(argument) = arguments.next() {
				if argument == "--work-dir" {
					if work_dir.is_some() {
						return Err("duplicate --work-dir option".into());
					}
					work_dir = Some(PathBuf::from(
						arguments.next().ok_or("--work-dir requires a path")?,
					));
				} else if argument == "--backends" {
					if backends.is_some() {
						return Err("duplicate --backends option".into());
					}
					let value = arguments.next().ok_or("--backends requires a list")?;
					let value = value.to_str().ok_or("backend list is not valid UTF-8")?;
					backends = Some(native_consumers::parse_backends(value)?);
				} else {
					return Err(format!(
						"unexpected check-native-consumers argument: {}\n{USAGE}",
						argument.to_string_lossy()
					)
					.into());
				}
			}
			Ok(CommandKind::CheckNativeConsumers {
				work_dir,
				backends: backends.unwrap_or_else(|| vec![native_consumers::Backend::Cpu]),
			})
		}
		Some(command) => Err(format!("unknown xtask command: {command}\n{USAGE}").into()),
		None => Err(format!("xtask command is not valid UTF-8\n{USAGE}").into()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	use std::ffi::OsString;
	use std::path::PathBuf;

	#[gtest]
	fn check_native_consumers_accepts_an_optional_work_directory() -> googletest::Result<()> {
		let command = parse_command(
			[
				"check-native-consumers",
				"--work-dir",
				"/tmp/fixture with spaces",
			]
			.into_iter()
			.map(OsString::from),
		)
		.or_fail()?;

		verify_that!(
			&command,
			eq(&CommandKind::CheckNativeConsumers {
				work_dir: Some(PathBuf::from("/tmp/fixture with spaces")),
				backends: vec![native_consumers::Backend::Cpu],
			})
		)
	}

	#[gtest]
	fn removed_and_trailing_arguments_are_rejected() -> googletest::Result<()> {
		let configure = parse_command([OsString::from("configure-native")]);
		let trailing = parse_command(
			["check-native-consumers", "--unexpected"]
				.into_iter()
				.map(OsString::from),
		);

		expect_that!(configure, err(anything()));
		verify_that!(trailing, err(anything()))
	}

	#[gtest]
	fn native_consumers_accept_backend_list_in_either_option_order() -> googletest::Result<()> {
		for arguments in [
			[
				"check-native-consumers",
				"--backends",
				"cpu,omp,gpu",
				"--work-dir",
				"/tmp/cases",
			],
			[
				"check-native-consumers",
				"--work-dir",
				"/tmp/cases",
				"--backends",
				"cpu,omp,gpu",
			],
		] {
			let parsed = parse_command(arguments.into_iter().map(OsString::from)).or_fail()?;
			expect_that!(
				&parsed,
				eq(&CommandKind::CheckNativeConsumers {
					work_dir: Some(PathBuf::from("/tmp/cases")),
					backends: vec![
						native_consumers::Backend::Cpu,
						native_consumers::Backend::Omp,
						native_consumers::Backend::Gpu
					],
				})
			);
		}
		verify_that!(
			&parse_command([OsString::from("check-native-consumers")]).or_fail()?,
			eq(&CommandKind::CheckNativeConsumers {
				work_dir: None,
				backends: vec![native_consumers::Backend::Cpu],
			})
		)
	}

	#[gtest]
	fn native_consumers_reject_invalid_backend_lists() {
		for value in ["", "cpu,", ",gpu", "cpu,,gpu", "cpu,cpu", "other"] {
			let parsed = parse_command(
				["check-native-consumers", "--backends", value]
					.into_iter()
					.map(OsString::from),
			);
			expect_that!(parsed, err(anything()));
		}
		for arguments in [
			[
				"check-native-consumers",
				"--backends",
				"cpu",
				"--backends",
				"gpu",
			],
			[
				"check-native-consumers",
				"--work-dir",
				"first",
				"--work-dir",
				"second",
			],
		] {
			expect_that!(
				parse_command(arguments.into_iter().map(OsString::from)),
				err(anything())
			);
		}
	}
}
