//! Verify the MPI recipe selected by the same probe used by `mpi-sys`.
use std::{
	env, fs,
	path::{Path, PathBuf},
	process::Command,
};

use crate::{NativePackage, Result, invalid, io, run};

/// Discovery route selected by the locked `mpi-sys` probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MpiSource {
	/// Explicit pkg-config package or file.
	PkgConfig(String),
	/// Cray programming environment's MPI package.
	CrayPkgConfig(PathBuf),
	/// MPI compiler wrapper, preserving its invocation name.
	CompilerWrapper(PathBuf),
	/// The dependency's MPICH/Open MPI pkg-config fallback.
	FallbackPkgConfig,
}

/// Evaluated MPI discovery, shared by build verification and diagnostics.
#[derive(Clone, Debug)]
pub struct MpiSelection {
	pub source: MpiSource,
	pub wrapper: Option<PathBuf>,
	pub include_dirs: Vec<PathBuf>,
	pub library_dirs: Vec<PathBuf>,
	pub libraries: Vec<String>,
	pub version: String,
}

/// Use the identical locked discovery implementation as `mpi-sys`.
///
/// # Errors
/// Returns discovery errors or a compiler wrapper which cannot expose a valid recipe.
pub fn probe_rsmpi() -> Result<MpiSelection> {
	let library = build_probe_mpi::probe().map_err(|errors| {
		invalid(format!(
			"rsmpi discovery failed: {}",
			errors
				.iter()
				.map(ToString::to_string)
				.collect::<Vec<_>>()
				.join("; ")
		))
	})?;
	let wrapper = library
		.mpicc
		.as_deref()
		.map(|name| verified_wrapper(Path::new(name)))
		.transpose()?;
	if let Some(wrapper) = &wrapper {
		// The upstream probe accepts successful spawning even when -show fails.
		// Never certify that fallback-shaped, empty recipe as usable MPI.
		run(Command::new(wrapper).arg("-show"))?;
		if library.libs.is_empty() || library.include_paths.is_empty() {
			return Err(invalid(
				"MPI compiler -show did not expose libraries and header paths used by mpi-sys",
			));
		}
	}
	let source = discovery_source(&library, wrapper.as_deref());
	Ok(MpiSelection {
		source,
		wrapper,
		include_dirs: library.include_paths,
		library_dirs: library.lib_paths,
		libraries: library.libs,
		version: library.version,
	})
}

fn discovery_source(library: &build_probe_mpi::Library, wrapper: Option<&Path>) -> MpiSource {
	if let Some(package) = env::var_os("MPI_PKG_CONFIG") {
		return MpiSource::PkgConfig(package.to_string_lossy().into_owned());
	}
	if let Some(wrapper) = wrapper {
		return MpiSource::CompilerWrapper(wrapper.into());
	}
	if let Some(root) = env::var_os("CRAY_MPICH_DIR") {
		let file = PathBuf::from(root).join("lib/pkgconfig/mpich.pc");
		// The dependency falls through when a Cray package is unavailable.
		let selected = pkg_config::Config::new()
			.cargo_metadata(false)
			.env_metadata(false)
			.probe(&file.to_string_lossy())
			.is_ok_and(|candidate| {
				let same_search = candidate.link_paths == library.lib_paths;
				candidate.libs == library.libs
					&& same_search
					&& candidate.include_paths == library.include_paths
			});
		if selected {
			return MpiSource::CrayPkgConfig(file);
		}
	}
	MpiSource::FallbackPkgConfig
}

