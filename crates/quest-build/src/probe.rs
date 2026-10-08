use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;

use crate::link::{NativeLink, record_link_tokens, split_flags};
#[cfg(test)]
use crate::{link::record_linked_file, validate_compiler_environment};
use cmake_file_api::{objects, query, reply};
use objects::codemodel_v2::Target;

use crate::{
	BridgeInputs, BuildError, HeaderContext, NativeBuildContext, NativePackage, Result, absolute,
	invalid, io, parse_header_configuration, run,
};

#[cfg(test)]
pub fn discover(
	work: &Path,
	host: &str,
	target: &str,
	inputs: Option<&BridgeInputs>,
) -> Result<NativePackage> {
	let context =
		NativeBuildContext::capture(work, host, target, env::var_os("OUT_DIR").is_some())?;
	let package = discover_context(&context, inputs)?;
	package.emit_cargo_input_metadata()?;
	Ok(package)
}

pub fn discover_context(
	context: &NativeBuildContext,
	inputs: Option<&BridgeInputs>,
) -> Result<NativePackage> {
	crate::validate_target(&context.host, &context.target)?;
	let target = context.target.as_str();
	let explicit = context
		.value("QUEST_ROOT")
		.map(|value| crate::installation_prefix(Path::new(value)))
		.transpose()?;
	let search_prefixes = context
		.value("CMAKE_PREFIX_PATH")
		.map_or_else(Vec::new, |value| {
			env::split_paths(value).collect::<Vec<_>>()
		});
	let setup = configure_context(context, explicit.as_deref(), inputs, &search_prefixes)?;
	let reader = reply::Reader::from_build_dir(&setup.build_directory)
		.map_err(|e| BuildError::CmakeFileApi(e.to_string()))?;
	let probe = read_target(&reader, &setup.profile, "quest_link_query")?;
	let compiler = read_compiler(&reader, target)?;
	let exported_includes = read_exported_includes(&setup.build_directory, &setup.profile)?;
	let (prefix, configuration, mut headers) =
		inspect_headers(&probe, explicit.as_deref(), &exported_includes)?;
	headers.implicit_include_dirs = compiler.implicit_include_dirs;
	headers.sysroot.clone_from(&setup.sysroot);
	validate_header_context(&headers, target)?;
	let mut link = inspect_link(
		&probe,
		&setup.build_directory,
		&setup.profile,
		target,
		&compiler.implicit_link_dirs,
	)?;
	// CMake's link fragments omit the driver's implicit standard library.
	// Use the evaluated toolchain rather than assuming GCC or Clang defaults.
	let stdlib = compiler.standard_library;
	link.libraries.push(stdlib);
	let mut watched_inputs =
		collect_input_watches(&reader, &probe, &link.linked_files, &exported_includes)?;
	watched_inputs.extend(bridge_input_watches(inputs)?);
	let bridge_archive = inputs
		.map(|_| {
			read_target(&reader, &setup.profile, "quest_bridge").and_then(|bridge| {
				bridge
					.artifacts
					.first()
					.map(|artifact| setup.build_directory.join(&artifact.path))
					.ok_or_else(|| invalid("CMake omitted the bridge archive"))
			})
		})
		.transpose()?;
	let package = NativePackage {
		context: context.clone(),
		watched_inputs,
		target: target.to_owned(),
		profile: setup.profile.clone(),
		prefix,
		version: configuration.version,
		mpi_enabled: configuration.mpi_enabled,
		subcommunicators_enabled: configuration.subcommunicators_enabled,
		headers,
		compiler: compiler.path,
		compiler_invocation: compiler.invocation,
		compiler_arguments: read_words(
			&setup.build_directory.join("quest-compiler-arguments.txt"),
		)?,
		implicit_link_dirs: compiler.implicit_link_dirs,
		cmake: context.cmake(),
		dynamic_loader_libraries: read_lines(
			&setup
				.build_directory
				.join("quest-dynamic-loader-libraries.txt"),
		)?,
		openmp_enabled: configuration.openmp_enabled,
		gpu_enabled: configuration.gpu_enabled,
		cuquantum_enabled: configuration.cuquantum_enabled,
		compiler_id: compiler.id,
		compiler_version: compiler.version,
		library: link.library,
		link_search_dirs: link.search_dirs,
		framework_search_dirs: link.framework_search_dirs,
		link_libraries: link.libraries,
		link_options: link.options,
		runtime_library_dirs: link.runtime_dirs,
		bridge_archive,
		exact_library_files: link.library_files_by_name,
		build_directory: setup.build_directory.clone(),
		mpi_probe: read_target(&reader, &setup.profile, "quest_mpi_abi")?
			.artifacts
			.first()
			.map(|artifact| setup.build_directory.join(&artifact.path))
			.ok_or_else(|| invalid("CMake omitted MPI ABI witness target"))?,
	};
	package.validate_link_search()?;
	Ok(package)
}

fn bridge_input_watches(inputs: Option<&BridgeInputs>) -> Result<BTreeSet<PathBuf>> {
	inputs
		.into_iter()
		.flat_map(|inputs| inputs.sources.iter().chain(&inputs.include_directories))
		.map(|path| absolute(path))
		.collect()
}

struct Setup {
	sysroot: Option<PathBuf>,
	build_directory: PathBuf,
	profile: String,
}

#[cfg(test)]
fn configure(
	work: &Path,
	host: &str,
	target: &str,
	explicit: Option<&Path>,
	inputs: Option<&BridgeInputs>,
	search_prefixes: &[PathBuf],
) -> Result<Setup> {
	let context =
		NativeBuildContext::capture(work, host, target, env::var_os("OUT_DIR").is_some())?;
	configure_context(&context, explicit, inputs, search_prefixes)
}

fn configure_context(
	context: &NativeBuildContext,
	explicit: Option<&Path>,
	inputs: Option<&BridgeInputs>,
	search_prefixes: &[PathBuf],
) -> Result<Setup> {
	let work = &context.work_directory;
	let target = context.target.as_str();
	let sysroot = if target.ends_with("-apple-darwin") {
		let path = if let Some(root) = context.value("SDKROOT") {
			PathBuf::from(root)
		} else {
			let output =
				run(context
					.command("xcrun")
					.args(["--sdk", "macosx", "--show-sdk-path"]))?;
			PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
		};
		Some(validate_sdk(&path)?)
	} else {
		None
	};
	let source = work.join("source");
	let profile = context.profile.clone();
	let prefix_value = search_prefixes
		.iter()
		.map(|path| cmake_path(path))
		.collect::<Result<Vec<_>>>()?
		.join(";");
	let package_directory = explicit.map(find_package_directory).transpose()?;
	let build_directory = work.join("build");
	prepare_project(&source, &build_directory, inputs, false)?;
	let cmake = context.cmake();
	let mut command = context.command(&cmake);
	command
		.arg("-S")
		.arg(&source)
		.arg("-B")
		.arg(&build_directory)
		.arg("--fresh")
		.arg(format!("-DCMAKE_BUILD_TYPE={profile}"))
		.arg(format!("-DCMAKE_PREFIX_PATH={prefix_value}"))
		.arg(format!(
			"-DQUEST_RUST_ARCH={}",
			target.split('-').next().unwrap_or_default()
		))
		.arg(format!(
			"-DQUEST_RUST_PLATFORM={}",
			if target.ends_with("-apple-darwin") {
				"darwin"
			} else {
				"linux_gnu"
			}
		));
	if let Some(root) = &sysroot {
		command.arg(format!("-DCMAKE_OSX_SYSROOT={}", cmake_path(root)?));
	}
	if let Some(directory) = &package_directory {
		command.arg(format!("-DQuEST_DIR={}", cmake_path(directory)?));
	}
	// CC/CXX, flags, toolchain files and CMake defaults are interpreted by CMake.
	run(&mut command).map_err(|error| BuildError::CmakeBuild(error.to_string()))?;
	let mut build = context.command(&cmake);
	build.arg("--build").arg(&build_directory).args([
		"--target",
		"quest_bridge",
		"--config",
		&profile,
	]);
	if context.value("CMAKE_BUILD_PARALLEL_LEVEL").is_none()
		&& let Some(jobs) = context.value("NUM_JOBS")
	{
		build.arg("--parallel").arg(jobs);
	}
	run(&mut build).map_err(|error| BuildError::CmakeBuild(error.to_string()))?;
	let evaluated_file = build_directory.join("quest-sysroot.txt");
	let evaluated =
		fs::read_to_string(&evaluated_file).map_err(|error| io(&evaluated_file, error))?;
	let evaluated = evaluated.trim();
	let evaluated_sysroot = if evaluated.is_empty() {
		None
	} else {
		let path = Path::new(evaluated);
		cmake_path(path)?;
		if !path.is_absolute() {
			return Err(invalid("CMake sysroot must be absolute"));
		}
		Some(if target.ends_with("-apple-darwin") {
			validate_sdk(path)?
		} else {
			fs::canonicalize(path).map_err(|error| io(path, error))?
		})
	};
	if sysroot.is_some() && sysroot != evaluated_sysroot {
		return Err(invalid("CMake changed the selected Darwin SDK"));
	}
	let sysroot = evaluated_sysroot;
	Ok(Setup {
		sysroot,
		build_directory,
		profile,
	})
}

