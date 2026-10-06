use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

use color_eyre::config::HookBuilder;
use color_eyre::eyre::{Result, eyre};

mod generate;
mod native_consumers;
mod native_doctor;
mod qsvt_catalog;
mod tooling;

fn main() -> Result<()> {
	HookBuilder::default().install()?;
	run().map_err(|error| eyre!(error))
}

fn run() -> Result<(), generate::DynError> {
	match parse_command(env::args_os().skip(1))? {
		CommandKind::GenerateQuestBindings { check } => generate::run(check),
		CommandKind::CheckNativeConsumers {
			work_dir,
			backends,
			loader_isolated,
		} => native_consumers::run(work_dir, &backends, loader_isolated),
		CommandKind::NativeDoctor { json } => native_doctor::run(json),
		CommandKind::NativeDoctorProbe { kind } => native_doctor::probe_worker(&kind),
		CommandKind::QsvtCatalog { fetch } => qsvt_catalog::run(fetch),
		CommandKind::Help => {
			eprintln!("{USAGE}");
			Ok(())
		}
	}
}

const USAGE: &str = "usage: cargo run -p xtask -- generate-quest-bindings [--check]\n       cargo run -p xtask -- check-native-consumers [--work-dir PATH] [--backends cpu,omp,gpu] [--loader-isolated]\n       cargo run -p xtask -- native-doctor [--json]\n       cargo run -p xtask -- fetch-qsvt-catalog\n       cargo run -p xtask -- check-qsvt-catalog";

#[derive(Debug, Eq, PartialEq)]
enum CommandKind {
	GenerateQuestBindings {
		check: bool,
	},
	CheckNativeConsumers {
		work_dir: Option<PathBuf>,
		backends: Vec<native_consumers::Backend>,
		loader_isolated: bool,
	},
	NativeDoctor {
		json: bool,
	},
	NativeDoctorProbe {
		kind: String,
	},
	QsvtCatalog {
		fetch: bool,
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
		Some(command @ ("fetch-qsvt-catalog" | "check-qsvt-catalog")) => {
			if arguments.next().is_some() {
				return Err("catalog maintenance commands take no arguments".into());
			}
			Ok(CommandKind::QsvtCatalog {
				fetch: command == "fetch-qsvt-catalog",
			})
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
		Some("__native-doctor-probe") => {
			let kind = arguments
				.next()
				.ok_or("missing doctor probe name")?
				.into_string()
				.map_err(|_| "doctor probe name is not UTF-8")?;
			if arguments.next().is_some() {
				return Err("unexpected doctor probe argument".into());
			}
			Ok(CommandKind::NativeDoctorProbe { kind })
		}
		Some("native-doctor") => match arguments.next().as_deref() {
			None => Ok(CommandKind::NativeDoctor { json: false }),
			Some(argument) if argument == "--json" && arguments.next().is_none() => {
				Ok(CommandKind::NativeDoctor { json: true })
			}
			_ => Err(format!("unexpected native-doctor argument\n{USAGE}").into()),
		},
		Some("check-native-consumers") => {
			let mut work_dir = None;
			let mut backends = None;
			let mut loader_isolated = false;
			while let Some(argument) = arguments.next() {
				if argument == "--loader-isolated" {
					if loader_isolated {
						return Err("duplicate --loader-isolated option".into());
					}
					loader_isolated = true;
				} else if argument == "--work-dir" {
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
				loader_isolated,
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
	fn native_doctor_accepts_json_and_consumers_accept_loader_isolation() {
		expect_true!(parse_command(["native-doctor", "--json"].map(OsString::from)).is_ok());
		expect_true!(
			parse_command(["check-native-consumers", "--loader-isolated"].map(OsString::from))
				.is_ok()
		);
		expect_true!(
			parse_command(["native-doctor", "--json", "--json"].map(OsString::from)).is_err()
		);
	}

	#[gtest]
	fn catalog_commands_are_explicit_and_reject_the_old_cpp_source() {
		for command in ["fetch-qsvt-catalog", "check-qsvt-catalog"] {
			expect_true!(parse_command([OsString::from(command)]).is_ok());
			expect_true!(parse_command([command, "--source", "cpp"].map(OsString::from)).is_err());
		}
		expect_true!(
			parse_command(["generate-qsvt-catalog", "--source", "cpp"].map(OsString::from))
				.is_err()
		);
	}

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
				loader_isolated: false,
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
					loader_isolated: false,
				})
			);
		}
		verify_that!(
			&parse_command([OsString::from("check-native-consumers")]).or_fail()?,
			eq(&CommandKind::CheckNativeConsumers {
				work_dir: None,
				backends: vec![native_consumers::Backend::Cpu],
				loader_isolated: false,
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