/// Verify rsmpi's selected headers, library and ABI against `QuEST::QuEST`.
///
/// Discovery follows `MPI_PKG_CONFIG`, Cray pkg-config, compiler wrapper and
/// pkg-config fallback exactly as the locked dependency does. Compiler module
/// environment is retained for both independent non-initializing witnesses.
///
/// # Errors
/// Returns discovery, compilation, execution or MPI ABI/library mismatch errors.
pub fn verify_rsmpi_compatibility(native: &NativePackage) -> Result<()> {
	for key in [
		"MPICC",
		"MPICH_CC",
		"MPI_PKG_CONFIG",
		"CRAY_MPICH_DIR",
		"CC",
		"CFLAGS",
		"CPPFLAGS",
		"BINDGEN_EXTRA_CLANG_ARGS",
		"PKG_CONFIG",
		"PKG_CONFIG_PATH",
		"PKG_CONFIG_LIBDIR",
	] {
		println!("cargo:rerun-if-env-changed={key}");
	}
	let selection = probe_rsmpi()?;
	let reference = build_native_mpi_witness(
		&native.cmake,
		&native.build_directory,
		&native.profile,
		&native.mpi_probe,
	)?;
	let source = native.build_directory.join("rsmpi-abi.c");
	let executable = native.build_directory.join("rsmpi-abi");
	fs::write(&source, include_str!("../native/mpi_abi.c")).map_err(|error| io(&source, error))?;
	let mut builder = cc::Build::new();
	builder
		.cargo_metadata(false)
		.host(&native.target)
		.target(&native.target)
		.opt_level(0)
		.debug(false)
		.warnings(false)
		.out_dir(&native.build_directory);
	if let Some(wrapper) = &selection.wrapper {
		builder.compiler(wrapper);
	} else {
		builder.includes(&selection.include_dirs);
	}
	let compiler = builder
		.try_get_compiler()
		.map_err(|error| invalid(format!("rsmpi C compiler: {error}")))?;
	let mut compile = compiler.to_command();
	compile.arg(&source).arg("-o").arg(&executable);
	for directory in &selection.library_dirs {
		compile.arg("-L").arg(directory);
	}
	for library in &selection.libraries {
		compile.arg(format!("-l{library}"));
	}
	// The independent C witness uses platform dynamic-loader support evaluated
	// by the native CMake project, not a hard-coded Unix library name.
	for library in &native.dynamic_loader_libraries {
		compile.arg(format!("-l{library}"));
	}
	let target_os = if native.target.ends_with("-apple-darwin") {
		"macos"
	} else {
		"linux"
	};
	compile.args(crate::runtime_link_args(
		target_os,
		&selection.library_dirs,
	)?);
	run(&mut compile)?;
	let selected = run(&mut Command::new(&executable))?;
	let library = compare_witnesses(&reference.stdout, &selected.stdout)?;
	println!("cargo:rustc-env=QUEST_RSMPI_LIBRARY={}", library.display());
	println!(
		"cargo:warning=Verified rsmpi {:?} matches QuEST MPI ABI and loaded library",
		selection.source
	);
	Ok(())
}

fn build_native_mpi_witness(
	cmake: &Path,
	build_directory: &Path,
	profile: &str,
	executable: &Path,
) -> Result<std::process::Output> {
	run(Command::new(cmake)
		.arg("--build")
		.arg(build_directory)
		.args(["--target", "quest_mpi_abi", "--config", profile]))?;
	run(&mut Command::new(executable))
}

fn verified_wrapper(wrapper: &Path) -> Result<PathBuf> {
	let invocation = if wrapper.is_absolute() {
		wrapper.to_path_buf()
	} else if wrapper.components().count() > 1 {
		env::current_dir()
			.map_err(|error| io(Path::new("."), error))?
			.join(wrapper)
	} else {
		env::split_paths(&env::var_os("PATH").unwrap_or_default())
			.map(|directory| directory.join(wrapper))
			.find(|path| path.is_file())
			.ok_or_else(|| {
				invalid(format!(
					"MPI compiler wrapper {} was not found in PATH",
					wrapper.display()
				))
			})?
	};
	let canonical = fs::canonicalize(&invocation).map_err(|error| io(&invocation, error))?;
	println!("cargo:rerun-if-changed={}", canonical.display());
	println!("cargo:rerun-if-changed={}", invocation.display());
	// Open MPI and Cray wrappers can dispatch by basename. Never execute realpath.
	Ok(invocation)
}

