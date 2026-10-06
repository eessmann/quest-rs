use crate::probe::{cmake_path, push_unique, validate_sdk};
use crate::{Result, invalid, io, runtime_link_args};
use std::{
	collections::{BTreeMap, BTreeSet},
	fs,
	io::Read as _,
	path::{Path, PathBuf},
};

#[derive(Default)]
pub struct NativeLink {
	pub(crate) target_os: Option<&'static str>,
	pub(crate) architecture: Option<&'static str>,
	pub(crate) sysroot: Option<PathBuf>,
	pub(crate) framework_search_dirs: Vec<PathBuf>,
	pub(crate) library: PathBuf,
	pub(crate) implicit_search_dirs: Vec<PathBuf>,
	pub(crate) search_dirs: Vec<PathBuf>,
	pub(crate) libraries: Vec<String>,
	pub(crate) options: Vec<String>,
	pub(crate) runtime_dirs: Vec<PathBuf>,
	pub(crate) linked_files: BTreeSet<PathBuf>,
	pub(crate) library_files_by_name: BTreeMap<String, PathBuf>,
}
pub fn record_link_tokens(tokens: &[String], link: &mut NativeLink) -> Result<()> {
	let tokens = expand_linker_tokens(tokens)?;
	let directories = collect_search_directories(&tokens, link)?;
	let mut whole_archive = false;
	let mut iter = tokens.iter();
	while let Some(token) = iter.next() {
		if matches!(token.as_str(), "--whole-archive" | "--no-whole-archive") {
			if link.target_os == Some("macos") {
				return Err(invalid(
					"ELF whole-archive scopes are unsupported on Darwin",
				));
			}
			let start = token == "--whole-archive";
			if whole_archive == start {
				return Err(invalid("unbalanced or nested whole-archive scope"));
			}
			whole_archive = start;
		} else if token == "-l" || token.starts_with("-l") {
			let name = if token == "-l" {
				iter.next()
					.ok_or_else(|| invalid("missing library after -l"))?
					.as_str()
			} else {
				token.strip_prefix("-l").unwrap_or_default()
			};
			record_named_library(name, whole_archive, &directories, link)?;
		} else if Path::new(token).is_absolute() {
			record_linked_file_scoped(token, whole_archive, link)?;
		} else if matches!(token.as_str(), "-Wl,-rpath" | "-Wl,-rpath-link") {
			let value = iter
				.next()
				.map(|value| value.strip_prefix("-Wl,").unwrap_or(value))
				.ok_or_else(|| invalid(format!("missing paired path after {token}")))?;
			let option = format!("{token},{value}");
			validate_link_option(&option, link.target_os.unwrap_or("linux"))?;
			link.options.push(option);
		} else if token == "-framework" && link.target_os == Some("macos") {
			let name = iter
				.next()
				.ok_or_else(|| invalid("missing framework name"))?;
			if name.is_empty()
				|| !name
					.chars()
					.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
			{
				return Err(invalid("unsupported framework name"));
			}
			link.libraries.push(format!("framework={name}"));
		} else if token == "-isysroot" && link.target_os == Some("macos") {
			let root = iter
				.next()
				.ok_or_else(|| invalid("missing Darwin sysroot"))?;
			let sdk = validate_sdk(Path::new(root))?;
			if link
				.sysroot
				.as_ref()
				.is_some_and(|expected| expected != &sdk)
			{
				return Err(invalid(
					"Darwin link sysroot differs from the evaluated SDK",
				));
			}
			runtime_link_args("macos", std::slice::from_ref(&sdk))?;
			link.options
				.push(format!("-Wl,-syslibroot,{}", sdk.display()));
		} else if token == "-arch" && link.target_os == Some("macos") {
			let arch = iter
				.next()
				.ok_or_else(|| invalid("missing Darwin architecture"))?;
			if !matches!(arch.as_str(), "arm64" | "x86_64")
				|| link.architecture.is_some_and(|expected| expected != arch)
			{
				return Err(invalid(
					"Darwin link architecture differs from the native Cargo target",
				));
			}
			// Rust already selects the validated native architecture.
		} else if token == "-L"
			|| token == "-l"
			|| (token == "-F" && link.target_os == Some("macos"))
		{
			let value = iter
				.next()
				.ok_or_else(|| invalid(format!("missing argument after {token}")))?;
			record_link_token(&format!("{token}{value}"), link)?;
		} else {
			record_link_token(token, link)?;
		}
	}
	if whole_archive {
		return Err(invalid("unclosed whole-archive scope"));
	}
	for path in &link.implicit_search_dirs {
		push_search_directory(&mut link.search_dirs, path.clone());
	}
	Ok(())
}