fn prepare_project(
	source: &Path,
	build_directory: &Path,
	inputs: Option<&BridgeInputs>,
	cargo: bool,
) -> Result<()> {
	fs::create_dir_all(source).map_err(|error| io(source, error))?;
	fs::create_dir_all(build_directory).map_err(|error| io(build_directory, error))?;
	for (name, content) in [
		("CMakeLists.txt", include_str!("../native/CMakeLists.txt")),
		("abi.cpp", include_str!("../native/abi.cpp")),
		("query.cpp", include_str!("../native/query.cpp")),
		("mpi_abi.c", include_str!("../native/mpi_abi.c")),
	] {
		write(&source.join(name), content)?;
	}
	write(
		&source.join("bridge-inputs.cmake"),
		&bridge_input_file_with_watches(inputs, cargo)?,
	)?;
	query::Writer::default()
		.request_object::<objects::CodeModelV2>()
		.request_object::<objects::ToolchainsV1>()
		.request_object::<objects::CMakeFilesV1>()
		.write_stateless(build_directory)
		.map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
	Ok(())
}

#[cfg(test)]
fn bridge_input_file(inputs: Option<&BridgeInputs>) -> Result<String> {
	bridge_input_file_with_watches(inputs, true)
}

fn bridge_input_file_with_watches(inputs: Option<&BridgeInputs>, cargo: bool) -> Result<String> {
	let mut text = String::new();
	let mut watched = BTreeSet::new();
	let empty = BridgeInputs::default();
	let inputs = inputs.unwrap_or(&empty);
	for (name, paths) in [
		("QUEST_BRIDGE_SOURCES", &inputs.sources),
		(
			"QUEST_BRIDGE_INCLUDE_DIRECTORIES",
			&inputs.include_directories,
		),
	] {
		let _ = writeln!(text, "set({name}");
		for path in paths {
			let path = absolute(path)?;
			let value = cmake_path(&path)?;
			let mut equals = String::new();
			while value.contains(&format!("]{equals}]")) {
				equals.push('=');
			}
			let _ = writeln!(text, "  [{equals}[{value}]{equals}]");
			watched.insert(path);
		}
		text.push_str(")\n");
	}
	if cargo {
		emit_input_watches(watched)?;
	}
	Ok(text)
}

pub fn cmake_path(path: &Path) -> Result<&str> {
	let text = path
		.to_str()
		.ok_or_else(|| invalid("CMake paths must be UTF-8"))?;
	if text.contains([';', '\n', '\r', '\0']) || text.contains("$<") {
		return Err(invalid(format!(
			"CMake path cannot be represented safely: {}",
			path.display()
		)));
	}
	Ok(text)
}

fn read_target(reader: &reply::Reader, profile: &str, name: &str) -> Result<Target> {
	let model: objects::CodeModelV2 = reader
		.read_object()
		.map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
	model
		.configurations
		.into_iter()
		.find(|configuration| configuration.name == profile)
		.and_then(|configuration| {
			configuration
				.targets
				.into_iter()
				.find(|target| target.name == name)
		})
		.ok_or_else(|| invalid(format!("CMake File API omitted {profile} {name}")))
}

struct CompilerConfiguration {
	path: PathBuf,
	invocation: PathBuf,
	implicit_link_dirs: Vec<PathBuf>,
	id: String,
	version: String,
	standard_library: String,
	implicit_include_dirs: Vec<PathBuf>,
}

fn read_compiler(reader: &reply::Reader, target: &str) -> Result<CompilerConfiguration> {
	let toolchains: objects::ToolchainsV1 = reader
		.read_object()
		.map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
	let compiler = toolchains
		.toolchains
		.into_iter()
		.find(|toolchain| toolchain.language == "CXX")
		.ok_or_else(|| invalid("CMake File API omitted the C++ compiler"))?
		.compiler;
	let compiler_path = compiler
		.path
		.as_ref()
		.ok_or_else(|| invalid("CMake did not identify its C++ compiler"))?;
	let path = fs::canonicalize(compiler_path).map_err(|error| io(compiler_path, error))?;
	// Native architecture/platform admission is compiled in abi.cpp using the
	// evaluated toolchain, including wrapper arguments and flags.
	let id = compiler.id.unwrap_or_default();
	let standard_library = select_standard_library(target, &id, &compiler.implicit.link_libraries)?;
	let implicit_include_dirs = compiler
		.implicit
		.include_directories
		.into_iter()
		.map(|path| {
			cmake_path(&path)?;
			if !path.is_absolute() {
				return Err(invalid("relative compiler implicit include directory"));
			}
			Ok(path)
		})
		.collect::<Result<Vec<_>>>()?;
	Ok(CompilerConfiguration {
		invocation: compiler_path.clone(),
		implicit_link_dirs: compiler.implicit.link_directories,
		path,
		id,
		version: compiler.version.unwrap_or_default(),
		standard_library,
		implicit_include_dirs,
	})
}

fn select_standard_library(target: &str, id: &str, libraries: &[PathBuf]) -> Result<String> {
	libraries
		.iter()
		.filter_map(|path| path.to_str())
		.find(|name| matches!(*name, "stdc++" | "c++"))
		// CMake can omit implicit libraries for the validated Darwin Clang driver.
		.or_else(|| {
			(target.ends_with("-apple-darwin") && matches!(id, "Clang" | "AppleClang"))
				.then_some("c++")
		})
		.map(str::to_owned)
		.ok_or_else(|| invalid("CMake did not identify a supported C++ standard library"))
}

fn validate_compiler_target(target: &str, compiler: &str) -> Result<()> {
	let arch = target.split('-').next().unwrap_or_default();
	let compiler_arch = compiler.split('-').next().unwrap_or_default();
	let darwin = target.ends_with("-apple-darwin");
	let same_arch =
		arch == compiler_arch || (darwin && arch == "aarch64" && compiler_arch == "arm64");
	let same_platform = if darwin {
		compiler.contains("-apple-darwin")
	} else {
		// Red Hat's native GNU/Linux GCC target omits the final GNU suffix.
		compiler.contains("-linux-gnu")
			|| compiler.strip_suffix("-redhat-linux") == Some(compiler_arch)
	};
	if !same_arch || !same_platform {
		return Err(invalid(format!(
			"C++ compiler targets {compiler}, Rust targets {target}"
		)));
	}
	Ok(())
}

pub fn validate_sdk(path: &Path) -> Result<PathBuf> {
	cmake_path(path)?;
	if !path.is_absolute()
		|| !path.join("usr/include").is_dir()
		|| !path.join("System/Library/Frameworks").is_dir()
		|| !(path.join("SDKSettings.plist").is_file() || path.join("SDKSettings.json").is_file())
	{
		return Err(invalid(
			"SDKROOT must select an absolute installed macOS SDK with SDKSettings and system headers",
		));
	}
	fs::canonicalize(path).map_err(|error| io(path, error))
}

fn validate_header_context(headers: &HeaderContext, target: &str) -> Result<()> {
	let mut flags = headers.frontend_flags.iter();
	while let Some(flag) = flags.next() {
		let (kind, value) = match flag.as_str() {
			"-arch" | "-target" | "--target" | "-isysroot" | "--sysroot" => (
				flag.as_str(),
				flags
					.next()
					.map(String::as_str)
					.ok_or_else(|| invalid(format!("missing compile argument after {flag}")))?,
			),
			_ => {
				if let Some(value) = flag
					.strip_prefix("--target=")
					.or_else(|| flag.strip_prefix("-target="))
				{
					("-target", value)
				} else if let Some(value) = flag.strip_prefix("--sysroot=") {
					("-isysroot", value)
				} else if let Some(value) = flag.strip_prefix("-isysroot") {
					("-isysroot", value)
				} else if matches!(flag.as_str(), "-m32" | "-m64") {
					let bits32 = matches!(target.split('-').next(), Some("i686" | "armv7"));
					if (flag == "-m32") != bits32 {
						return Err(invalid(
							"compile pointer width differs from the native Cargo target",
						));
					}
					continue;
				} else if flag.starts_with("-Xarch_") {
					return Err(invalid(format!(
						"unsupported compile architecture override {flag}"
					)));
				} else {
					continue;
				}
			}
		};
		match kind {
			"-arch" => {
				let expected = match target {
					"aarch64-apple-darwin" => "arm64",
					"x86_64-apple-darwin" => "x86_64",
					_ => return Err(invalid("-arch compile option requires a Darwin target")),
				};
				if value != expected {
					return Err(invalid(
						"compile architecture differs from the native Cargo target",
					));
				}
			}
			"-target" | "--target" => validate_compiler_target(target, value)?,
			_ => {
				let path = Path::new(value);
				if !path.is_absolute() {
					return Err(invalid("compile sysroot must be absolute"));
				}
				let canonical = fs::canonicalize(path).map_err(|error| io(path, error))?;
				if headers.sysroot.as_ref() != Some(&canonical) {
					return Err(invalid("compile sysroot differs from the evaluated SDK"));
				}
			}
		}
	}
	Ok(())
}

