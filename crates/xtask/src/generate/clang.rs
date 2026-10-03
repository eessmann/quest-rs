use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use clang::diagnostic::Severity;
use clang::{Availability, Clang, Entity, EntityKind, Index};

use super::DynError;
use super::model::{ApiArgument, ApiItem, overload_key};

const QUEST_ENV_VARS: &[&str] = &["QUEST_ROOT"];

#[derive(Debug, Clone)]
pub struct QuestRoot {
	path: PathBuf,
	source: String,
}

impl QuestRoot {
	pub fn from_package(package: &quest_build::NativePackage) -> Self {
		Self {
			path: package.prefix.clone(),
			source: source_label(&package.prefix),
		}
	}

	pub fn path(&self) -> &Path {
		&self.path
	}

	pub fn source_label(&self) -> &str {
		&self.source
	}
}

#[cfg(test)]
pub fn fixture_root(path: PathBuf) -> QuestRoot {
	QuestRoot {
		path,
		source: "QUEST_ROOT".to_owned(),
	}
}

pub fn collect_quest_api(package: &quest_build::NativePackage) -> Result<Vec<ApiItem>, DynError> {
	let include_root = package.prefix.join("include");
	let quest_h = include_root.join("quest.h");

	if !quest_h.is_file() {
		return Err(format!("missing QuEST umbrella header at {}", quest_h.display()).into());
	}

	let include_root = canonicalize_existing(&include_root)?;
	let quest_h = canonicalize_existing(&quest_h)?;
	let _guard = clang_lock()
		.lock()
		.map_err(|_| "libclang mutex was poisoned")?;
	// clang-sys already searches llvm-config's prefix and explicit LIBCLANG_PATH.
	// Do not mutate the process environment while other test threads may read it.
	let clang = Clang::new().map_err(|error| format_libclang_error(&error))?;
	let index = Index::new(&clang, false, false);
	let args = clang_arguments(package, &include_root)?;
	let mut parser = index.parser(&quest_h);
	parser.arguments(&args);

	let translation_unit = parser
		.parse()
		.map_err(|error| format!("libclang failed to parse {}: {error:?}", quest_h.display()))?;

	let diagnostics = translation_unit
		.get_diagnostics()
		.into_iter()
		.filter(|diagnostic| matches!(diagnostic.get_severity(), Severity::Error | Severity::Fatal))
		.map(|diagnostic| format!("{:?}: {}", diagnostic.get_severity(), diagnostic.get_text()))
		.collect::<Vec<_>>();
	if !diagnostics.is_empty() {
		return Err(format!(
			"libclang reported errors while parsing {}:\n{}",
			quest_h.display(),
			diagnostics.join("\n")
		)
		.into());
	}

	let mut items = Vec::new();
	collect_function_decls(
		translation_unit.get_entity(),
		&include_root,
		&mut BTreeSet::new(),
		&mut items,
	);
	items.sort_by(|left, right| {
		left.overload_key
			.cmp(&right.overload_key)
			.then_with(|| left.header.cmp(&right.header))
			.then_with(|| left.line.cmp(&right.line))
	});

	Ok(items)
}

fn clang_lock() -> &'static Mutex<()> {
	static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
	LOCK.get_or_init(|| Mutex::new(()))
}

pub fn format_libclang_error(error: &str) -> String {
	format!(
		"could not load libclang: {error}\n\
         Set LIBCLANG_PATH to a directory containing libclang, make llvm-config available, \
         or install LLVM so the system loader can find libclang."
	)
}