fn collect_search_directories(tokens: &[String], link: &mut NativeLink) -> Result<Vec<PathBuf>> {
	// Linker -L arguments affect every -l, even when written later. Keep their
	// original directory order and search each directory shared-before-static.
	let mut directories = Vec::new();
	let mut search = tokens.iter();
	while let Some(token) = search.next() {
		let directory = if token == "-L" {
			search.next().map(String::as_str)
		} else {
			token.strip_prefix("-L")
		};
		if let Some(directory) = directory {
			let path = PathBuf::from(directory);
			runtime_link_args(
				link.target_os.unwrap_or("linux"),
				std::slice::from_ref(&path),
			)?;
			push_search_directory(&mut directories, path.clone());
			push_search_directory(&mut link.search_dirs, path);
		}
	}
	for path in &link.implicit_search_dirs {
		runtime_link_args(
			link.target_os.unwrap_or("linux"),
			std::slice::from_ref(path),
		)?;
		push_search_directory(&mut directories, path.clone());
		push_search_directory(&mut link.search_dirs, path.clone());
	}
	Ok(directories)
}

fn push_search_directory(directories: &mut Vec<PathBuf>, path: PathBuf) {
	// Preserve the first search position and spelling, including mounted/symlink
	// aliases. Resolving a library must not append that same directory again.
	if directories.contains(&path) {
		return;
	}
	if let Ok(canonical) = fs::canonicalize(&path)
		&& directories
			.iter()
			.any(|earlier| fs::canonicalize(earlier).is_ok_and(|value| value == canonical))
	{
		return;
	}
	// Missing search directories remain representable and distinct; they may
	// be populated later, and two failed canonicalizations do not mean equality.
	directories.push(path);
}

/// Normalize driver forwarding while retaining library and scope order.
fn expand_linker_tokens(tokens: &[String]) -> Result<Vec<String>> {
	let mut expanded = Vec::new();
	let mut input = tokens.iter();
	while let Some(token) = input.next() {
		let forwarded;
		let token = if token == "-Xlinker" {
			forwarded = format!(
				"-Wl,{}",
				input
					.next()
					.ok_or_else(|| invalid("missing argument after -Xlinker"))?
			);
			&forwarded
		} else {
			token
		};
		if let Some(fragment) = token.strip_prefix("-Wl,") {
			let mut parts = fragment.split(',').peekable();
			while let Some(part) = parts.next() {
				if matches!(part, "--whole-archive" | "--no-whole-archive")
					|| part.starts_with("-l")
					|| part.starts_with("-L")
					|| !part.starts_with('-')
				{
					expanded.push(part.into());
				} else if matches!(part, "-rpath" | "-rpath-link") && parts.peek().is_some() {
					expanded.push(format!("-Wl,{part},{}", parts.next().unwrap_or_default()));
				} else {
					expanded.push(format!("-Wl,{part}"));
				}
			}
		} else {
			expanded.push(token.clone());
		}
	}
	Ok(expanded)
}

fn record_named_library(
	name: &str,
	whole_archive: bool,
	directories: &[PathBuf],
	link: &mut NativeLink,
) -> Result<()> {
	if name.trim_start_matches(':').is_empty()
		|| name.strip_prefix(':').unwrap_or(name).contains(':')
		|| name.is_empty()
		|| name.contains(['=', ',', '/', ' ', '\n', '\r', '\0'])
		|| name.starts_with('-')
	{
		return Err(invalid("invalid native library name"));
	}
	if name.starts_with(':') && link.target_os == Some("macos") {
		return Err(invalid(
			"Darwin does not support GNU -l:filename library syntax",
		));
	}
	let candidates = if let Some(exact) = name.strip_prefix(':') {
		vec![exact.to_owned()]
	} else if link.target_os == Some("macos") {
		vec![
			format!("lib{name}.tbd"),
			format!("lib{name}.dylib"),
			format!("lib{name}.a"),
		]
	} else {
		vec![format!("lib{name}.so"), format!("lib{name}.a")]
	};
	let selected = directories
		.iter()
		.flat_map(|dir| candidates.iter().map(move |name| dir.join(name)))
		.find(|path| path.is_file());
	if let Some(path) = selected {
		// A Darwin system text stub is handled by the compiler's native -l search.
		if path.extension().is_some_and(|ext| ext == "tbd") && !whole_archive {
			link.libraries.push(name.into());
			return Ok(());
		}
		record_linked_file_scoped(cmake_path(&path)?, whole_archive, link)
	} else if whole_archive {
		Err(invalid(format!(
			"cannot resolve -l{name} inside whole-archive using CMake's explicit and implicit search directories"
		)))
	} else {
		record_link_token(&format!("-l{name}"), link)
	}
}