fn read_exported_includes(build_directory: &Path, profile: &str) -> Result<Vec<PathBuf>> {
	let file = build_directory.join(format!("quest-includes-{profile}.txt"));
	let text = fs::read_to_string(&file).map_err(|error| io(&file, error))?;
	text.lines()
		.filter(|line| !line.is_empty())
		.map(|line| {
			let path = Path::new(line);
			cmake_path(path)?;
			if !path.is_absolute() {
				return Err(invalid("relative imported QuEST include directory"));
			}
			fs::canonicalize(path).map_err(|error| io(path, error))
		})
		.collect()
}

fn inspect_headers(
	probe: &Target,
	explicit: Option<&Path>,
	exported_includes: &[PathBuf],
) -> Result<(PathBuf, crate::HeaderConfiguration, HeaderContext)> {
	let mut headers = HeaderContext::default();
	for group in &probe.compile_groups {
		for include in &group.includes {
			let path = fs::canonicalize(&include.path).map_err(|error| io(&include.path, error))?;
			cmake_path(&path)?;
			if include.is_system {
				push_unique(&mut headers.system_include_dirs, path.clone());
			}
			push_unique(&mut headers.include_dirs, path);
		}
		headers.definitions.extend(
			group
				.defines
				.iter()
				.map(|definition| definition.define.clone()),
		);
		for fragment in &group.compile_command_fragments {
			for flag in split_flags(&fragment.fragment)? {
				if flag.starts_with("-O") || flag.starts_with("-g") || flag == "-DNDEBUG" {
					continue;
				}
				headers.frontend_flags.push(flag);
			}
		}
	}
	for path in exported_includes {
		if !headers.include_dirs.contains(path) {
			// A target include absent from the codemodel is compiler-implicit.
			push_unique(&mut headers.include_dirs, path.clone());
			push_unique(&mut headers.system_include_dirs, path.clone());
		}
	}
	let prefix = exported_includes
		.iter()
		.chain(&headers.include_dirs)
		.find(|directory| directory.join("quest.h").is_file())
		.and_then(|directory| directory.parent())
		.ok_or_else(|| invalid("QuEST::QuEST did not supply installed include/quest.h"))?;
	let prefix = fs::canonicalize(prefix).map_err(|error| io(prefix, error))?;
	if explicit.is_some_and(|requested| requested != prefix) {
		return Err(invalid(
			"CMake selected a different installation from the explicit QuEST prefix",
		));
	}
	let config_header = prefix.join("include/quest/include/config.h");
	let parsed = parse_header_configuration(
		&fs::read_to_string(&config_header).map_err(|error| io(&config_header, error))?,
	)?;
	Ok((prefix, parsed, headers))
}

fn inspect_link(
	probe: &Target,
	build_directory: &Path,
	profile: &str,
	target: &str,
	implicit_search_dirs: &[PathBuf],
) -> Result<NativeLink> {
	let file = build_directory.join(format!("quest-library-{profile}.txt"));
	let text = fs::read_to_string(&file).map_err(|error| io(&file, error))?;
	let library =
		fs::canonicalize(text.trim()).map_err(|error| io(Path::new(text.trim()), error))?;
	let model = probe
		.link
		.as_ref()
		.ok_or_else(|| invalid("CMake link query has no link model"))?;
	let sysroot = if target.ends_with("-apple-darwin") {
		let file = build_directory.join("quest-sysroot.txt");
		let value = fs::read_to_string(&file).map_err(|error| io(&file, error))?;
		Some(validate_sdk(Path::new(value.trim()))?)
	} else {
		None
	};
	let mut link = NativeLink {
		implicit_search_dirs: implicit_search_dirs.to_vec(),
		sysroot,
		target_os: Some(if target.ends_with("-apple-darwin") {
			"macos"
		} else {
			"linux"
		}),
		architecture: match target {
			"aarch64-apple-darwin" => Some("arm64"),
			"x86_64-apple-darwin" => Some("x86_64"),
			_ => None,
		},
		library,
		..NativeLink::default()
	};
	let tokens = model
		.command_fragments
		.iter()
		.map(|fragment| split_flags(&fragment.fragment))
		.collect::<Result<Vec<_>>>()?
		.into_iter()
		.flatten()
		.collect::<Vec<_>>();
	record_link_tokens(&tokens, &mut link)?;
	if !link.linked_files.contains(&link.library) {
		return Err(invalid(
			"CMake link model does not contain the selected QuEST library",
		));
	}
	Ok(link)
}

fn collect_input_watches(
	reader: &reply::Reader,
	probe: &Target,
	linked_files: &BTreeSet<PathBuf>,
	exported_includes: &[PathBuf],
) -> Result<BTreeSet<PathBuf>> {
	let mut inputs = linked_files.clone();
	for include in probe
		.compile_groups
		.iter()
		.flat_map(|group| &group.includes)
	{
		collect_files(&include.path, &mut inputs, &mut BTreeSet::new())?;
	}
	for include in exported_includes {
		collect_files(include, &mut inputs, &mut BTreeSet::new())?;
	}
	let cmake_files: objects::CMakeFilesV1 = reader
		.read_object()
		.map_err(|error| BuildError::CmakeFileApi(error.to_string()))?;
	for input in cmake_files.inputs {
		if input.path.is_absolute() && input.path.is_file() {
			inputs.insert(fs::canonicalize(&input.path).map_err(|error| io(&input.path, error))?);
			inputs.insert(input.path);
		}
	}
	Ok(inputs)
}

pub fn emit_input_watches(inputs: BTreeSet<PathBuf>) -> Result<()> {
	emit_input_watches_in(inputs, env::var_os("OUT_DIR").as_deref())
}

pub fn emit_input_watches_in(
	inputs: BTreeSet<PathBuf>,
	output: Option<&std::ffi::OsStr>,
) -> Result<()> {
	let output = output
		.map(|path| {
			let path = Path::new(&path);
			fs::canonicalize(path).map_err(|error| io(path, error))
		})
		.transpose()?;
	for path in inputs {
		// CXX and CMake recreate these outputs after Cargo starts the build
		// script. Watching them makes every subsequent build appear stale.
		// Resolve aliases before deciding ownership: a lexical OUT_DIR prefix
		// can lead outside through `..` or a symlink. Keep unresolved inputs
		// watched so a subsequently created external path can invalidate Cargo.
		if output
			.as_ref()
			.is_some_and(|root| fs::canonicalize(&path).is_ok_and(|path| path.starts_with(root)))
		{
			continue;
		}
		println!("cargo:rerun-if-changed={}", path.display());
	}
	Ok(())
}
fn find_package_directory(prefix: &Path) -> Result<PathBuf> {
	// QuEST_DIR chooses the requested package even if a different package is
	// discoverable through unrelated CMAKE_PREFIX_PATH entries.
	["lib/cmake/QuEST", "lib64/cmake/QuEST", "share/QuEST/cmake"]
		.into_iter()
		.map(|relative| prefix.join(relative))
		.find(|path| path.join("QuESTConfig.cmake").is_file())
		.ok_or_else(|| {
			invalid(format!(
				"{} has headers but no installed QuEST CMake package",
				prefix.display()
			))
		})
}

