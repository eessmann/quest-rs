use clap::{CommandFactory, Parser};
#[cfg(test)]
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
	match command_kind(Cli::parse())? {
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
			Cli::command().print_help()?;
			Ok(())
		}
	}
}

#[derive(clap::Parser)]
#[command(
	name = "xtask",
	about = "Maintain QuEST bindings, native consumers, and the bundled QSVT catalog"
)]
struct Cli {
	#[command(subcommand)]
	command: Option<CliCommand>,
}

#[derive(clap::Subcommand)]
enum CliCommand {
	GenerateQuestBindings {
		#[arg(long)]
		check: bool,
	},
	CheckNativeConsumers {
		#[arg(long)]
		work_dir: Option<PathBuf>,
		#[arg(long, default_value = "cpu")]
		backends: String,
		#[arg(long)]
		loader_isolated: bool,
	},
	NativeDoctor {
		#[arg(long)]
		json: bool,
	},
	#[command(name = "__native-doctor-probe", hide = true)]
	NativeDoctorProbe {
		kind: String,
	},
	FetchQsvtCatalog,
	CheckQsvtCatalog,
}

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

fn command_kind(cli: Cli) -> Result<CommandKind, generate::DynError> {
	Ok(match cli.command {
		None => CommandKind::Help,
		Some(CliCommand::GenerateQuestBindings { check }) => {
			CommandKind::GenerateQuestBindings { check }
		}
		Some(CliCommand::CheckNativeConsumers {
			work_dir,
			backends,
			loader_isolated,
		}) => CommandKind::CheckNativeConsumers {
			work_dir,
			backends: native_consumers::parse_backends(&backends)?,
			loader_isolated,
		},
		Some(CliCommand::NativeDoctor { json }) => CommandKind::NativeDoctor { json },
		Some(CliCommand::NativeDoctorProbe { kind }) => CommandKind::NativeDoctorProbe { kind },
		Some(CliCommand::FetchQsvtCatalog) => CommandKind::QsvtCatalog { fetch: true },
		Some(CliCommand::CheckQsvtCatalog) => CommandKind::QsvtCatalog { fetch: false },
	})
}

#[cfg(test)]
fn parse_command(
	arguments: impl IntoIterator<Item = OsString>,
) -> Result<CommandKind, generate::DynError> {
	command_kind(Cli::try_parse_from(
		std::iter::once(OsString::from("xtask")).chain(arguments),
	)?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	#[cfg(test)]
	use std::ffi::OsString;
	use std::path::PathBuf;

	#[gtest]
	fn consumer_options_accept_equals_syntax_without_splitting_paths() -> googletest::Result<()> {
		let parsed = parse_command(
			[
				"check-native-consumers",
				"--work-dir=/tmp/fixture with spaces",
				"--backends=cpu,omp",
			]
			.map(OsString::from),
		)
		.or_fail()?;
		expect_eq!(
			parsed,
			CommandKind::CheckNativeConsumers {
				work_dir: Some(PathBuf::from("/tmp/fixture with spaces")),
				backends: vec![
					native_consumers::Backend::Cpu,
					native_consumers::Backend::Omp
				],
				loader_isolated: false
			}
		);
		Ok(())
	}

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