fn record_link_token(token: &str, link: &mut NativeLink) -> Result<()> {
	if token.is_empty()
		|| token.starts_with("-O")
		|| token == "-g"
		|| token.starts_with("-D")
		|| token.starts_with("-U")
		|| token.starts_with("-std=")
		|| matches!(
			token,
			"-fPIC"
				| "-fpic"
				| "-fPIE"
				| "-fpie"
				| "-fno-exceptions"
				| "-fexceptions"
				| "-fno-rtti"
				| "-frtti"
				| "-ffunction-sections"
				| "-fdata-sections"
		)
		|| (token.starts_with("-W") && !token.starts_with("-Wl,") && !token.starts_with("-Wa,"))
		|| token.starts_with("-march=")
		|| token.starts_with("-mtune=")
		|| token.starts_with("-mcpu=")
		|| token == "-w"
		|| matches!(token, "-m32" | "-m64")
	{
		return Ok(());
	}
	if token.contains(['\n', '\r', '\0']) || token.contains("$<") {
		return Err(invalid(format!(
			"unevaluated or invalid CMake link token: {token}"
		)));
	}
	if let Some(directory) = token
		.strip_prefix("-F")
		.filter(|_| link.target_os == Some("macos"))
	{
		let directory = PathBuf::from(directory);
		runtime_link_args("macos", std::slice::from_ref(&directory))?;
		push_unique(&mut link.framework_search_dirs, directory);
	} else if let Some(directory) = token.strip_prefix("-L") {
		let directory = PathBuf::from(directory);
		runtime_link_args(
			link.target_os.unwrap_or("linux"),
			std::slice::from_ref(&directory),
		)?;
		push_search_directory(&mut link.search_dirs, directory);
	} else if let Some(name) = token.strip_prefix("-l") {
		if name.is_empty() || name.contains(['=', ',', '/', ' ']) || name.starts_with('-') {
			return Err(invalid("invalid native library name"));
		}
		if link.target_os == Some("macos") && name.starts_with(':') {
			return Err(invalid(
				"Darwin does not support GNU -l:filename library syntax",
			));
		}
		link.libraries.push(name.strip_prefix(':').map_or_else(
			|| name.to_owned(),
			|filename| format!("dylib:+verbatim={filename}"),
		));
	} else if Path::new(token).is_absolute() {
		record_linked_file(token, link)?;
	} else {
		validate_link_option(token, link.target_os.unwrap_or("linux"))?;
		link.options.push(token.to_owned());
	}
	Ok(())
}

pub fn record_linked_file(token: &str, link: &mut NativeLink) -> Result<()> {
	record_linked_file_scoped(token, false, link)
}

fn record_linked_file_scoped(
	token: &str,
	whole_archive: bool,
	link: &mut NativeLink,
) -> Result<()> {
	cmake_path(Path::new(token))?;
	let path = fs::canonicalize(token).map_err(|error| io(Path::new(token), error))?;
	let name = path
		.file_name()
		.and_then(|name| name.to_str())
		.ok_or_else(|| invalid("native library filename must be UTF-8"))?;
	if name.contains([':', '=', '\n', '\r', '\0']) {
		return Err(invalid(
			"native library filename cannot be represented safely in Cargo",
		));
	}
	if let Some(earlier) = link.library_files_by_name.get(name) {
		return Err(invalid(if earlier == &path {
			format!(
				"repeated exact native library {name}: Rust cannot preserve repeated libraries with linking modifiers"
			)
		} else {
			format!(
				"ambiguous native library basename {name}: Cargo's global link search paths cannot preserve distinct absolute libraries"
			)
		}));
	}
	let kind = if whole_archive {
		scoped_library_kind(&path)?
	} else {
		native_library_kind(&path, name)?
	};
	let parent = path
		.parent()
		.ok_or_else(|| invalid("native library lacks a parent"))?;
	cmake_path(parent)?;
	push_search_directory(&mut link.search_dirs, parent.to_owned());
	if kind == "dylib" {
		push_unique(&mut link.runtime_dirs, parent.to_owned());
	}
	if kind == "dylib" && link.target_os == Some("macos") {
		let stem = name
			.strip_prefix("lib")
			.and_then(|name| name.strip_suffix(".dylib"))
			.filter(|name| !name.is_empty())
			.ok_or_else(|| invalid("Darwin shared libraries must have a libNAME.dylib filename"))?;
		// ld64 uses -lNAME, and does not implement GNU -l:filename. Keep the
		// version in NAME and validate search resolution against the exact file.
		link.libraries.push(format!("dylib={stem}"));
	} else if kind == "static" && whole_archive {
		link.libraries
			.push(format!("static:-bundle,+whole-archive,+verbatim={name}"));
	} else {
		link.libraries.push(format!("{kind}:+verbatim={name}"));
	}
	link.library_files_by_name
		.insert(name.to_owned(), path.clone());
	link.linked_files.insert(PathBuf::from(token));
	link.linked_files.insert(path);
	Ok(())
}