fn compare_witnesses(reference: &[u8], selected: &[u8]) -> Result<std::path::PathBuf> {
	let parse = |bytes: &[u8]| -> Result<(std::path::PathBuf, String)> {
		let text =
			std::str::from_utf8(bytes).map_err(|_| invalid("MPI witness output is not UTF-8"))?;
		let (path, signature) = text
			.split_once('\n')
			.ok_or_else(|| invalid("MPI witness omitted its library identity"))?;
		let path = Path::new(path);
		let canonical = fs::canonicalize(path).map_err(|error| io(path, error))?;
		if signature.is_empty() {
			return Err(invalid("MPI witness omitted ABI signature"));
		}
		Ok((canonical, signature.to_owned()))
	};
	let expected = parse(reference)?;
	let actual = parse(selected)?;
	if expected != actual {
		return Err(invalid(format!(
			"rsmpi MPICC MPI ABI/library differs from QuEST::QuEST: native {expected:?}; MPICC {actual:?}"
		)));
	}
	Ok(expected.0)
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;

	#[gtest]
	fn multi_config_mpi_witness_uses_the_evaluated_profile() -> googletest::Result<()> {
		if !Command::new("ninja")
			.arg("--version")
			.output()
			.is_ok_and(|output| output.status.success())
		{
			eprintln!("skipping multi-config witness regression: Ninja is unavailable");
			return Ok(());
		}
		let fixture = tempfile::tempdir()?;
		let build = fixture.path().join("build");
		fs::write(
			fixture.path().join("CMakeLists.txt"),
			"cmake_minimum_required(VERSION 3.24)\nproject(mpi_witness_profile LANGUAGES CXX)\nadd_executable(quest_mpi_abi witness.cpp)\ntarget_compile_definitions(quest_mpi_abi PRIVATE WITNESS_PROFILE=\"$<CONFIG>\")\n",
		)?;
		fs::write(
			fixture.path().join("witness.cpp"),
			"#include <cstdio>\nint main() { std::puts(WITNESS_PROFILE); }\n",
		)?;
		run(Command::new("cmake")
			.arg("-S")
			.arg(fixture.path())
			.arg("-B")
			.arg(&build)
			.args(["-G", "Ninja Multi-Config", "-DCMAKE_BUILD_TYPE=Release"]))?;
		for profile in ["Release", "Debug", "RelWithDebInfo"] {
			let output = build_native_mpi_witness(
				Path::new("cmake"),
				&build,
				profile,
				&build.join(profile).join("quest_mpi_abi"),
			)?;
			verify_that!(String::from_utf8_lossy(&output.stdout).trim(), eq(profile))?;
		}
		Ok(())
	}

	#[cfg(unix)]
	#[gtest]
	fn mpi_wrapper_preserves_symlink_dispatch_name() -> googletest::Result<()> {
		use std::os::unix::fs::{PermissionsExt, symlink};
		let fixture = tempfile::tempdir()?;
		let executable = fixture.path().join("opal_wrapper");
		let wrapper = fixture.path().join("mpicc");
		fs::write(&executable, "#!/bin/sh\nprintf '%s' \"${0##*/}\"\n")?;
		fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
		symlink(&executable, &wrapper)?;
		let output = run(&mut Command::new(verified_wrapper(&wrapper)?))?;
		verify_that!(output.stdout, eq(b"mpicc"))?;
		Ok(())
	}

	#[gtest]
	fn discovery_uses_dependency_precedence_with_cray_and_explicit_pkg_config()
	-> googletest::Result<()> {
		if env::var_os("QUEST_MPI_PROBE_CHILD").is_some() {
			let expected = env::var("QUEST_MPI_PROBE_CHILD")?;
			let selection = probe_rsmpi()?;
			match expected.as_str() {
				"cray" => verify_that!(
					matches!(selection.source, MpiSource::CrayPkgConfig(_)),
					eq(true)
				)?,
				"explicit" => verify_that!(
					matches!(selection.source, MpiSource::PkgConfig(_)),
					eq(true)
				)?,
				_ => return Err(invalid("unexpected probe fixture").into()),
			}
			verify_that!(selection.libraries, eq(&vec!["mpi_fixture".to_owned()]))?;
			return Ok(());
		}
		let fixture = tempfile::tempdir()?;
		let package_dir = fixture.path().join("lib/pkgconfig");
		fs::create_dir_all(&package_dir)?;
		fs::create_dir_all(fixture.path().join("include"))?;
		let package = package_dir.join("mpich.pc");
		fs::write(
			&package,
			format!(
				"prefix={}\nName: MPI\nDescription: isolated MPI discovery fixture\nVersion: 4.2\nLibs: -L${{prefix}}/lib -lmpi_fixture\nCflags: -I${{prefix}}/include\n",
				fixture.path().display()
			),
		)?;
		for route in ["cray", "explicit"] {
			let mut command = Command::new(env::current_exe()?);
			command
				.args([
					"--exact",
					"rsmpi::tests::discovery_uses_dependency_precedence_with_cray_and_explicit_pkg_config",
					"--nocapture",
				])
				.env("QUEST_MPI_PROBE_CHILD", route)
				.env("CRAY_MPICH_DIR", fixture.path())
				.env("MPICC", "/missing/wrapper-must-not-be-used")
				.env_remove("MPI_PKG_CONFIG")
				.env_remove("PKG_CONFIG_SYSROOT_DIR");
			if route == "explicit" {
				command
					.env("MPI_PKG_CONFIG", &package)
					.env("CRAY_MPICH_DIR", "/missing/cray-must-not-be-used");
			}
			let output = command.output()?;
			verify_that!(output.status.success(), eq(true)).with_failure_message(|| {
				format!(
					"{} {}",
					String::from_utf8_lossy(&output.stdout),
					String::from_utf8_lossy(&output.stderr)
				)
			})?;
		}
		Ok(())
	}

	#[cfg(unix)]
	#[gtest]
	fn relative_wrapper_resolves_from_working_directory() -> googletest::Result<()> {
		use std::os::unix::fs::PermissionsExt;
		if env::var_os("QUEST_MPI_RELATIVE_CHILD").is_some() {
			let wrapper = verified_wrapper(Path::new("./toolchain/mpicc"))?;
			let output = run(&mut Command::new(wrapper))?;
			verify_that!(output.stdout, eq(b"relative-wrapper"))?;
			return Ok(());
		}
		let fixture = tempfile::tempdir()?;
		fs::create_dir(fixture.path().join("toolchain"))?;
		let wrapper = fixture.path().join("toolchain/mpicc");
		fs::write(&wrapper, "#!/bin/sh\nprintf 'relative-wrapper'\n")?;
		fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755))?;
		let output = Command::new(env::current_exe()?)
			.args([
				"--exact",
				"rsmpi::tests::relative_wrapper_resolves_from_working_directory",
				"--nocapture",
			])
			.current_dir(fixture.path())
			.env("QUEST_MPI_RELATIVE_CHILD", "1")
			.env("PATH", "/does-not-contain-the-wrapper")
			.output()?;
		verify_that!(output.status.success(), eq(true)).with_failure_message(|| {
			format!(
				"{} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			)
		})?;
		Ok(())
	}

	#[gtest]
	#[cfg(target_os = "linux")]
	fn compiled_mpi_witness_rejects_request_size_and_alignment_mismatches() -> googletest::Result<()>
	{
		let compiler = env::var_os("MPICC").unwrap_or_else(|| "mpicc".into());
		if !Command::new(&compiler)
			.arg("-show")
			.output()
			.is_ok_and(|output| output.status.success())
		{
			eprintln!("skipping compiled MPI witness regression: MPI wrapper is unavailable");
			return Ok(());
		}
		let fixture = tempfile::tempdir()?;
		for language in ["c", "c++"] {
			let mut witnesses = Vec::new();
			for (name, definition) in [
				("plain", "struct { char bytes[16]; }"),
				("larger", "struct { char bytes[32]; }"),
				("aligned", "union { char bytes[16]; double alignment; }"),
			] {
				let source = fixture.path().join(format!("{language}-{name}.c"));
				let executable = source.with_extension("exe");
				// Keep the real MPI header and loaded library while independently
				// changing just the request type seen by the compiled witness.
				fs::write(
					&source,
					format!(
						"#define _GNU_SOURCE\n#include <mpi.h>\ntypedef {definition} witness_request;\n#define MPI_Request witness_request\n{}",
						include_str!("../native/mpi_abi.c")
					),
				)?;
				run(Command::new(&compiler)
					.args(["-x", language])
					.arg(&source)
					.arg("-ldl")
					.arg("-o")
					.arg(&executable))?;
				witnesses.push(run(&mut Command::new(executable))?.stdout);
			}
			compare_witnesses(&witnesses[0], &witnesses[0])?;
			for other in &witnesses[1..] {
				verify_that!(compare_witnesses(&witnesses[0], other).is_err(), eq(true))
					.with_failure_message(|| {
						format!("{language} witness accepted a different MPI_Request layout")
					})?;
			}
		}
		Ok(())
	}

	#[gtest]
	fn mpi_witness_rejects_same_name_different_library_or_layout() -> googletest::Result<()> {
		let fixture = tempfile::tempdir().or_fail()?;
		let first = fixture.path().join("first.so");
		let second = fixture.path().join("second.so");
		fs::write(&first, "a").or_fail()?;
		fs::write(&second, "b").or_fail()?;
		let reference = format!("{}\ncomm=4 fint=4 status=20\nMPICH", first.display());
		compare_witnesses(reference.as_bytes(), reference.as_bytes()).or_fail()?;
		let other = format!("{}\ncomm=4 fint=4 status=20\nMPICH", second.display());
		verify_that!(
			compare_witnesses(reference.as_bytes(), other.as_bytes()).is_err(),
			eq(true)
		)?;
		let other = format!("{}\ncomm=8 fint=4 status=20\nMPICH", first.display());
		verify_that!(
			compare_witnesses(reference.as_bytes(), other.as_bytes()).is_err(),
			eq(true)
		)
	}
}
