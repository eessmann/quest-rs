//! Final-target loader support for installed serial HDF5.
use crate::{BuildError, Result, invalid, run, runtime_link_args};
use std::{
	env, fs,
	path::{Path, PathBuf},
	process::Command,
};

/// Serial HDF5 installation selected with the locked dependency's rules.
#[derive(Clone, Debug)]
pub struct SerialHdf5 {
	pub header: PathBuf,
	pub include_dirs: Vec<PathBuf>,
	pub library_dirs: Vec<PathBuf>,
	pub version: Option<String>,
	pub source: String,
}

const LINUX_DEFAULTS: &[(&str, &str)] = &[
	(
		"/usr/include/hdf5/serial",
		"/usr/lib/x86_64-linux-gnu/hdf5/serial",
	),
	("/usr/include", "/usr/lib/x86_64-linux-gnu"),
	("/usr/include", "/usr/lib64"),
];

/// Discover serial HDF5 without emitting final-executable linker options.
///
/// Header/library runtime agreement is independently checked by `hdf5-metno-sys`
/// during normal builds. `lib64` installations are selected through pkg-config:
/// the locked dependency's explicit `HDF5_DIR` recipe only supports lib/bin.
/// # Errors
/// Returns missing-library/header, parallel-library or unsupported-platform errors.
pub fn discover_serial_hdf5() -> Result<SerialHdf5> {
	discover_with_defaults(LINUX_DEFAULTS)
}

/// Emit direct serial-HDF5 RUNPATHs for final executable packages.
///
/// Selection matches hdf5-metno-sys: explicit `HDF5_DIR`, then the `hdf5`
/// pkg-config entry, then standard Linux installation layouts. Other fallback
/// installations require explicit `HDF5_DIR`.
/// Parallel HDF5 is rejected so serial IO cannot load a different MPI ABI into
/// a process whose runtime belongs to rsmpi and `QuEST`.
///
/// # Errors
/// Reports missing headers/libraries, parallel HDF5, unsupported targets or
/// paths that the supported native linker interface cannot represent.
pub fn emit_serial_hdf5_runtime_paths() -> Result<()> {
	emit_runtime_paths(LINUX_DEFAULTS)
}

fn emit_runtime_paths(linux_defaults: &[(&str, &str)]) -> Result<()> {
	let selected = discover_with_defaults(linux_defaults)?;
	let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| env::consts::OS.into());
	for argument in runtime_link_args(&target_os, &selected.library_dirs)? {
		println!("cargo::rustc-link-arg={argument}");
	}
	Ok(())
}

fn discover_with_defaults(linux_defaults: &[(&str, &str)]) -> Result<SerialHdf5> {
	for key in [
		"HDF5_DIR",
		"HDF5_VERSION",
		"PKG_CONFIG",
		"PKG_CONFIG_PATH",
		"PKG_CONFIG_LIBDIR",
	] {
		println!("cargo::rerun-if-env-changed={key}");
	}
	let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| env::consts::OS.into());
	let (includes, mut libraries) = if let Some(root) = env::var_os("HDF5_DIR") {
		let root = PathBuf::from(root);
		if !root.is_absolute() {
			return Err(invalid("HDF5_DIR must be an absolute installation prefix"));
		}
		(
			vec![root.join("include")],
			vec![root.join("lib"), root.join("bin")],
		)
	} else if target_os == "macos" {
		let root = homebrew_root()?;
		(
			vec![root.join("include")],
			vec![root.join("lib"), root.join("bin")],
		)
	} else {
		pkg_config_or_system(&target_os, linux_defaults)?
	};
	let header = includes
		.iter()
		.find_map(|p| hdf5_header(p))
		.ok_or_else(|| {
			invalid(
				"serial HDF5 headers were not found; set HDF5_DIR to the same prefix used by hdf5-metno",
			)
		})?;
	// Match hdf5-metno-sys finalization: an empty search list is derived from
	// the include directory which actually supplied H5pubconf, not the first
	// pkg-config include (which may belong to a transitive dependency).
	if libraries.is_empty()
		&& let Some(root) = header.parent().and_then(Path::parent)
	{
		libraries.extend([root.join("lib"), root.join("bin")]);
	}
	check_serial_header(&header)?;
	println!("cargo::rerun-if-changed={}", header.display());
	let library_name = match target_os.as_str() {
		"linux" => "libhdf5.so",
		"macos" => "libhdf5.dylib",
		_ => {
			return Err(invalid(
				"serial HDF5 loader support requires Linux or macOS",
			));
		}
	};
	let mut directories = Vec::new();
	for directory in libraries {
		let library = directory.join(library_name);
		if library.is_file() {
			println!("cargo::rerun-if-changed={}", library.display());
			directories.push(directory);
		}
	}
	// An explicit prefix must provide its own shared library in the dependency's layout.
	if env::var_os("HDF5_DIR").is_some() && directories.is_empty() {
		return Err(invalid(format!(
			"HDF5_DIR must contain a shared serial {library_name} in lib/bin, matching hdf5-metno-sys; for lib64 use the installation pkg-config path with HDF5_DIR unset"
		)));
	}
	let text = fs::read_to_string(&header).map_err(|error| io_header(&header, error))?;
	let version = text.lines().find_map(|line| {
		let mut tokens = line.split_whitespace();
		(tokens.next() == Some("#define") && tokens.next() == Some("H5_VERSION"))
			.then(|| tokens.next().map(|v| v.trim_matches('"').to_owned()))
			.flatten()
	});
	let source = if env::var_os("HDF5_DIR").is_some() {
		"HDF5_DIR"
	} else if target_os == "macos" {
		"Homebrew"
	} else {
		"pkg-config or system layout"
	};
	Ok(SerialHdf5 {
		header,
		include_dirs: includes,
		library_dirs: directories,
		version,
		source: source.into(),
	})
}