pub fn split_flags(fragment: &str) -> Result<Vec<String>> {
	shlex::split(fragment)
		.ok_or_else(|| invalid(format!("cannot parse CMake command fragment: {fragment}")))
}

fn validate_link_option(option: &str, target_os: &str) -> Result<()> {
	if matches!(option, "-pthread" | "-fopenmp") || option.starts_with("-fopenmp=") {
		return Ok(());
	}
	if target_os == "linux" && option == "-Wl,--enable-new-dtags" {
		return Ok(());
	}
	if target_os == "macos"
		&& matches!(
			option,
			"-Wl,-search_paths_first" | "-Wl,-headerpad_max_install_names"
		) {
		// Global Mach-O options: path-first search matches exact-file validation;
		// header padding changes capacity, without changing library ordering.
		return Ok(());
	}
	if target_os == "macos"
		&& option
			.strip_prefix("-mmacosx-version-min=")
			.is_some_and(|value| {
				!value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
			}) {
		return Ok(());
	}
	for prefix in ["-Wl,-rpath,", "-Wl,-rpath-link,"] {
		if prefix == "-Wl,-rpath-link," && target_os != "linux" {
			continue;
		}
		if let Some(paths) = option.strip_prefix(prefix) {
			let directories = if target_os == "linux" {
				paths.split(':').map(PathBuf::from).collect()
			} else {
				vec![PathBuf::from(paths)]
			};
			runtime_link_args(target_os, &directories)?;
			return Ok(());
		}
	}
	Err(invalid(format!(
		"unsupported or order-sensitive CMake link option {option}; Cargo cannot preserve its placement among libraries"
	)))
}

fn scoped_library_kind(path: &Path) -> Result<&'static str> {
	// Identify the input kind without interpreting linker scripts or loading a
	// potentially large library. The native linker still validates its contents.
	// ELF's fixed header is at most 64 bytes; archives have an eight-byte magic.
	let mut header = Vec::with_capacity(64);
	fs::File::open(path)
		.and_then(|file| file.take(64).read_to_end(&mut header))
		.map_err(|error| io(path, error))?;
	if header.starts_with(b"!<arch>\n") || header.starts_with(b"!<thin>\n") {
		return Ok("static");
	}
	if header.starts_with(b"\x7fELF") && header.len() >= 18 {
		let expected_length = match header.get(4) {
			Some(1) => 52,
			Some(2) => 64,
			_ => 0,
		};
		let shared_object = match header.get(5) {
			Some(1) => header.get(16..18) == Some([3, 0].as_slice()),
			Some(2) => header.get(16..18) == Some([0, 3].as_slice()),
			_ => false,
		};
		if expected_length != 0
			&& header.len() >= expected_length
			&& header.get(6) == Some(&1)
			&& shared_object
		{
			return Ok("dylib");
		}
	}
	Err(invalid(format!(
		"cannot lower whole-archive input {}: only archive files and ELF shared objects are supported; linker scripts and unrecognized inputs require their original linker scope",
		path.display()
	)))
}