fn collect_files(
	directory: &Path,
	files: &mut BTreeSet<PathBuf>,
	visited: &mut BTreeSet<PathBuf>,
) -> Result<()> {
	let canonical = fs::canonicalize(directory).map_err(|e| io(directory, e))?;
	files.insert(directory.to_owned());
	files.insert(canonical.clone());
	if !visited.insert(canonical.clone()) {
		return Ok(());
	}
	for entry in fs::read_dir(directory).map_err(|e| io(directory, e))? {
		let path = entry.map_err(|e| io(directory, e))?.path();
		if path.is_dir() {
			collect_files(&path, files, visited)?;
		} else if path.is_file() {
			files.insert(fs::canonicalize(&path).map_err(|e| io(&path, e))?);
			files.insert(path);
		}
	}
	visited.remove(&canonical);
	Ok(())
}

fn write(path: &Path, text: &str) -> Result<()> {
	fs::write(path, text).map_err(|e| io(path, e))
}
pub fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
	if !items.contains(&item) {
		items.push(item);
	}
}

fn read_words(path: &Path) -> Result<Vec<String>> {
	split_flags(&fs::read_to_string(path).map_err(|error| io(path, error))?)
}
fn read_lines(path: &Path) -> Result<Vec<String>> {
	Ok(fs::read_to_string(path)
		.map_err(|error| io(path, error))?
		.lines()
		.filter(|line| !line.is_empty())
		.map(str::to_owned)
		.collect())
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;

	fn fixture_package(prefix: &Path) -> googletest::Result<()> {
		let include = prefix.join("include");
		let package = prefix.join("lib/cmake/QuEST");
		fs::create_dir_all(include.join("quest/include")).or_fail()?;
		fs::create_dir_all(&package).or_fail()?;
		fs::write(include.join("quest.h"), "#include <quest/include/config.h>\nusing qreal = double;\ninline bool isQuESTEnvInit() { return false; }\n").or_fail()?;
		fs::write(include.join("quest/include/config.h"), "#define QUEST_VERSION_MAJOR 4\n#define QUEST_VERSION_MINOR 3\n#define QUEST_VERSION_PATCH 9\n#define QUEST_FLOAT_PRECISION 2\n#define QUEST_INCLUDE_DEPRECATED_FUNCTIONS 0\n").or_fail()?;
		fs::write(
			prefix.join(if cfg!(target_os = "macos") {
				"lib/libQuEST.dylib"
			} else {
				"lib/libQuEST.so"
			}),
			"unused imported location: archive build never links it",
		)
		.or_fail()?;
		fs::write(
			package.join("QuESTConfigVersion.cmake"),
			"set(PACKAGE_VERSION 4.3.9)\nset(PACKAGE_VERSION_COMPATIBLE TRUE)\n",
		)
		.or_fail()?;
		fs::write(
            package.join("QuESTConfig.cmake"),
            r#"set(QuEST_VERSION 4.3.9)
get_filename_component(fixture_prefix "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
add_library(QuEST::QuEST SHARED IMPORTED)
set_target_properties(QuEST::QuEST PROPERTIES
  IMPORTED_LOCATION "${fixture_prefix}/lib/libQuEST${CMAKE_SHARED_LIBRARY_SUFFIX}"
  INTERFACE_INCLUDE_DIRECTORIES "${fixture_prefix}/include"
  INTERFACE_COMPILE_DEFINITIONS "$<$<CONFIG:Release>:QUEST_FIXTURE_EVALUATED=1>;$<$<CONFIG:Debug>:QUEST_FIXTURE_EVALUATED=2>"
  INTERFACE_COMPILE_OPTIONS "$<$<COMPILE_LANGUAGE:CXX>:-DQUEST_FIXTURE_CXX=1>"
  INTERFACE_LINK_OPTIONS "$<$<AND:$<CONFIG:Release>,$<PLATFORM_ID:Linux>>:LINKER:--enable-new-dtags>"
  INTERFACE_LINK_LIBRARIES "$<$<CONFIG:Release>:m>")
"#,
        )
        .or_fail()?;
		Ok(())
	}

	#[cfg(unix)]
	#[gtest]
	fn tooling_preserves_toolchain_wrapper_arguments_and_normal_environment()
	-> googletest::Result<()> {
		use std::os::unix::fs::PermissionsExt as _;
		const CHILD: &str = "QUEST_NATIVE_CONTEXT_CHILD";
		if let Some(root) = env::var_os(CHILD) {
			let root = PathBuf::from(root);
			let source = root.join("environment.cpp");
			fs::write(
				&source,
				"#include <environment_marker.h>\nstatic_assert(QUEST_ENV_FLAG == 31);\nstatic_assert(QUEST_CPATH_MARKER == 47);\nstatic_assert(QUEST_WRAPPER_ARG == 59);\n",
			)?;
			let context = NativeBuildContext::for_tooling(root.join("work"), None)?
				.request()
				.capture()?;
			let native = context.build_bridge(&BridgeInputs {
				sources: vec![source],
				include_directories: Vec::new(),
			})?;
			expect_eq!(&native.compiler_invocation, &root.join("CC"));
			expect_true!(native.watched_inputs.contains(&native.library));
			expect_true!(
				native
					.watched_inputs
					.contains(&root.join("package/include/quest.h"))
			);
			expect_eq!(&native.compiler, &root.join("dispatcher"));
			expect_eq!(
				&native.compiler_arguments,
				&vec!["-DQUEST_WRAPPER_ARG=59".to_owned()]
			);
			expect_that!(
				&native.headers.frontend_flags,
				contains(eq("-DQUEST_ENV_FLAG=31"))
			);
			return Ok(());
		}
		let fixture = tempfile::tempdir()?;
		let root = fixture.path().canonicalize()?;
		fixture_package(&root.join("package"))?;
		fs::create_dir(root.join("headers"))?;
		fs::write(
			root.join("headers/environment_marker.h"),
			"#define QUEST_CPATH_MARKER 47\n",
		)?;
		let compiler = env::split_paths(&env::var_os("PATH").unwrap_or_default())
			.map(|dir| dir.join("c++"))
			.find(|path| path.is_file())
			.ok_or_else(|| invalid("missing C++ compiler"))?;
		let quote =
			|path: &Path| format!("'{}'", path.display().to_string().replace('\'', "'\\''"));
		fs::write(
			root.join("dispatcher"),
			format!(
				"#!/bin/sh\ncase \"$0\" in */CC) ;; *) exit 92;; esac\nfor arg do [ \"$arg\" = -dumpmachine ] && exit 91; done\nexec {} \"$@\"\n",
				quote(&compiler)
			),
		)?;
		fs::set_permissions(root.join("dispatcher"), fs::Permissions::from_mode(0o755))?;
		std::os::unix::fs::symlink(root.join("dispatcher"), root.join("CC"))?;
		fs::write(
			root.join("toolchain.cmake"),
			format!(
				"set(CMAKE_CXX_COMPILER \"{}\")\nset(CMAKE_CXX_COMPILER_ARG1 \"-DQUEST_WRAPPER_ARG=59\")\n",
				root.join("CC").display()
			),
		)?;
		let mut child = Command::new(env::current_exe()?);
		child
			.args([
				"--exact",
				"probe::tests::tooling_preserves_toolchain_wrapper_arguments_and_normal_environment",
				"--nocapture",
			])
			.env(CHILD, &root)
			.env("QUEST_ROOT", root.join("package"))
			.env("CMAKE_TOOLCHAIN_FILE", root.join("toolchain.cmake"))
			.env("CPATH", root.join("headers"))
			.env(
				"CXXFLAGS",
				"-DQUEST_ENV_FLAG=31 -fno-exceptions -Werror -fPIC",
			)
			.env("OUT_DIR", root.join("irrelevant-cargo-output"))
			.env_remove("CXX");
		let output = run(&mut child)?;
		expect_false!(String::from_utf8_lossy(&output.stdout).contains("cargo:"));
		Ok(())
	}

	#[gtest]
	fn cargo_watches_external_inputs_without_watching_its_own_outputs() -> googletest::Result<()> {
		const CHILD: &str = "QUEST_BUILD_WATCH_INPUTS_CHILD";
		if let Some(root) = env::var_os(CHILD) {
			return emit_fixture_input_watches(Path::new(&root));
		}
		let fixture = tempfile::tempdir().or_fail()?;
		let root = fixture.path().canonicalize().or_fail()?;
		let output = Command::new(env::current_exe().or_fail()?)
			.args([
				"--exact",
				"probe::tests::cargo_watches_external_inputs_without_watching_its_own_outputs",
				"--nocapture",
			])
			.env(CHILD, &root)
			.env("OUT_DIR", root.join("own output"))
			.env("PROFILE", "release")
			.env("OPT_LEVEL", "3")
			.env("DEBUG", "false")
			.output()
			.or_fail()?;
		if !output.status.success() {
			return fail!(
				"watch fixture failed:\n{}\n{}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
		}
		let stdout = String::from_utf8_lossy(&output.stdout);
		let watched: BTreeSet<_> = stdout
			.lines()
			.filter_map(|line| line.strip_prefix("cargo:rerun-if-changed="))
			.map(PathBuf::from)
			.collect();
		let own_output = root.join("own output");
		expect_that!(
			watched
				.iter()
				.filter(|path| {
					path.canonicalize()
						.is_ok_and(|path| path.starts_with(&own_output))
				})
				.collect::<Vec<_>>(),
			is_empty()
		);
		for external in [
			"source.cpp",
			"own output/../external.cpp",
			"own output/../future headers",
			"other output",
			"package/include/quest.h",
			"package/lib/cmake/QuEST/QuESTConfig.cmake",
			if cfg!(target_os = "macos") {
				"package/lib/libQuEST.dylib"
			} else {
				"package/lib/libQuEST.so"
			},
		] {
			expect_that!(&watched, contains(eq(&root.join(external))));
		}
		#[cfg(unix)]
		expect_that!(&watched, contains(eq(&own_output.join("external headers"))));
		Ok(())
	}

	fn emit_fixture_input_watches(root: &Path) -> googletest::Result<()> {
		let output = root.join("own output");
		let other_output = root.join("other output");
		fs::create_dir_all(&output).or_fail()?;
		fs::create_dir_all(&other_output).or_fail()?;
		let generated = output.join("generated.cpp");
		let source = root.join("source.cpp");
		let external = output.join("../external.cpp");
		fs::write(&generated, "int generated_value() { return 1; }").or_fail()?;
		fs::write(&source, "int source_value() { return 2; }").or_fail()?;
		fs::write(&external, "int external_value() { return 3; }").or_fail()?;
		let prefix = root.join("package");
		fixture_package(&prefix)?;
		let mut inputs = BridgeInputs {
			sources: vec![generated.clone(), source, external],
			include_directories: vec![
				output.clone(),
				other_output,
				output.join("../future headers"),
			],
		};
		#[cfg(unix)]
		{
			let alias = output.join("external headers");
			std::os::unix::fs::symlink(prefix.join("include"), &alias).or_fail()?;
			inputs.include_directories.push(alias);
		}
		let host = fixture_host()?;
		let work = output.join("quest-native");
		let setup = configure(&work, &host, &host, Some(&prefix), Some(&inputs), &[]).or_fail()?;
		emit_input_watches(
			inputs
				.sources
				.iter()
				.chain(&inputs.include_directories)
				.cloned()
				.collect(),
		)?;
		let bridge_inputs =
			fs::read_to_string(work.join("source/bridge-inputs.cmake")).or_fail()?;
		expect_that!(
			bridge_inputs,
			contains_substring(generated.to_string_lossy())
		);
		let reader = reply::Reader::from_build_dir(&setup.build_directory).or_fail()?;
		let probe = read_target(&reader, &setup.profile, "quest_link_query").or_fail()?;
		let native = prefix.join(if cfg!(target_os = "macos") {
			"lib/libQuEST.dylib"
		} else {
			"lib/libQuEST.so"
		});
		emit_input_watches(collect_input_watches(
			&reader,
			&probe,
			&BTreeSet::from([native]),
			&[],
		)?)
		.or_fail()?;
		Ok(())
	}

	#[gtest]
	fn cargo_discovery_evaluates_the_selected_package_once() -> googletest::Result<()> {
		if env::var_os("QUEST_BUILD_SINGLE_EVALUATION_CHILD").is_none() {
			let work = tempfile::tempdir().or_fail()?;
			let output = Command::new(env::current_exe().or_fail()?)
				.args([
					"--exact",
					"probe::tests::cargo_discovery_evaluates_the_selected_package_once",
					"--nocapture",
				])
				.env("OUT_DIR", work.path())
				.env("PROFILE", "release")
				.env("OPT_LEVEL", "3")
				.env("DEBUG", "false")
				.env("QUEST_BUILD_SINGLE_EVALUATION_CHILD", "1")
				.output()
				.or_fail()?;
			if !output.status.success() {
				return fail!(
					"discovery child failed:\n{}\n{}",
					String::from_utf8_lossy(&output.stdout),
					String::from_utf8_lossy(&output.stderr)
				);
			}
			return Ok(());
		}

		let fixture = tempfile::tempdir().or_fail()?;
		let prefix = fixture.path().join("package");
		fixture_package(&prefix)?;
		let config = prefix.join("lib/cmake/QuEST/QuESTConfig.cmake");
		let original = fs::read_to_string(&config).or_fail()?;
		let marker = prefix.join("evaluated.marker");
		let guard = format!(
			"if(EXISTS \"{}\")\n  message(FATAL_ERROR \"QuEST package evaluated twice\")\nendif()\nfile(WRITE \"{}\" \"once\")\n",
			marker.display(),
			marker.display()
		);
		fs::write(&config, format!("{guard}{original}")).or_fail()?;
		let host = fixture_host()?;
		let work = fixture.path().join("build");
		configure(&work, &host, &host, Some(&prefix), None, &[]).or_fail()?;
		expect_that!(marker.is_file(), eq(true));
		Ok(())
	}

	#[gtest]
	fn reconfiguration_uses_current_prefix_after_explicit_selection_is_removed()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir().or_fail()?;
		let first = fixture.path().join("first");
		let second = fixture.path().join("second");
		fixture_package(&first)?;
		fixture_package(&second)?;
		let work = fixture.path().join("reused build");
		let output = run(Command::new("rustc").arg("-vV")).or_fail()?;
		let version = String::from_utf8_lossy(&output.stdout);
		let host = version
			.lines()
			.find_map(|line| line.strip_prefix("host: "))
			.ok_or_else(|| invalid("missing test host"))
			.or_fail()?;
		for (explicit, search, expected) in [
			(Some(first.as_path()), second.as_path(), first.as_path()),
			(None, second.as_path(), second.as_path()),
			(None, first.as_path(), first.as_path()),
		] {
			let setup =
				configure(&work, host, host, explicit, None, &[search.to_owned()]).or_fail()?;
			let reader = reply::Reader::from_build_dir(&setup.build_directory).or_fail()?;
			let query = read_target(&reader, &setup.profile, "quest_link_query").or_fail()?;
			let (selected, _, _) = inspect_headers(&query, None, &[]).or_fail()?;
			expect_that!(selected, eq(&expected.canonicalize().or_fail()?));
		}
		Ok(())
	}

	#[gtest]
	fn cmake_build_evaluates_target_requirements_and_paths_with_spaces() -> googletest::Result<()> {
		let fixture = tempfile::tempdir().or_fail()?;
		let prefix = fixture.path().join("prefix with spaces");
		fixture_package(&prefix)?;
		let source = fixture.path().join("generated bridge.cpp");
		fs::write(&source, "#include <quest.h>\n#if !QUEST_FIXTURE_EVALUATED || !QUEST_FIXTURE_CXX\n#error imported target requirements were lost\n#endif\nqreal bridge_value() { return 1.0; }\n").or_fail()?;
		let work = fixture.path().join("build with spaces");
		let host_output = run(Command::new("rustc").arg("-vV")).or_fail()?;
		let host_text = String::from_utf8_lossy(&host_output.stdout);
		let host = host_text
			.lines()
			.find_map(|line| line.strip_prefix("host: "))
			.ok_or_else(|| invalid("missing test host"))
			.or_fail()?;
		let setup = configure(
			&work,
			host,
			host,
			Some(&prefix),
			Some(&BridgeInputs {
				sources: vec![source],
				include_directories: Vec::new(),
			}),
			&[],
		)
		.or_fail()?;
		let reader = reply::Reader::from_build_dir(&setup.build_directory).or_fail()?;
		let query = read_target(&reader, &setup.profile, "quest_link_query").or_fail()?;
		let (_, configuration, headers) =
			inspect_headers(&query, Some(&prefix.canonicalize().or_fail()?), &[]).or_fail()?;
		let link =
			inspect_link(&query, &setup.build_directory, &setup.profile, host, &[]).or_fail()?;
		expect_eq!(configuration.version, "4.3.9");
		let expected_profile =
			env::var("QUEST_BUILD_FIXTURE_PROFILE").unwrap_or_else(|_| "Release".to_owned());
		expect_eq!(&setup.profile, &expected_profile);
		let compiler = read_compiler(&reader, host).or_fail()?;
		validate_compiler_environment(host, Some(&compiler.path)).or_fail()?;
		let definition = if expected_profile == "Debug" {
			"QUEST_FIXTURE_EVALUATED=2"
		} else {
			"QUEST_FIXTURE_EVALUATED=1"
		};
		expect_that!(headers.definitions, contains(eq(definition)));
		expect_that!(
			headers.frontend_flags,
			contains(eq("-DQUEST_FIXTURE_CXX=1"))
		);
		if expected_profile == "Release" {
			if !host.ends_with("-apple-darwin") {
				expect_that!(link.options, contains(eq("-Wl,--enable-new-dtags")));
			}
			expect_that!(link.libraries, contains(eq("m")));
		} else {
			expect_that!(
				link.options
					.iter()
					.any(|option| option == "-Wl,--enable-new-dtags"),
				eq(false)
			);
			expect_that!(
				link.libraries.iter().any(|library| library == "m"),
				eq(false)
			);
		}
		let archive = setup.build_directory.join("libquest_bridge.a");
		expect_that!(archive.is_file(), eq(true));
		let contents = run(Command::new("ar").arg("t").arg(archive)).or_fail()?;
		expect_that!(
			String::from_utf8_lossy(&contents.stdout),
			contains_substring("generated_bridge.cpp.o")
		);
		Ok(())
	}

	#[gtest]
	fn cargo_profiles_and_explicit_cxx_remain_coherent_in_child_processes() -> googletest::Result<()>
	{
		let executable = env::current_exe().or_fail()?;
		let paths = env::var_os("PATH")
			.ok_or_else(|| invalid("test PATH missing"))
			.or_fail()?;
		let compiler = env::split_paths(&paths)
			.map(|directory| directory.join("c++"))
			.find(|candidate| candidate.is_file())
			.ok_or_else(|| invalid("test C++ compiler missing"))
			.or_fail()?;
		let compiler = fs::canonicalize(compiler).or_fail()?;
		for (profile, optimization, debug, expected) in [
			("debug", "0", "true", "Debug"),
			("release", "3", "false", "Release"),
		] {
			let out = tempfile::tempdir().or_fail()?;
			let output = run(Command::new(&executable)
				.args([
					"--exact",
					"probe::tests::cmake_build_evaluates_target_requirements_and_paths_with_spaces",
					"--nocapture",
				])
				.env("OUT_DIR", out.path())
				.env("PROFILE", profile)
				.env("OPT_LEVEL", optimization)
				.env("DEBUG", debug)
				.env("CXX", &compiler)
				.env("QUEST_BUILD_FIXTURE_PROFILE", expected))
			.or_fail()?;
			expect_that!(
				String::from_utf8_lossy(&output.stdout),
				contains_substring("1 passed")
			);
		}
		Ok(())
	}

	fn fixture_host() -> googletest::Result<String> {
		let output = run(Command::new("rustc").arg("-vV")).or_fail()?;
		String::from_utf8_lossy(&output.stdout)
			.lines()
			.find_map(|line| line.strip_prefix("host: "))
			.map(str::to_owned)
			.ok_or_else(|| invalid("missing test host"))
			.or_fail()
	}

	#[gtest]
	fn missing_imported_dependency_returns_typed_failure_without_an_archive()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir().or_fail()?;
		let prefix = fixture.path().join("package");
		fixture_package(&prefix)?;
		let package = prefix.join("lib/cmake/QuEST/QuESTConfig.cmake");
		let source = fs::read_to_string(&package).or_fail()?;
		fs::write(
			&package,
			source.replace("$<$<CONFIG:Release>:m>", "Missing::Dependency"),
		)
		.or_fail()?;
		let work = fixture.path().join("build");
		let host = fixture_host()?;
		let result = configure(&work, &host, &host, Some(&prefix), None, &[]);
		expect_that!(matches!(result, Err(BuildError::CmakeBuild(_))), eq(true));
		expect_that!(work.join("build/libquest_bridge.a").exists(), eq(false));
		Ok(())
	}

	#[gtest]
	fn compiled_abi_guard_rejects_incompatible_precision_before_archiving() -> googletest::Result<()>
	{
		let fixture = tempfile::tempdir().or_fail()?;
		let prefix = fixture.path().join("package");
		fixture_package(&prefix)?;
		let header = prefix.join("include/quest/include/config.h");
		let source = fs::read_to_string(&header).or_fail()?;
		fs::write(
			&header,
			source.replace("QUEST_FLOAT_PRECISION 2", "QUEST_FLOAT_PRECISION 1"),
		)
		.or_fail()?;
		let work = fixture.path().join("build");
		let host = fixture_host()?;
		let result = configure(&work, &host, &host, Some(&prefix), None, &[]);
		expect_that!(matches!(result, Err(BuildError::CmakeBuild(_))), eq(true));
		expect_that!(work.join("build/libquest_bridge.a").exists(), eq(false));
		Ok(())
	}

	#[gtest]
	fn rejects_an_unreferenced_shadow_file_in_an_earlier_search_directory() -> googletest::Result<()>
	{
		let fixture = tempfile::tempdir().or_fail()?;
		let earlier = fixture.path().join("earlier");
		let intended = fixture.path().join("intended");
		fs::create_dir_all(&earlier).or_fail()?;
		fs::create_dir_all(&intended).or_fail()?;
		fs::write(earlier.join("libQuEST.so"), "quest").or_fail()?;
		fs::write(earlier.join("libdependency.so"), "stale dependency").or_fail()?;
		let dependency = intended.join("libdependency.so");
		fs::write(&dependency, "intended dependency").or_fail()?;
		let mut link = NativeLink::default();
		record_linked_file(
			cmake_path(&earlier.join("libQuEST.so")).or_fail()?,
			&mut link,
		)
		.or_fail()?;
		record_linked_file(cmake_path(&dependency).or_fail()?, &mut link).or_fail()?;
		expect_that!(
			crate::package::validate_library_resolution(
				&link.search_dirs,
				&link.library_files_by_name
			)
			.is_err(),
			eq(true)
		);
		Ok(())
	}

	#[gtest]
	fn rejects_distinct_absolute_libraries_with_the_same_basename() -> googletest::Result<()> {
		let directory = tempfile::tempdir().or_fail()?;
		let mut link = NativeLink::default();
		for child in ["first", "second"] {
			let folder = directory.path().join(child);
			fs::create_dir_all(&folder).or_fail()?;
			fs::write(folder.join("libsame.so"), child).or_fail()?;
		}
		let first = directory.path().join("first/libsame.so");
		let second = directory.path().join("second/libsame.so");
		record_linked_file(cmake_path(&first).or_fail()?, &mut link).or_fail()?;
		let result = record_linked_file(cmake_path(&second).or_fail()?, &mut link);
		expect_that!(result.is_err(), eq(true));
		Ok(())
	}

	#[cfg(unix)]
	#[gtest]
	fn watches_library_and_header_symlink_lookups_as_well_as_targets() -> googletest::Result<()> {
		let directory = tempfile::tempdir().or_fail()?;
		let native = directory.path().join("libnative.so.7");
		let alias = directory.path().join("libnative.so");
		fs::write(&native, "native").or_fail()?;
		std::os::unix::fs::symlink(&native, &alias).or_fail()?;
		let mut link = NativeLink::default();
		record_linked_file(cmake_path(&alias).or_fail()?, &mut link).or_fail()?;
		expect_that!(&link.linked_files, contains(eq(&alias)));
		expect_that!(
			&link.linked_files,
			contains(eq(&native.canonicalize().or_fail()?))
		);
		let headers = directory.path().join("include");
		let target_headers = directory.path().join("actual headers");
		fs::create_dir_all(&target_headers).or_fail()?;
		let header = target_headers.join("quest.h");
		fs::write(&header, "header").or_fail()?;
		std::os::unix::fs::symlink(&target_headers, &headers).or_fail()?;
		let header_alias = target_headers.join("alias.h");
		std::os::unix::fs::symlink(&header, &header_alias).or_fail()?;
		let mut watched = BTreeSet::new();
		collect_files(&headers, &mut watched, &mut BTreeSet::new()).or_fail()?;
		for path in [
			&header.canonicalize().or_fail()?,
			&headers.join("quest.h"),
			&headers.join("alias.h"),
		] {
			expect_that!(&watched, contains(eq(path)));
		}
		Ok(())
	}

	#[gtest]
	fn paired_mpi_paths_and_library_order_survive_cargo_translation() -> googletest::Result<()> {
		let mut link = NativeLink::default();
		let tokens = split_flags(
			"-Wl,-rpath '-Wl,/opt/mpi lib' -Wl,--enable-new-dtags -lfirst -lsecond -lfirst",
		)
		.or_fail()?;
		record_link_tokens(&tokens, &mut link).or_fail()?;
		expect_that!(
			link.options,
			elements_are![eq("-Wl,-rpath,/opt/mpi lib"), eq("-Wl,--enable-new-dtags")]
		);
		expect_that!(
			link.libraries,
			elements_are![eq("first"), eq("second"), eq("first")]
		);
		Ok(())
	}

	#[cfg(target_os = "linux")]
	#[gtest]
	fn whole_archive_scope_obeys_shared_first_search_in_each_directory() -> googletest::Result<()> {
		let root = tempfile::tempdir()?;
		let shared = root.path().join("libhugetlbfs.so");
		let archive = root.path().join("libhugetlbfs.a");
		let source = root.path().join("fixture.c");
		fs::write(&source, "int fixture(void) { return 1; }\n")?;
		run(Command::new("cc")
			.args(["-shared", "-fPIC"])
			.arg(&source)
			.arg("-o")
			.arg(&shared))?;
		run(Command::new("ar").arg("rcs").arg(&archive))?;
		let mut link = NativeLink::default();
		record_link_tokens(
			&[
				format!("-L{}", root.path().display()),
				"-Wl,--whole-archive,-lhugetlbfs,--no-whole-archive".into(),
			],
			&mut link,
		)?;
		expect_eq!(link.libraries, vec!["dylib:+verbatim=libhugetlbfs.so"]);
		fs::remove_file(shared)?;
		let mut link = NativeLink::default();
		record_link_tokens(
			&[
				format!("-L{}", root.path().display()),
				"-Wl,--whole-archive,-lhugetlbfs,--no-whole-archive".into(),
			],
			&mut link,
		)?;
		expect_eq!(
			link.libraries,
			vec!["static:-bundle,+whole-archive,+verbatim=libhugetlbfs.a"]
		);
		Ok(())
	}

	#[gtest]
	fn rejects_unevaluated_expressions_and_order_sensitive_link_state() -> googletest::Result<()> {
		for text in [
			"-Wl,--start-group",
			"-Wl,--end-group",
			"-Wl,--whole-archive",
			"-Wl,--as-needed",
			"-Wl,--no-as-needed",
			"-Wl,-Bstatic",
			"-Bdynamic",
			"-Tcustom.ld",
			"$<LINK_ONLY:foo>",
			"-Wl,-rpath",
			"-Wl,-rpath -Wl,/opt/mpi,--as-needed",
		] {
			let mut link = NativeLink::default();
			let tokens = split_flags(text).or_fail()?;
			expect_that!(record_link_tokens(&tokens, &mut link).is_err(), eq(true));
		}
		Ok(())
	}

	#[gtest]
	fn bridge_paths_preserve_spaces_and_literal_cmake_variable_syntax() -> googletest::Result<()> {
		let inputs = BridgeInputs {
			sources: vec![PathBuf::from("/opt/path with spaces/${literal}/bridge.cpp")],
			include_directories: vec![PathBuf::from("/opt/headers]")],
		};
		let file = bridge_input_file(Some(&inputs)).or_fail()?;
		expect_that!(
			file,
			contains_substring("[[/opt/path with spaces/${literal}/bridge.cpp]]")
		);
		for path in ["/opt/foo;bar", "/opt/$<CONFIG>", "/opt/foo\nbar"] {
			expect_that!(cmake_path(Path::new(path)).is_err(), eq(true));
		}
		Ok(())
	}

	#[gtest]
	fn explicit_shared_files_keep_exact_filename_and_direct_runtime_directory()
	-> googletest::Result<()> {
		let directory = tempfile::tempdir().or_fail()?;
		let shared = directory.path().join("libfixture.so.7");
		let static_archive = directory.path().join("libstatic.a");
		fs::write(&shared, "fixture").or_fail()?;
		fs::write(&static_archive, "fixture").or_fail()?;
		let mut link = NativeLink::default();
		record_linked_file(cmake_path(&shared).or_fail()?, &mut link).or_fail()?;
		record_linked_file(cmake_path(&static_archive).or_fail()?, &mut link).or_fail()?;
		expect_that!(
			link.libraries,
			elements_are![
				eq("dylib:+verbatim=libfixture.so.7"),
				eq("static:+verbatim=libstatic.a")
			]
		);
		expect_that!(
			link.runtime_dirs,
			elements_are![eq(&directory.path().canonicalize().or_fail()?)]
		);
		Ok(())
	}
	#[gtest]
	fn darwin_dylibs_preserve_versioned_filename() -> googletest::Result<()> {
		let directory = tempfile::tempdir()?;
		for name in ["libQuEST.dylib", "libomp.5.dylib"] {
			let library = directory.path().join(name);
			fs::write(&library, "fixture")?;
			let mut link = NativeLink {
				target_os: Some("macos"),
				..NativeLink::default()
			};
			record_linked_file(cmake_path(&library)?, &mut link)?;
			expect_eq!(
				link.libraries,
				vec![format!(
					"dylib={}",
					name.strip_prefix("lib")
						.unwrap()
						.strip_suffix(".dylib")
						.unwrap()
				)]
			);
			expect_eq!(link.runtime_dirs, vec![directory.path().canonicalize()?]);
		}
		Ok(())
	}

	#[gtest]
	fn darwin_framework_pairs_preserve_order_and_reject_state() -> googletest::Result<()> {
		let mut link = NativeLink {
			target_os: Some("macos"),
			..NativeLink::default()
		};
		record_link_tokens(
			&split_flags(
				"-F '/SDK/System/Library/Frameworks' -framework Accelerate -lomp -framework Foundation",
			)?,
			&mut link,
		)?;
		expect_eq!(
			link.framework_search_dirs,
			vec![PathBuf::from("/SDK/System/Library/Frameworks")]
		);
		expect_eq!(
			link.libraries,
			vec!["framework=Accelerate", "omp", "framework=Foundation"]
		);
		for flags in [
			"-framework",
			"-framework -lomp",
			"-F",
			"-Frelative",
			"-Wl,-force_load,/tmp/lib.a",
			"-Wl,-all_load",
			"-Wl,--enable-new-dtags",
			"-Wl,-rpath,/a:/b",
		] {
			expect_true!(
				record_link_tokens(
					&split_flags(flags)?,
					&mut NativeLink {
						target_os: Some("macos"),
						..NativeLink::default()
					}
				)
				.is_err()
			);
		}
		Ok(())
	}

	#[gtest]
	fn compiler_target_validation_preserves_native_platform_and_architecture()
	-> googletest::Result<()> {
		for (rust, compiler) in [
			("aarch64-apple-darwin", "arm64-apple-darwin25.0.0"),
			("aarch64-apple-darwin", "aarch64-apple-darwin"),
			("x86_64-apple-darwin", "x86_64-apple-darwin24.6"),
			("x86_64-unknown-linux-gnu", "x86_64-pc-linux-gnu"),
			("x86_64-unknown-linux-gnu", "x86_64-redhat-linux"),
			("aarch64-unknown-linux-gnu", "aarch64-redhat-linux"),
			("x86_64-unknown-linux-gnu", "x86_64-redhat-linux-gnu"),
		] {
			validate_compiler_target(rust, compiler)?;
		}
		for (rust, compiler) in [
			("aarch64-apple-darwin", "x86_64-apple-darwin"),
			("aarch64-apple-darwin", "aarch64-unknown-linux-gnu"),
			("x86_64-unknown-linux-gnu", "x86_64-apple-darwin"),
			("x86_64-unknown-linux-gnu", "x86_64-linux-musl"),
			("x86_64-unknown-linux-gnu", "x86_64-redhat-linux-musl"),
			("aarch64-unknown-linux-gnu", "x86_64-redhat-linux"),
			("x86_64-unknown-linux-gnu", "aarch64-redhat-linux"),
			("x86_64-unknown-linux-gnu", "x86_64-w64-mingw32"),
		] {
			expect_true!(validate_compiler_target(rust, compiler).is_err());
		}
		Ok(())
	}

	#[gtest]
	fn darwin_sdk_requires_an_absolute_installed_sdk() -> googletest::Result<()> {
		let directory = tempfile::tempdir()?;
		expect_true!(validate_sdk(Path::new("relative.sdk")).is_err());
		expect_true!(validate_sdk(directory.path()).is_err());
		fs::create_dir_all(directory.path().join("usr/include"))?;
		fs::create_dir_all(directory.path().join("System/Library/Frameworks"))?;
		fs::write(directory.path().join("SDKSettings.json"), "{}")?;
		expect_eq!(
			validate_sdk(directory.path())?,
			directory.path().canonicalize()?
		);
		Ok(())
	}

	#[gtest]
	fn darwin_link_architecture_must_match_the_cargo_target() -> googletest::Result<()> {
		let mut link = NativeLink {
			target_os: Some("macos"),
			architecture: Some("arm64"),
			..NativeLink::default()
		};
		record_link_tokens(&split_flags("-arch arm64")?, &mut link)?;
		expect_true!(record_link_tokens(&split_flags("-arch x86_64")?, &mut link).is_err());
		Ok(())
	}
	#[gtest]
	fn libcxx_fallback_is_confined_to_darwin_clang() -> googletest::Result<()> {
		for id in ["Clang", "AppleClang"] {
			expect_eq!(
				select_standard_library("aarch64-apple-darwin", id, &[])?,
				"c++"
			);
		}
		expect_true!(select_standard_library("aarch64-apple-darwin", "GNU", &[]).is_err());
		expect_true!(select_standard_library("x86_64-unknown-linux-gnu", "Clang", &[]).is_err());
		expect_eq!(
			select_standard_library(
				"x86_64-unknown-linux-gnu",
				"GNU",
				&[PathBuf::from("stdc++")]
			)?,
			"stdc++"
		);
		Ok(())
	}
	#[gtest]
	fn darwin_link_sysroot_must_match_the_evaluated_sdk() -> googletest::Result<()> {
		let directory = tempfile::tempdir()?;
		let mut roots = Vec::new();
		for name in ["first.sdk", "second.sdk"] {
			let root = directory.path().join(name);
			fs::create_dir_all(root.join("usr/include"))?;
			fs::create_dir_all(root.join("System/Library/Frameworks"))?;
			fs::write(root.join("SDKSettings.json"), "{}")?;
			roots.push(root.canonicalize()?);
		}
		let mut link = NativeLink {
			target_os: Some("macos"),
			sysroot: Some(roots[0].clone()),
			..NativeLink::default()
		};
		expect_true!(
			record_link_tokens(
				&["-isysroot".into(), roots[1].to_string_lossy().into_owned()],
				&mut link
			)
			.is_err()
		);
		record_link_tokens(
			&["-isysroot".into(), roots[0].to_string_lossy().into_owned()],
			&mut link,
		)?;
		Ok(())
	}
	#[cfg(target_os = "macos")]
	#[gtest]
	fn darwin_exact_dylib_metadata_links_a_real_rust_consumer() -> googletest::Result<()> {
		let directory = tempfile::tempdir()?;
		let root = directory.path().canonicalize()?;
		let source = root.join("native.cpp");
		let dylib = root.join("libnative_fixture.7.dylib");
		fs::write(&source, "extern \"C\" int native_value() { return 73; }")?;
		run(
			Command::new(env::var_os("CXX").unwrap_or_else(|| "c++".into()))
				.arg("-dynamiclib")
				.arg(&source)
				.arg("-o")
				.arg(&dylib),
		)?;
		let rust = root.join("main.rs");
		fs::write(
			&rust,
			include_str!("../../quest-sys/tests/fixtures/linkage/darwin_consumer.rs"),
		)?;
		let mut link = NativeLink {
			target_os: Some("macos"),
			..NativeLink::default()
		};
		record_linked_file(cmake_path(&dylib)?, &mut link)?;
		let executable = root.join("consumer");
		let mut command = Command::new("rustc");
		command.arg(&rust).arg("-o").arg(&executable);
		for dir in &link.search_dirs {
			command.arg("-L").arg(format!("native={}", dir.display()));
		}
		for library in &link.libraries {
			command.arg("-l").arg(library);
		}
		run(&mut command)?;
		run(&mut Command::new(&executable))?;
		Ok(())
	}

	#[gtest]
	fn explicit_pointer_width_flags_must_agree_with_native_target() -> googletest::Result<()> {
		for (target, flag, accepted) in [
			("x86_64-unknown-linux-gnu", "-m64", true),
			("x86_64-unknown-linux-gnu", "-m32", false),
			("i686-unknown-linux-gnu", "-m32", true),
			("i686-unknown-linux-gnu", "-m64", false),
		] {
			let headers = HeaderContext {
				frontend_flags: vec![flag.into()],
				..HeaderContext::default()
			};
			verify_that!(
				validate_header_context(&headers, target).is_ok(),
				eq(accepted)
			)?;
		}
		Ok(())
	}

	#[gtest]
	fn compile_only_target_and_sdk_overrides_cannot_diverge_from_native_context()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let root = fixture.path().canonicalize()?;
		let other = root.join("other.sdk");
		fs::create_dir(&other)?;
		let mut headers = HeaderContext {
			sysroot: Some(root.clone()),
			..HeaderContext::default()
		};
		for flags in [
			"-arch x86_64".to_owned(),
			"-target x86_64-apple-darwin".to_owned(),
			"--target=x86_64-apple-darwin".to_owned(),
			format!("-isysroot {}", other.display()),
			format!("--sysroot={}", other.display()),
		] {
			headers.frontend_flags = split_flags(&flags)?;
			expect_true!(
				validate_header_context(&headers, "aarch64-apple-darwin").is_err(),
				"admitted {flags}"
			);
		}
		for flags in [
			"-arch arm64".to_owned(),
			"-target arm64-apple-darwin25.0.0".to_owned(),
			"--target=aarch64-apple-darwin".to_owned(),
			format!("-isysroot {}", root.display()),
			format!("--sysroot={}", root.display()),
		] {
			headers.frontend_flags = split_flags(&flags)?;
			validate_header_context(&headers, "aarch64-apple-darwin")?;
		}
		Ok(())
	}
	#[cfg(unix)]
	#[gtest]
	fn imported_header_identity_survives_compiler_implicit_include_suppression()
	-> googletest::Result<()> {
		use std::os::unix::fs::PermissionsExt as _;

		if let Some(prefix) = env::var_os("QUEST_IMPLICIT_INCLUDE_CHILD") {
			let prefix = PathBuf::from(prefix).canonicalize()?;
			let work = tempfile::tempdir()?;
			let host = fixture_host()?;
			let result = discover(work.path(), &host, &host, None);
			if env::var_os("QUEST_IMPLICIT_NO_EXPORT").is_some() {
				expect_that!(
					result.unwrap_err().to_string(),
					contains_substring("did not supply installed include/quest.h")
				);
				return Ok(());
			}
			let package = result?;
			expect_eq!(&package.prefix, &prefix);
			expect_that!(
				&package.headers.include_dirs,
				contains(eq(&prefix.join("include")))
			);
			expect_that!(
				&package.headers.implicit_include_dirs,
				contains(eq(&prefix.join("include")))
			);
			return Ok(());
		}
		let fixture = tempfile::tempdir()?;
		let prefix = fixture.path().join("package");
		fixture_package(&prefix)?;
		let compiler = env::var_os("CXX").unwrap_or_else(|| "c++".into());
		let compiler = if Path::new(&compiler).is_absolute() {
			PathBuf::from(compiler)
		} else {
			env::split_paths(&env::var_os("PATH").ok_or_else(|| invalid("missing PATH"))?)
				.map(|dir| dir.join(&compiler))
				.find(|path| path.is_file())
				.ok_or_else(|| invalid("missing compiler"))?
		};
		let wrapper = fixture.path().join("cxx-wrapper");
		let quote = |value: &Path| format!("'{}'", value.to_string_lossy().replace('\'', "'\\''"));
		fs::write(
			&wrapper,
			format!(
				"#!/bin/sh\nexec {} -isystem {} \"$@\"\n",
				quote(&compiler),
				quote(&prefix.join("include").canonicalize()?)
			),
		)?;
		fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755))?;
		let mut command = Command::new(env::current_exe()?);
		command
			.args([
				"--exact",
				"probe::tests::imported_header_identity_survives_compiler_implicit_include_suppression",
				"--nocapture",
			])
			.env("QUEST_IMPLICIT_INCLUDE_CHILD", &prefix)
			.env("QUEST_ROOT", &prefix)
			.env("CXX", &wrapper);
		for key in ["QUEST_DIR", "QuEST_DIR", "QuEST_ROOT"] {
			command.env_remove(key);
		}
		let output = run(&mut command)?;
		expect_that!(
			String::from_utf8_lossy(&output.stdout),
			contains_substring(format!(
				"cargo:rerun-if-changed={}/include/quest.h",
				prefix.canonicalize()?.display()
			))
		);
		let config = prefix.join("lib/cmake/QuEST/QuESTConfig.cmake");
		let source = fs::read_to_string(&config)?;
		fs::write(
			&config,
			source.replace(
				"  INTERFACE_INCLUDE_DIRECTORIES \"${fixture_prefix}/include\"\n",
				"",
			),
		)?;
		command.env("QUEST_IMPLICIT_NO_EXPORT", "1");
		run(&mut command)?;
		Ok(())
	}
}