fn pkg_config_or_system(
	target_os: &str,
	linux_defaults: &[(&str, &str)],
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
	// Match hdf5-metno-sys, including system paths and target-specific
	// pkg-config environment variables. Raw queries can omit standard paths.
	match pkg_config::Config::new()
		.cargo_metadata(false)
		.probe("hdf5")
	{
		Ok(library)
			if library
				.include_paths
				.iter()
				.any(|include| hdf5_header(include).is_some()) =>
		{
			Ok((library.include_paths, library.link_paths))
		}
		probe => {
			// The dependency also falls back after a successful probe without
			// usable headers. Retain the selected fallback's paired library path.
			let layout = linux_defaults.iter().find(|(include, _)| {
				target_os == "linux" && hdf5_header(Path::new(include)).is_some()
			});
			match layout {
				Some((include, library)) => {
					Ok((vec![PathBuf::from(include)], vec![PathBuf::from(library)]))
				}
				None => Err(match probe {
					Err(error) => invalid(format!("HDF5 pkg-config discovery failed: {error}")),
					Ok(_) => invalid(
						"HDF5 pkg-config supplied no usable headers and no system layout was found",
					),
				}),
			}
		}
	}
}

fn io_header(path: &Path, source: std::io::Error) -> BuildError {
	BuildError::Io {
		path: path.into(),
		source,
	}
}

// Mirror hdf5-metno-sys 0.12.4's Homebrew ordering, including version selectors.
// An explicit HDF5_DIR bypasses this, just as it does in that dependency.
fn homebrew_root() -> Result<PathBuf> {
	let selected = env::var("HDF5_VERSION").ok();
	let family = selected
		.as_deref()
		.and_then(|v| v.rsplit_once('.').map(|(prefix, _)| prefix));
	let mut root = None;
	for (name, excluded, fallback) in [
		(
			"hdf5@2.2",
			&["1.8", "1.10", "1.12", "1.14", "2.1"][..],
			false,
		),
		(
			"hdf5@2.1",
			&["1.8", "1.10", "1.12", "1.14", "2.0"][..],
			false,
		),
		("hdf5@2.0", &["1.8", "1.10", "1.12", "1.14"][..], false),
		("hdf5@1.14", &["1.8", "1.10", "1.12"][..], false),
		("hdf5@1.12", &["1.8", "1.10"][..], false),
		("hdf5@1.10", &["1.8"][..], true),
		("hdf5@1.8", &[][..], true),
		("hdf5-mpi", &[][..], true),
		("hdf5", &[][..], true),
	] {
		if (fallback && root.is_some()) || family.is_some_and(|v| excluded.contains(&v)) {
			continue;
		}
		if let Ok(output) = run(Command::new("brew").args(["--prefix", name])) {
			let candidate = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
			if hdf5_header(&candidate.join("include")).is_some() {
				root = Some(candidate);
			}
		}
	}
	root.ok_or_else(|| {
		invalid("serial HDF5 was not found via Homebrew; set HDF5_DIR to its installation prefix")
	})
}

