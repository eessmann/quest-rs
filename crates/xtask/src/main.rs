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
        CommandKind::CheckNativeConsumers { work_dir } => native_consumers::run(work_dir),
        CommandKind::GenerateQsvtCatalog { source, check } => qsvt_catalog::run(&source, check),
        CommandKind::Help => {
            eprintln!("{USAGE}");
            Ok(())
        }
    }
}

const USAGE: &str = "usage: cargo run -p xtask -- generate-quest-bindings [--check]\n       cargo run -p xtask -- check-native-consumers [--work-dir PATH]\n       cargo run -p xtask -- generate-qsvt-catalog --source CPP_REPOSITORY [--check]";

#[derive(Debug, Eq, PartialEq)]
enum CommandKind {
    GenerateQuestBindings { check: bool },
    CheckNativeConsumers { work_dir: Option<PathBuf> },
    GenerateQsvtCatalog { source: PathBuf, check: bool },
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
        Some("check-native-consumers") => match arguments.next().as_deref() {
            None => Ok(CommandKind::CheckNativeConsumers { work_dir: None }),
            Some(argument) if argument == "--work-dir" => {
                let work_dir = arguments
                    .next()
                    .ok_or_else(|| format!("--work-dir requires a path\n{USAGE}"))?;
                if arguments.next().is_some() {
                    return Err(format!("unexpected trailing arguments\n{USAGE}").into());
                }
                Ok(CommandKind::CheckNativeConsumers {
                    work_dir: Some(PathBuf::from(work_dir)),
                })
            }
            Some(argument) => Err(format!(
                "unexpected check-native-consumers argument: {}\n{USAGE}",
                argument.to_string_lossy()
            )
            .into()),
        },
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
}