fn native_library_kind(path: &Path, name: &str) -> Result<&'static str> {
	let extension = path.extension().and_then(|extension| extension.to_str());
	if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("a")) {
		return Ok("static");
	}
	let versioned_shared = name
		.as_bytes()
		.windows(4)
		.any(|window| window.eq_ignore_ascii_case(b".so."));
	if extension
		.is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "so" | "dylib"))
		|| versioned_shared
	{
		return Ok("dylib");
	}
	Err(invalid(format!(
		"unrecognized native library: {}",
		path.display()
	)))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
	use super::*;
	use crate::{Result, package::validate_library_resolution, run};
	use googletest::prelude::*;
	use std::ffi::OsString;
	use std::process::Command;

	fn rustc_command_from_environment(
		target: &str,
		mut lookup: impl FnMut(&str) -> Option<OsString>,
	) -> Result<Command> {
		let mut command = Command::new(lookup("RUSTC").unwrap_or_else(|| "rustc".into()));
		let target_key = target.replace(['-', '.'], "_").to_ascii_uppercase();
		if let Some(linker) =
			lookup(&format!("CARGO_TARGET_{target_key}_LINKER")).or_else(|| lookup("RUSTC_LINKER"))
		{
			let mut argument = OsString::from("linker=");
			argument.push(linker);
			command.arg("-C").arg(argument);
		}
		// Cargo, not rustc, interprets these environment variables. Apply the
		// active source only; an explicitly empty encoded value clears fallbacks.
		if let Some(flags) = lookup("CARGO_ENCODED_RUSTFLAGS") {
			let flags = flags
				.into_string()
				.map_err(|_| invalid("non-UTF-8 encoded Rust flags"))?;
			if !flags.is_empty() {
				command.args(flags.split('\x1f'));
			}
		} else if let Some(flags) = lookup("RUSTFLAGS")
			.or_else(|| lookup(&format!("CARGO_TARGET_{target_key}_RUSTFLAGS")))
			.or_else(|| lookup("CARGO_BUILD_RUSTFLAGS"))
		{
			let flags = flags
				.into_string()
				.map_err(|_| invalid("non-UTF-8 Rust flags"))?;
			command.args(flags.split_whitespace());
		}
		Ok(command)
	}

	fn native_rustc_command() -> Result<Command> {
		let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
		let output = run(Command::new(rustc).arg("-vV"))?;
		let version = String::from_utf8_lossy(&output.stdout);
		let target = version
			.lines()
			.find_map(|line| line.strip_prefix("host: "))
			.ok_or_else(|| invalid("rustc did not report its native host"))?;
		rustc_command_from_environment(target, |name| std::env::var_os(name))
	}

	#[gtest]
	fn downstream_rustc_preserves_native_cargo_environment() -> googletest::Result<()> {
		use std::os::unix::fs::PermissionsExt;
		let directory = tempfile::tempdir()?;
		let root = directory.path();
		let rustc = root.join("rustc wrapper");
		let linker = root.join("linker wrapper");
		for (path, body) in [
			(
				&rustc,
				"#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$QUEST_FIXTURE_RUSTC_LOG\"\nexec \"$QUEST_FIXTURE_REAL_RUSTC\" \"$@\"\n",
			),
			(
				&linker,
				"#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$QUEST_FIXTURE_LINKER_LOG\"\nexec cc \"$@\"\n",
			),
		] {
			fs::write(path, body)?;
			fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
		}
		let source = root.join("main.rs");
		fs::write(
			&source,
			"#[cfg(not(all(fixture_flag, fixture_local, fixture_text = \"with spaces\")))] compile_error!(\"native flags were lost\"); fn main() {}",
		)?;
		let settings = std::collections::BTreeMap::from([
			("RUSTC", rustc.into_os_string()),
			(
				"CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
				linker.clone().into_os_string(),
			),
			("RUSTFLAGS", "--cfg=wrong_fallback".into()),
			(
				"CARGO_ENCODED_RUSTFLAGS",
				[
					"-C",
					"linker-features=-lld",
					"--cfg",
					"fixture_flag",
					"--cfg",
					"fixture_text=\"with spaces\"",
				]
				.join("\x1f")
				.into(),
			),
		]);
		let rustc_log = root.join("rustc.log");
		let linker_log = root.join("linker.log");
		let executable = root.join("consumer");
		let mut command = rustc_command_from_environment("x86_64-unknown-linux-gnu", |name| {
			settings.get(name).cloned()
		})?;
		command
			.env(
				"QUEST_FIXTURE_REAL_RUSTC",
				std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()),
			)
			.env("QUEST_FIXTURE_RUSTC_LOG", &rustc_log)
			.env("QUEST_FIXTURE_LINKER_LOG", &linker_log)
			.args(["--cfg", "fixture_local"])
			.arg(&source)
			.arg("-o")
			.arg(&executable);
		run(&mut command)?;
		run(&mut Command::new(executable))?;
		let actual = fs::read_to_string(rustc_log)?;
		expect_true!(actual.contains(&format!("linker={}\n", linker.display())));
		expect_true!(actual.contains("fixture_text=\"with spaces\"\n"));
		expect_false!(actual.contains("wrong_fallback"));
		expect_false!(fs::read_to_string(linker_log)?.is_empty());
		Ok(())
	}

	#[gtest]
	fn downstream_rustc_flags_follow_cargo_precedence() -> googletest::Result<()> {
		for (encoded, plain, target_flags, build_flags, expected) in [
			(
				Some("--cfg\x1fencoded=\"with spaces\""),
				Some("--cfg plain"),
				Some("--cfg target"),
				Some("--cfg build"),
				vec!["--cfg", "encoded=\"with spaces\""],
			),
			(
				Some(""),
				Some("--cfg plain"),
				Some("--cfg target"),
				Some("--cfg build"),
				vec![],
			),
			(
				None,
				Some("--cfg\tplain  -C opt-level=1"),
				Some("--cfg target"),
				Some("--cfg build"),
				vec!["--cfg", "plain", "-C", "opt-level=1"],
			),
			(
				None,
				None,
				Some("--cfg target"),
				Some("--cfg build"),
				vec!["--cfg", "target"],
			),
			(
				None,
				None,
				None,
				Some("--cfg build"),
				vec!["--cfg", "build"],
			),
		] {
			let command =
				rustc_command_from_environment("x86_64-unknown-linux-gnu", |name| match name {
					"CARGO_ENCODED_RUSTFLAGS" => encoded.map(OsString::from),
					"RUSTFLAGS" => plain.map(OsString::from),
					"CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS" => {
						target_flags.map(OsString::from)
					}
					"CARGO_BUILD_RUSTFLAGS" => build_flags.map(OsString::from),
					_ => None,
				})?;
			let actual: Vec<_> = command.get_args().collect();
			let expected: Vec<_> = expected.iter().map(std::ffi::OsStr::new).collect();
			expect_eq!(actual, expected);
		}
		let mut command =
			rustc_command_from_environment("x86_64-unknown-linux-gnu", |name| match name {
				"RUSTC_LINKER" => Some("resolved linker".into()),
				"CARGO_ENCODED_RUSTFLAGS" => Some("-C\x1flinker=flags linker".into()),
				_ => None,
			})?;
		command.args(["-C", "linker=fixture linker"]);
		let actual: Vec<_> = command.get_args().collect();
		let expected: Vec<_> = [
			"-C",
			"linker=resolved linker",
			"-C",
			"linker=flags linker",
			"-C",
			"linker=fixture linker",
		]
		.iter()
		.map(std::ffi::OsStr::new)
		.collect();
		expect_eq!(actual, expected);
		Ok(())
	}

	fn object(root: &Path, name: &str, source: &str) -> Result<PathBuf> {
		let input = root.join(format!("{name}.c"));
		let output = root.join(format!("{name}.o"));
		fs::write(&input, source).map_err(|error| io(&input, error))?;
		run(Command::new("cc")
			.arg("-fPIC")
			.arg("-c")
			.arg(input)
			.arg("-o")
			.arg(&output))?;
		Ok(output)
	}
	fn archive(path: &Path, objects: &[PathBuf]) -> Result<()> {
		run(Command::new("ar").arg("rcs").arg(path).args(objects))?;
		Ok(())
	}
	fn downstream(root: &Path, link: &NativeLink, expected: i32) -> Result<()> {
		validate_library_resolution(&link.search_dirs, &link.library_files_by_name)?;
		let library_source = root.join("native_api.rs");
		fs::write(&library_source, "unsafe extern \"C\" { fn fixture_value() -> i32; } pub fn value() -> i32 { unsafe { fixture_value() } }").map_err(|error| io(&library_source, error))?;
		let rlib = root.join("libnative_api.rlib");
		let mut library = native_rustc_command()?;
		library
			.arg("--crate-type=rlib")
			.arg("--crate-name=native_api")
			.arg(&library_source)
			.arg("-o")
			.arg(&rlib);
		for path in &link.search_dirs {
			library.arg("-L").arg(format!("native={}", path.display()));
		}
		for native in &link.libraries {
			library.arg("-l").arg(native);
		}
		run(&mut library)?;
		let app_source = root.join("main.rs");
		fs::write(
			&app_source,
			format!("fn main() {{ assert_eq!(native_api::value(), {expected}); }}"),
		)
		.map_err(|error| io(&app_source, error))?;
		let app = root.join("consumer");
		let mut consumer = native_rustc_command()?;
		consumer
			.arg(&app_source)
			.arg("--extern")
			.arg(format!("native_api={}", rlib.display()))
			.arg("-o")
			.arg(&app);
		for path in &link.search_dirs {
			consumer.arg("-L").arg(format!("native={}", path.display()));
		}
		for option in runtime_link_args("linux", &link.runtime_dirs)? {
			consumer.arg("-C").arg(format!("link-arg={option}"));
		}
		run(&mut consumer)?;
		run(Command::new(&app)
			.env_remove("LD_LIBRARY_PATH")
			.env_remove("LD_PRELOAD")
			.env_remove("LD_AUDIT"))?;
		Ok(())
	}

	#[gtest]
	fn whole_archive_rejects_linker_scripts_without_losing_unreferenced_members()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let root = fixture.path();
		let entry = object(
			root,
			"script_entry",
			"int registered; int fixture_value(void) { return registered; }",
		)?;
		let registration = object(
			root,
			"script_registration",
			"extern int registered; __attribute__((constructor)) static void register_value(void) { registered = 73; }",
		)?;
		let members = root.join("libmembers.a");
		archive(&members, &[entry, registration])?;
		let script = root.join("libscript.so");
		fs::write(&script, format!("INPUT (\"{}\")\n", members.display()))?;
		let main = root.join("script_main.c");
		fs::write(
			&main,
			"#include <stdio.h>\nextern int fixture_value(void); int main(void) { printf(\"%d\\n\", fixture_value()); return 0; }\n",
		)?;
		for (whole, expected) in [(true, "73\n"), (false, "0\n")] {
			let app = root.join(if whole { "whole" } else { "ordinary" });
			let mut compile = Command::new("cc");
			compile.arg(&main).arg("-L").arg(root);
			if whole {
				compile.arg("-Wl,--whole-archive");
			}
			compile.arg("-lscript");
			if whole {
				compile.arg("-Wl,--no-whole-archive");
			}
			run(compile.arg("-o").arg(&app))?;
			expect_eq!(run(&mut Command::new(app))?.stdout, expected.as_bytes());
		}
		fs::copy(&script, root.join("libscript.a"))?;
		for name in ["script", ":libscript.a"] {
			let mut scoped = NativeLink {
				implicit_search_dirs: vec![root.to_owned()],
				..NativeLink::default()
			};
			let result = record_link_tokens(
				&[format!("-Wl,--whole-archive,-l{name},--no-whole-archive")],
				&mut scoped,
			);
			let error = result.expect_err("a linker script was admitted as a scoped library");
			expect_that!(error.to_string(), contains_substring("linker scripts"));
		}
		let mut ordinary = NativeLink {
			implicit_search_dirs: vec![root.to_owned()],
			..NativeLink::default()
		};
		record_link_tokens(&["-lscript".into()], &mut ordinary)?;
		downstream(root, &ordinary, 0)?;
		Ok(())
	}

	#[gtest]
	fn downstream_whole_archive_selects_shared_then_includes_all_real_archive_members()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let root = fixture.path();
		let shared_object = object(root, "shared", "int fixture_value(void) { return 11; }")?;
		let shared = root.join("libhugetlbfs.so");
		run(Command::new("cc")
			.arg("-shared")
			.arg(shared_object)
			.arg("-o")
			.arg(&shared))?;
		let entry = object(
			root,
			"entry",
			"int registered; int fixture_value(void) { return registered; }",
		)?;
		let registration = object(
			root,
			"registration",
			"extern int registered; __attribute__((constructor)) static void register_value(void) { registered = 73; }",
		)?;
		archive(&root.join("libhugetlbfs.a"), &[entry, registration])?;
		let mut link = NativeLink {
			implicit_search_dirs: vec![root.to_owned()],
			..NativeLink::default()
		};
		record_link_tokens(
			&["-Wl,--whole-archive,-lhugetlbfs,--no-whole-archive".into()],
			&mut link,
		)?;
		downstream(root, &link, 11)?;
		fs::remove_file(&shared)?;
		let mut link = NativeLink {
			implicit_search_dirs: vec![root.to_owned()],
			..NativeLink::default()
		};
		record_link_tokens(
			&[
				"-Xlinker".into(),
				"--whole-archive".into(),
				"-lhugetlbfs".into(),
				"-Xlinker".into(),
				"--no-whole-archive".into(),
			],
			&mut link,
		)?;
		downstream(root, &link, 73)?;
		Ok(())
	}

	#[gtest]
	fn aliased_search_directories_keep_first_spelling_and_library_precedence()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let first = fixture.path().join("first");
		let second = fixture.path().join("second");
		fs::create_dir(&first)?;
		fs::create_dir(&second)?;
		archive(&first.join("libsame.a"), &[])?;
		fs::write(second.join("libsame.so"), "shared")?;
		let alias = fixture.path().join("alias");
		std::os::unix::fs::symlink(fixture.path(), &alias)?;
		let first_alias = alias.join("first");
		let second_alias = alias.join("second");
		let mut link = NativeLink {
			implicit_search_dirs: vec![second_alias.clone(), first.canonicalize()?],
			..NativeLink::default()
		};
		record_link_tokens(
			&[
				format!("-L{}", first_alias.display()),
				"-Wl,--whole-archive,-lsame,--no-whole-archive".into(),
			],
			&mut link,
		)?;
		verify_that!(&link.search_dirs, eq(&vec![first_alias, second_alias]))?;
		verify_that!(
			&link.libraries,
			eq(&vec!["static:-bundle,+whole-archive,+verbatim=libsame.a"])
		)?;
		validate_library_resolution(&link.search_dirs, &link.library_files_by_name)?;
		Ok(())
	}

	#[gtest]
	fn whole_archive_resolves_per_directory_and_keeps_implicit_directory_order()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let first = fixture.path().join("first");
		let second = fixture.path().join("second");
		fs::create_dir(&first)?;
		fs::create_dir(&second)?;
		archive(&first.join("libsame.a"), &[])?;
		fs::write(second.join("libsame.so"), "shared")?;
		let mut link = NativeLink {
			implicit_search_dirs: vec![first.clone(), second.clone()],
			..NativeLink::default()
		};
		record_link_tokens(
			&["-Wl,--whole-archive,-lsame,--no-whole-archive".into()],
			&mut link,
		)?;
		expect_eq!(
			link.libraries,
			vec!["static:-bundle,+whole-archive,+verbatim=libsame.a"]
		);
		expect_eq!(link.search_dirs, vec![first, second]);
		Ok(())
	}

	#[gtest]
	fn bundled_exact_files_and_late_implicit_directory_keep_native_semantics()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let first = fixture.path().join("first");
		let second = fixture.path().join("second");
		fs::create_dir(&first)?;
		fs::create_dir(&second)?;
		let exact_archive = second.join("libexact.a");
		archive(&exact_archive, &[])?;
		let mut link = NativeLink {
			implicit_search_dirs: vec![first.clone(), second.clone()],
			..NativeLink::default()
		};
		record_link_tokens(
			&[format!(
				"-Wl,--whole-archive,{},--no-whole-archive",
				exact_archive.display()
			)],
			&mut link,
		)?;
		expect_eq!(
			link.libraries,
			vec!["static:-bundle,+whole-archive,+verbatim=libexact.a"]
		);
		expect_eq!(link.search_dirs, vec![first.clone(), second.clone()]);
		let mut link = NativeLink {
			implicit_search_dirs: vec![first.clone(), second.clone()],
			..NativeLink::default()
		};
		record_link_tokens(
			&["-Wl,--whole-archive,-l:libexact.a,--no-whole-archive".into()],
			&mut link,
		)?;
		expect_eq!(link.search_dirs, vec![first, second]);
		Ok(())
	}

	#[gtest]
	fn exact_library_names_cannot_inject_or_rename_cargo_requirements() -> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		for name in ["libbad:name.a", "libbad\nname.a"] {
			let path = fixture.path().join(name);
			fs::write(&path, "archive")?;
			expect_true!(
				record_link_tokens(&[path.display().to_string()], &mut NativeLink::default())
					.is_err()
			);
		}
		for name in ["-l:", "-l:libbad:name.a", "-lbad:name"] {
			expect_true!(record_link_tokens(&[name.into()], &mut NativeLink::default()).is_err());
		}
		Ok(())
	}

	#[gtest]
	fn whole_archive_scopes_reject_unbalanced_and_unresolved_members() -> googletest::Result<()> {
		for tokens in [
			"-Wl,--whole-archive",
			"-Wl,--no-whole-archive",
			"-Wl,--whole-archive,--whole-archive,--no-whole-archive",
			"-Wl,--whole-archive,-lmissing,--no-whole-archive",
			"-Wl,--whole-archive,-Bstatic,--no-whole-archive",
			"-Wl,--whole-archive,--start-group,--no-whole-archive",
		] {
			expect_true!(
				record_link_tokens(&split_flags(tokens)?, &mut NativeLink::default()).is_err(),
				"admitted {tokens}"
			);
		}
		Ok(())
	}

	#[gtest]
	fn downstream_archive_order_survives_rust_library_dependency() -> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let root = fixture.path();
		let entry = object(
			root,
			"entry",
			"extern int selected(void); int fixture_value(void) { return selected(); }",
		)?;
		let first = object(root, "first", "int selected(void) { return 91; }")?;
		let second = object(root, "second", "int selected(void) { return 17; }")?;
		let entry_archive = root.join("libentry.a");
		let first_archive = root.join("libfirst.a");
		let second_archive = root.join("libsecond.a");
		archive(&entry_archive, &[entry])?;
		archive(&first_archive, &[first])?;
		archive(&second_archive, &[second])?;
		for (first, second, expected) in [
			(&first_archive, &second_archive, 91),
			(&second_archive, &first_archive, 17),
		] {
			let mut ordered = NativeLink::default();
			record_link_tokens(
				&[
					entry_archive.display().to_string(),
					first.display().to_string(),
					second.display().to_string(),
				],
				&mut ordered,
			)?;
			downstream(root, &ordered, expected)?;
		}
		let mut repeated = NativeLink::default();
		expect_true!(
			record_link_tokens(
				&[
					first_archive.display().to_string(),
					second_archive.display().to_string(),
					first_archive.display().to_string()
				],
				&mut repeated
			)
			.is_err()
		);
		Ok(())
	}
}