fn hdf5_header(include: &Path) -> Option<PathBuf> {
	["H5pubconf.h", "H5pubconf-64.h"]
		.iter()
		.map(|name| include.join(name))
		.find(|path| path.is_file())
}
fn check_serial_header(header: &Path) -> Result<()> {
	let source = fs::read_to_string(header).map_err(|e| BuildError::Io {
		path: header.to_path_buf(),
		source: e,
	})?;
	for line in source.lines() {
		let mut tokens = line.split_whitespace();
		if tokens.next() == Some("#define")
			&& tokens.next() == Some("H5_HAVE_PARALLEL")
			&& tokens.next() != Some("0")
		{
			return Err(invalid(
				"QSVT root-only IO requires serial HDF5; select a non-MPI HDF5_DIR",
			));
		}
	}
	Ok(())
}
#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	#[cfg(unix)]
	#[gtest]
	fn darwin_homebrew_discovery_matches_dependency_selection() -> googletest::Result<()> {
		use std::os::unix::fs::PermissionsExt;
		if let Some(root) = env::var_os("QUEST_HDF5_BREW_FIXTURE") {
			let selection = discover_serial_hdf5()?;
			verify_that!(
				selection.header,
				eq(&PathBuf::from(root).join("include/H5pubconf.h"))
			)?;
			verify_that!(selection.version.as_deref(), eq(Some("1.14.3")))?;
			verify_that!(selection.source, eq("Homebrew"))?;
			return Ok(());
		}
		let directory = tempfile::tempdir()?;
		let root = directory.path().join("serial hdf5");
		fs::create_dir_all(root.join("include"))?;
		fs::create_dir_all(root.join("lib"))?;
		fs::write(
			root.join("include/H5pubconf.h"),
			r#"#define H5_VERSION "1.14.3"
"#,
		)?;
		fs::write(root.join("lib/libhdf5.dylib"), "fixture")?;
		let brew = directory.path().join("brew");
		fs::write(
			&brew,
			r#"#!/bin/sh
case "$2" in hdf5@1.14) printf '%s\n' "$QUEST_HDF5_BREW_FIXTURE";; *) exit 1;; esac
"#,
		)?;
		fs::set_permissions(&brew, fs::Permissions::from_mode(0o755))?;
		let output = Command::new(env::current_exe()?)
			.args([
				"--exact",
				"hdf5::tests::darwin_homebrew_discovery_matches_dependency_selection",
				"--nocapture",
			])
			.env("QUEST_HDF5_BREW_FIXTURE", &root)
			.env("CARGO_CFG_TARGET_OS", "macos")
			.env("HDF5_VERSION", "1.14.3")
			.env("PATH", directory.path())
			.env_remove("HDF5_DIR")
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
	fn serial_header_admission_excludes_independent_mpi_runtime() -> googletest::Result<()> {
		let directory = tempfile::tempdir()?;
		let header = directory.path().join("H5pubconf.h");
		fs::write(
			&header,
			"/* #undef H5_HAVE_PARALLEL */\n#define H5_HAVE_THREADSAFE 1\n",
		)?;
		check_serial_header(&header)?;
		fs::write(&header, "#define H5_HAVE_PARALLEL 1\n")?;
		expect_true!(check_serial_header(&header).is_err());
		Ok(())
	}
	#[cfg(target_os = "linux")]
	fn pkg_config_child(test: &str, root: &Path) -> std::io::Result<Command> {
		let mut command = Command::new(env::current_exe()?);
		command
			.args(["--exact", test, "--nocapture"])
			.env("QUEST_HDF5_PKG_CONFIG_CHILD", root)
			.env("CARGO_CFG_TARGET_OS", "linux")
			.env("PKG_CONFIG", "pkg-config")
			.env("PKG_CONFIG_LIBDIR", root.join("pkgconfig"))
			.env_remove("HOST")
			.env_remove("TARGET")
			.env_remove("PKG_CONFIG_PATH")
			.env_remove("PKG_CONFIG_SYSROOT_DIR")
			.env_remove("PKG_CONFIG_ALLOW_SYSTEM_CFLAGS")
			.env_remove("PKG_CONFIG_ALLOW_SYSTEM_LIBS")
			.env_remove("HDF5_NO_PKG_CONFIG")
			.env_remove("HDF5_DIR");
		Ok(command)
	}
	#[cfg(target_os = "linux")]
	#[gtest]
	fn linux_pkg_config_keeps_system_header_and_library_paths() -> googletest::Result<()> {
		if let Some(root) = env::var_os("QUEST_HDF5_PKG_CONFIG_CHILD") {
			let root = PathBuf::from(root);
			let selection = discover_with_defaults(&[])?;
			verify_that!(selection.header, eq(&root.join("include/H5pubconf.h")))?;
			verify_that!(selection.library_dirs, eq(&vec![root.join("lib")]))?;
			verify_that!(selection.version.as_deref(), eq(Some("1.14.3")))?;
			return Ok(());
		}
		let directory = tempfile::tempdir()?;
		let root = directory.path().join("system hdf5");
		for path in ["include", "lib", "pkgconfig"] {
			fs::create_dir_all(root.join(path))?;
		}
		fs::write(
			root.join("include/H5pubconf.h"),
			"#define H5_VERSION \"1.14.3\"\n",
		)?;
		fs::write(root.join("lib/libhdf5.so"), "fixture")?;
		fs::write(
			root.join("pkgconfig/hdf5.pc"),
			format!(
				"prefix={}\nName: HDF5\nDescription: system-path fixture\nVersion: 1.14.3\nLibs: -L\"${{prefix}}/lib\" -lhdf5\nCflags: -I\"${{prefix}}/include\"\n",
				root.display()
			),
		)?;
		let output = pkg_config_child(
			"hdf5::tests::linux_pkg_config_keeps_system_header_and_library_paths",
			&root,
		)?
		.env("PKG_CONFIG_SYSTEM_INCLUDE_PATH", root.join("include"))
		.env("PKG_CONFIG_SYSTEM_LIBRARY_PATH", root.join("lib"))
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
	#[cfg(target_os = "linux")]
	#[gtest]
	fn linux_pkg_config_empty_link_paths_use_selected_header_root() -> googletest::Result<()> {
		if let Some(root) = env::var_os("QUEST_HDF5_PKG_CONFIG_CHILD") {
			let root = PathBuf::from(root).join("selected");
			let selection = discover_with_defaults(&[])?;
			verify_that!(selection.header, eq(&root.join("include/H5pubconf-64.h")))?;
			verify_that!(
				selection.library_dirs,
				eq(&vec![root.join("lib"), root.join("bin")])
			)?;
			emit_runtime_paths(&[])?;
			return Ok(());
		}
		let directory = tempfile::tempdir()?;
		let root = directory.path().join("empty link paths");
		for path in [
			"unrelated/include",
			"unrelated/lib",
			"selected/include",
			"selected/lib",
			"selected/bin",
			"pkgconfig",
		] {
			fs::create_dir_all(root.join(path))?;
		}
		fs::write(
			root.join("selected/include/H5pubconf-64.h"),
			"#define H5_VERSION \"1.14.3\"\n",
		)?;
		for path in ["unrelated/lib", "selected/lib", "selected/bin"] {
			fs::write(root.join(path).join("libhdf5.so"), "discovery fixture")?;
		}
		fs::write(
			root.join("pkgconfig/hdf5.pc"),
			format!(
				"prefix={}\nName: HDF5\nDescription: empty library paths fixture\nVersion: 1.14.3\nLibs: -lhdf5\nCflags: -I\"${{prefix}}/unrelated/include\" -I\"${{prefix}}/selected/include\"\n",
				root.display()
			),
		)?;
		let output = pkg_config_child(
			"hdf5::tests::linux_pkg_config_empty_link_paths_use_selected_header_root",
			&root,
		)?
		.output()?;
		verify_that!(output.status.success(), eq(true)).with_failure_message(|| {
			format!(
				"{} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			)
		})?;
		verify_that!(
			String::from_utf8_lossy(&output.stdout),
			contains_substring(format!(
				"-Wl,-rpath,{}:{}",
				root.join("selected/lib").display(),
				root.join("selected/bin").display()
			))
		)?;
		Ok(())
	}
	#[cfg(target_os = "linux")]
	#[gtest]
	fn linux_headerless_pkg_config_uses_matching_default_library() -> googletest::Result<()> {
		if let Some(root) = env::var_os("QUEST_HDF5_PKG_CONFIG_CHILD") {
			let root = PathBuf::from(root);
			let include = root.join("default/include");
			let library = root.join("default/lib");
			let selection = discover_with_defaults(&[(
				include
					.to_str()
					.ok_or_else(|| invalid("non-UTF-8 fixture include"))?,
				library
					.to_str()
					.ok_or_else(|| invalid("non-UTF-8 fixture library"))?,
			)])?;
			verify_that!(selection.header, eq(&include.join("H5pubconf.h")))?;
			verify_that!(selection.library_dirs, eq(&vec![library]))?;
			verify_that!(selection.version.as_deref(), eq(Some("1.14.3")))?;
			return Ok(());
		}
		let directory = tempfile::tempdir()?;
		let root = directory.path();
		for path in [
			"default/include",
			"default/lib",
			"headerless/include",
			"headerless/lib",
			"pkgconfig",
		] {
			fs::create_dir_all(root.join(path))?;
		}
		fs::write(
			root.join("default/include/H5pubconf.h"),
			"#define H5_VERSION \"1.14.3\"\n",
		)?;
		fs::write(root.join("default/lib/libhdf5.so"), "default fixture")?;
		fs::write(root.join("headerless/lib/libhdf5.so"), "unrelated fixture")?;
		fs::write(
			root.join("pkgconfig/hdf5.pc"),
			format!(
				"prefix={}/headerless\nName: HDF5\nDescription: missing-header fixture\nVersion: 1.10.7\nLibs: -L\"${{prefix}}/lib\" -lhdf5\nCflags: -I\"${{prefix}}/include\"\n",
				root.display()
			),
		)?;
		let output = pkg_config_child(
			"hdf5::tests::linux_headerless_pkg_config_uses_matching_default_library",
			root,
		)?
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
	#[cfg(target_os = "linux")]
	#[gtest]
	fn linux_system_hdf5_without_pkg_config_preserves_serial_admission() -> googletest::Result<()> {
		if let Some(root) = env::var_os("QUEST_HDF5_SYSTEM_CHILD") {
			let root = PathBuf::from(root);
			let include = root.join("include");
			let library = root.join("lib64");
			emit_runtime_paths(&[(include.to_str().unwrap(), library.to_str().unwrap())])?;
			return Ok(());
		}
		let directory = tempfile::tempdir()?;
		fs::create_dir_all(directory.path().join("include"))?;
		fs::create_dir_all(directory.path().join("lib64"))?;
		fs::create_dir_all(directory.path().join("pkgconfig"))?;
		fs::write(directory.path().join("lib64/libhdf5.so"), "fixture")?;
		for (header, parallel) in [
			("H5pubconf.h", false),
			("H5pubconf.h", true),
			("H5pubconf-64.h", false),
			("H5pubconf-64.h", true),
		] {
			let header_path = directory.path().join("include").join(header);
			fs::write(
				&header_path,
				if parallel {
					"#define H5_HAVE_PARALLEL 1\n"
				} else {
					"/* serial */\n"
				},
			)?;
			let output = Command::new(env::current_exe()?)
				.args([
					"--exact",
					"hdf5::tests::linux_system_hdf5_without_pkg_config_preserves_serial_admission",
					"--nocapture",
				])
				.env("QUEST_HDF5_SYSTEM_CHILD", directory.path())
				.env("CARGO_CFG_TARGET_OS", "linux")
				.env("PKG_CONFIG", "pkg-config")
				.env("PKG_CONFIG_LIBDIR", directory.path().join("pkgconfig"))
				.env_remove("PKG_CONFIG_PATH")
				.env_remove("PKG_CONFIG_SYSROOT_DIR")
				.env_remove("HDF5_DIR")
				.output()?;
			expect_eq!(
				output.status.success(),
				!parallel,
				"{} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
			if parallel {
				expect_that!(
					String::from_utf8_lossy(&output.stdout),
					contains_substring("requires serial HDF5")
				);
			} else {
				expect_that!(
					String::from_utf8_lossy(&output.stdout),
					contains_substring(header)
				);
			}
			fs::remove_file(header_path)?;
		}
		Ok(())
	}
	#[cfg(target_os = "linux")]
	#[gtest]
	fn linux_hdf5_selection_matches_dependency_layouts() -> googletest::Result<()> {
		if env::var_os("QUEST_HDF5_LINUX_CHILD").is_some() {
			emit_serial_hdf5_runtime_paths()?;
			return Ok(());
		}
		let directory = tempfile::tempdir()?;
		for (layout, explicit, succeeds) in [
			("lib", true, true),
			("bin", true, true),
			("lib64", true, false),
			("lib64", false, true),
		] {
			let root = directory
				.path()
				.join(format!("serial hdf5 {layout} {explicit}"));
			fs::create_dir_all(root.join("include"))?;
			fs::create_dir_all(root.join(layout))?;
			fs::create_dir_all(root.join("pkgconfig"))?;
			fs::write(root.join("include/H5pubconf.h"), "/* serial */")?;
			fs::write(root.join(layout).join("libhdf5.so"), "fixture")?;
			fs::write(
				root.join("pkgconfig/hdf5.pc"),
				format!(
					"prefix={}\nName: HDF5\nDescription: serial layout fixture\nVersion: 1.10.7\nLibs: -L\"${{prefix}}/{layout}\" -lhdf5\nCflags: -I\"${{prefix}}/include\"\n",
					root.display()
				),
			)?;
			let mut command = Command::new(env::current_exe()?);
			command
				.args([
					"--exact",
					"hdf5::tests::linux_hdf5_selection_matches_dependency_layouts",
					"--nocapture",
				])
				.env("QUEST_HDF5_LINUX_CHILD", "1")
				.env("CARGO_CFG_TARGET_OS", "linux")
				.env("PKG_CONFIG", "pkg-config")
				.env("PKG_CONFIG_LIBDIR", root.join("pkgconfig"))
				.env_remove("PKG_CONFIG_PATH")
				.env_remove("PKG_CONFIG_SYSROOT_DIR")
				.env_remove("HDF5_DIR");
			if explicit {
				command.env("HDF5_DIR", &root);
			}
			let output = command.output()?;
			expect_eq!(
				output.status.success(),
				succeeds,
				"layout={layout}, explicit={explicit}: {} {}",
				String::from_utf8_lossy(&output.stdout),
				String::from_utf8_lossy(&output.stderr)
			);
			if succeeds {
				expect_that!(
					String::from_utf8_lossy(&output.stdout),
					contains_substring(format!("-Wl,-rpath,{}/{layout}", root.display()))
				);
			}
		}
		Ok(())
	}
	#[gtest]
	fn darwin_serial_hdf5_emits_a_direct_dylib_runtime_path() -> googletest::Result<()> {
		if env::var_os("QUEST_HDF5_DARWIN_CHILD").is_some() {
			emit_serial_hdf5_runtime_paths()?;
			return Ok(());
		}
		let directory = tempfile::tempdir()?;
		fs::create_dir_all(directory.path().join("include"))?;
		fs::create_dir_all(directory.path().join("lib"))?;
		fs::write(directory.path().join("include/H5pubconf.h"), "/* serial */")?;
		fs::write(directory.path().join("lib/libhdf5.dylib"), "fixture")?;
		let output = Command::new(env::current_exe()?)
			.args([
				"--exact",
				"hdf5::tests::darwin_serial_hdf5_emits_a_direct_dylib_runtime_path",
				"--nocapture",
			])
			.env("QUEST_HDF5_DARWIN_CHILD", "1")
			.env("HDF5_DIR", directory.path())
			.env("CARGO_CFG_TARGET_OS", "macos")
			.output()?;
		expect_true!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stdout)
		);
		expect_that!(
			String::from_utf8_lossy(&output.stdout),
			contains_substring(format!("-Wl,-rpath,{}/lib", directory.path().display()))
		);
		Ok(())
	}
}