fn clang_arguments(
	package: &quest_build::NativePackage,
	include_root: &Path,
) -> Result<Vec<String>, DynError> {
	// A libclang resource directory is compiler-specific. Refuse to silently
	// combine an explicitly selected libclang with a different driver version.
	let driver = clang_command();
	let driver_output = Command::new(&driver)
		.arg("--version")
		.output()
		.map_err(|error| format!("could not inspect parser driver {driver}: {error}"))?;
	let driver_version = String::from_utf8_lossy(&driver_output.stdout);
	let library_version = clang::get_version();
	if !driver_output.status.success()
		|| clang_major_version(&driver_version).is_none()
		|| clang_major_version(&driver_version) != clang_major_version(&library_version)
	{
		return Err(format!("libclang and parser driver must come from the same LLVM major version; libclang={library_version}; driver={driver_version}. Select matching LIBCLANG_PATH and CLANG.").into());
	}
	let mut args = vec![
		"-x".to_owned(),
		"c++".to_owned(),
		"-std=c++20".to_owned(),
		format!("-I{}", include_root.display()),
		format!("-I{}", include_root.join("quest/include").display()),
	];
	args.extend(header_context_arguments(&package.headers));

	if let Some(resource_dir) = clang_resource_dir() {
		args.push("-resource-dir".to_owned());
		args.push(resource_dir.display().to_string());
	}

	args.push("-target".to_owned());
	args.push(package.target.clone());

	if let Some(sdk_path) = parser_sysroot(&package.headers, macos_sdk_path) {
		args.push("-isysroot".to_owned());
		args.push(sdk_path.display().to_string());
		let sdk_include = sdk_path.join("usr").join("include");
		if sdk_include.is_dir() {
			args.push("-isystem".to_owned());
			args.push(sdk_include.display().to_string());
		}
		let sdk_frameworks = sdk_path.join("System").join("Library").join("Frameworks");
		if sdk_frameworks.is_dir() {
			args.push("-iframework".to_owned());
			args.push(sdk_frameworks.display().to_string());
		}
	}

	Ok(args)
}

fn clang_major_version(version: &str) -> Option<u32> {
	version
		.split_once("version ")?
		.1
		.split('.')
		.next()?
		.parse()
		.ok()
}

fn manifest_canonical_type(ty: &str) -> String {
	if ty == "std::string" {
		"std::basic_string<char>".to_owned()
	} else {
		ty.to_owned()
	}
}

fn parser_sysroot(
	headers: &quest_build::HeaderContext,
	fallback: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
	headers.sysroot.clone().or_else(fallback)
}

fn macos_sdk_path() -> Option<PathBuf> {
	if !cfg!(target_os = "macos") {
		return None;
	}

	let output = Command::new("xcrun").arg("--show-sdk-path").output().ok()?;
	if !output.status.success() {
		return None;
	}

	let text = String::from_utf8(output.stdout).ok()?;
	let path = PathBuf::from(text.trim());
	path.is_dir().then_some(path)
}

fn source_label(prefix: &Path) -> String {
	for variable in QUEST_ENV_VARS {
		if env::var_os(variable)
			.and_then(|value| PathBuf::from(value).canonicalize().ok())
			.is_some_and(|candidate| candidate == prefix)
		{
			return (*variable).to_owned();
		}
	}
	if env::var_os("CMAKE_PREFIX_PATH")
		.into_iter()
		.flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
		.filter_map(|candidate| candidate.canonicalize().ok())
		.any(|candidate| candidate == prefix)
	{
		return "CMAKE_PREFIX_PATH".to_owned();
	}
	"quest-build selected package".to_owned()
}

fn llvm_config_path(arg: &str) -> Option<PathBuf> {
	let output = Command::new("llvm-config").arg(arg).output().ok()?;
	if !output.status.success() {
		return None;
	}
	let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
	(!value.is_empty()).then_some(PathBuf::from(value))
}

fn header_context_arguments(headers: &quest_build::HeaderContext) -> Vec<String> {
	let mut arguments = Vec::new();
	for include in &headers.include_dirs {
		if !headers.system_include_dirs.contains(include) {
			arguments.push(format!("-I{}", include.display()));
		}
	}
	for include in &headers.system_include_dirs {
		arguments.push("-isystem".to_owned());
		arguments.push(include.display().to_string());
	}
	for include in &headers.implicit_include_dirs {
		if !headers.system_include_dirs.contains(include) {
			arguments.push("-isystem".to_owned());
			arguments.push(include.display().to_string());
		}
	}
	for definition in &headers.definitions {
		if definition.starts_with("-D") {
			arguments.push(definition.clone());
		} else {
			arguments.push(format!("-D{definition}"));
		}
	}
	let mut flags = headers.frontend_flags.iter();
	while let Some(flag) = flags.next() {
		match flag.as_str() {
			"-arch" | "-target" | "--target" => {
				flags.next();
			}
			"-isysroot" | "--sysroot" if headers.sysroot.is_some() => {
				flags.next();
			}
			_ if flag.starts_with("-isysroot=") && headers.sysroot.is_some() => {}
			_ if flag.starts_with("--target=") => {}
			_ => arguments.push(flag.clone()),
		}
	}
	arguments
}

fn clang_resource_dir() -> Option<PathBuf> {
	let output = Command::new(clang_command())
		.arg("-print-resource-dir")
		.output()
		.ok()?;
	if !output.status.success() {
		return None;
	}
	let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
	let path = PathBuf::from(value);
	path.is_dir().then_some(path)
}

fn clang_command() -> String {
	env::var("CLANG").unwrap_or_else(|_| {
		llvm_config_path("--bindir")
			.map(|bindir| bindir.join("clang"))
			.filter(|path| path.is_file())
			.map_or_else(|| "clang".to_owned(), |path| path.display().to_string())
	})
}

fn collect_function_decls(
	entity: Entity<'_>,
	include_root: &Path,
	seen: &mut BTreeSet<String>,
	items: &mut Vec<ApiItem>,
) {
	if entity.get_kind() == EntityKind::FunctionDecl
		&& let Some(item) = api_item_from_entity(entity, include_root)
		&& seen.insert(item.overload_key.clone())
	{
		items.push(item);
	}

	for child in entity.get_children() {
		collect_function_decls(child, include_root, seen, items);
	}
}

fn api_item_from_entity(entity: Entity<'_>, include_root: &Path) -> Option<ApiItem> {
	if entity.get_availability() == Availability::Deprecated {
		return None;
	}

	let name = entity.get_name()?;
	if name.starts_with('_') {
		return None;
	}

	let location = entity.get_location()?.get_file_location();
	let line = location.line;
	let file = location.file?.get_path();
	let file = canonicalize_existing(&file).ok()?;
	if !file.starts_with(include_root)
		|| file.file_name().is_some_and(|name| name == "deprecated.h")
	{
		return None;
	}

	let header = file
		.strip_prefix(include_root)
		.unwrap_or(file.as_path())
		.display()
		.to_string();
	let result_type = entity.get_result_type()?;
	let result_type_name = result_type.get_display_name();
	let result_canonical_type =
		manifest_canonical_type(&result_type.get_canonical_type().get_display_name());
	let arguments = entity
		.get_arguments()
		.unwrap_or_default()
		.into_iter()
		.enumerate()
		.map(|(index, argument)| {
			let ty = argument.get_type()?;
			Some(ApiArgument {
				name: argument
					.get_name()
					.filter(|name| !name.is_empty())
					.unwrap_or_else(|| format!("arg{index}")),
				ty: ty.get_display_name(),
				canonical_type: manifest_canonical_type(
					&ty.get_canonical_type().get_display_name(),
				),
			})
		})
		.collect::<Option<Vec<_>>>()?;
	let overload_key = overload_key(&name, &result_canonical_type, &arguments);
	let display_name = entity.get_display_name().unwrap_or_else(|| {
		let args = arguments
			.iter()
			.map(|argument| format!("{} {}", argument.ty, argument.name))
			.collect::<Vec<_>>()
			.join(", ");
		format!("{name}({args})")
	});
	let signature = format!("{result_type_name} {display_name}");

	Some(ApiItem {
		name,
		overload_key,
		header,
		line,
		linkage: entity
			.get_linkage()
			.map_or_else(|| "none".to_owned(), |linkage| format!("{linkage:?}")),
		availability: format!("{:?}", entity.get_availability()),
		result_type: result_type_name,
		result_canonical_type,
		arguments,
		signature,
		status: "unclassified".to_owned(),
		reason: String::new(),
	})
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, DynError> {
	path.canonicalize()
		.map_err(|error| format!("failed to canonicalize {}: {error}", path.display()).into())
}

#[cfg(test)]
pub fn fixture_package() -> Result<Option<quest_build::NativePackage>, DynError> {
	let explicitly_selected = QUEST_ENV_VARS
		.iter()
		// Removed selectors are explicit invalid inputs, not reason to skip a fixture.
		.chain(["QUEST_DIR", "QuEST_DIR", "QuEST_ROOT"].iter())
		.any(|name| env::var_os(name).is_some())
		|| env::var_os("CMAKE_PREFIX_PATH").is_some_and(|value| !value.is_empty());
	let work = tempfile::tempdir()?;
	match quest_build::discover_for_tooling(work.path(), None) {
		Ok(package) => Ok(Some(package)),
		Err(_) if !explicitly_selected => Ok(None),
		Err(error) => Err(error.into()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;

	#[gtest]
	fn libclang_error_mentions_expected_lookup_paths() -> googletest::Result<()> {
		let message = format_libclang_error("not found");

		verify_that!(
			message,
			all!(
				contains_substring("LIBCLANG_PATH"),
				contains_substring("llvm-config")
			)
		)
	}

	#[gtest]
	fn evaluated_header_context_is_forwarded_without_splitting_paths() -> googletest::Result<()> {
		let headers = quest_build::HeaderContext {
			include_dirs: vec![PathBuf::from("/opt/QuEST install/include")],
			system_include_dirs: vec![PathBuf::from("/opt/MPI include")],
			implicit_include_dirs: vec![PathBuf::from("/opt/LLVM include")],
			definitions: vec!["QUEST_MPI=1".to_owned()],
			frontend_flags: vec!["-pthread".to_owned()],
			sysroot: Some(PathBuf::from("/opt/macOS SDK")),
		};

		verify_that!(
			header_context_arguments(&headers),
			elements_are![
				eq("-I/opt/QuEST install/include"),
				eq("-isystem"),
				eq("/opt/MPI include"),
				eq("-isystem"),
				eq("/opt/LLVM include"),
				eq("-DQUEST_MPI=1"),
				eq("-pthread")
			]
		)
	}

	#[gtest]
	fn evaluated_sysroot_takes_precedence_over_fallback() -> googletest::Result<()> {
		let headers = quest_build::HeaderContext {
			sysroot: Some(PathBuf::from("/opt/evaluated SDK")),
			..Default::default()
		};
		let mut fallback_called = false;
		let sysroot = parser_sysroot(&headers, || {
			fallback_called = true;
			Some(PathBuf::from("/opt/ambient SDK"))
		});
		expect_that!(fallback_called, eq(false));
		verify_that!(sysroot, some(eq(&PathBuf::from("/opt/evaluated SDK"))))
	}

	#[gtest]
	fn evaluated_target_and_sysroot_are_not_repeated_from_cmake_flags() -> googletest::Result<()> {
		let headers = quest_build::HeaderContext {
			sysroot: Some(PathBuf::from("/opt/macOS SDK")),
			frontend_flags: vec![
				"-arch".into(),
				"arm64".into(),
				"-isysroot".into(),
				"/opt/macOS SDK".into(),
				"-mmacosx-version-min=14.0".into(),
			],
			..Default::default()
		};
		verify_that!(
			header_context_arguments(&headers),
			elements_are![eq("-mmacosx-version-min=14.0")]
		)
	}

	#[gtest]
	fn manifest_canonical_string_spelling_is_independent_of_standard_library()
	-> googletest::Result<()> {
		expect_that!(
			manifest_canonical_type("std::string"),
			eq("std::basic_string<char>")
		);
		verify_that!(
			manifest_canonical_type("std::basic_string<char>"),
			eq("std::basic_string<char>")
		)
	}

	#[gtest]
	fn libclang_extracts_representative_overloads() -> googletest::Result<()> {
		let Some(package) = fixture_package().or_fail()? else {
			eprintln!("skipping test because no QuEST package was discovered");
			return Ok(());
		};

		let items = collect_quest_api(&package).or_fail()?;
		let overloads = items
			.iter()
			.filter(|item| item.name == "applyCompMatr")
			.collect::<Vec<_>>();

		expect_that!(
			overloads.iter().any(|item| item
				.arguments
				.iter()
				.any(|arg| arg.ty.contains("std::vector<int>"))),
			eq(true)
		);
		expect_that!(
			overloads
				.iter()
				.any(|item| item.arguments.iter().any(|arg| arg.ty.contains('*'))),
			eq(true)
		);
		verify_that!(overloads.iter().all(|item| item.line > 0), eq(true))
	}

	#[gtest]
	fn explicit_broken_package_makes_generator_fixture_tests_fail() -> googletest::Result<()> {
		let work = tempfile::tempdir().or_fail()?;
		let missing = work.path().join("missing-quest-installation");
		let executable = std::env::current_exe().or_fail()?;
		for test in [
			"generate::clang::tests::libclang_extracts_representative_overloads",
			"generate::classify::tests::classification_distinguishes_overloads_by_signature_shape",
		] {
			let output = Command::new(&executable)
				.args(["--exact", test, "--nocapture"])
				.env("QUEST_ROOT", &missing)
				.env_remove("QUEST_DIR")
				.env_remove("QuEST_ROOT")
				.env_remove("QuEST_DIR")
				.env_remove("CMAKE_PREFIX_PATH")
				.output()
				.or_fail()?;
			if output.status.success() {
				return fail!(
					"{test} skipped an explicitly selected but broken package:\n{}\n{}",
					String::from_utf8_lossy(&output.stdout),
					String::from_utf8_lossy(&output.stderr)
				);
			}
			let output_text = format!(
				"{}{}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
			if !output_text.contains(missing.to_string_lossy().as_ref()) {
				return fail!(
					"{test} failed for a reason other than the selected package: {output_text}"
				);
			}
		}
		Ok(())
	}
}
