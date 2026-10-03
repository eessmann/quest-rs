//! Final-target loader support for installed serial HDF5.
use crate::{BuildError, Result, invalid, run, runtime_link_args};
use std::{
	env, fs,
	path::{Path, PathBuf},
	process::Command,
};

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
	emit_runtime_paths(&[
		(
			"/usr/include/hdf5/serial",
			"/usr/lib/x86_64-linux-gnu/hdf5/serial",
		),
		("/usr/include", "/usr/lib/x86_64-linux-gnu"),
		("/usr/include", "/usr/lib64"),
	])
}

fn emit_runtime_paths(linux_defaults: &[(&str, &str)]) -> Result<()> {
	for key in [
		"HDF5_DIR",
		"HDF5_VERSION",
		"PKG_CONFIG",
		"PKG_CONFIG_PATH",
		"PKG_CONFIG_LIBDIR",
	] {
		println!("cargo::rerun-if-env-changed={key}");
	}
	let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
	let (includes, libraries) = if let Some(root) = env::var_os("HDF5_DIR") {
		let root = PathBuf::from(root);
		if !root.is_absolute() {
			return Err(invalid("HDF5_DIR must be an absolute installation prefix"));
		}
		(
			vec![root.join("include")],
			vec![root.join("lib"), root.join("bin")],
		)
	} else {
		let executable = env::var_os("PKG_CONFIG").unwrap_or_else(|| "pkg-config".into());
		match run(Command::new(&executable).args(["--cflags-only-I", "hdf5"])) {
			Ok(include) => {
				let library = run(Command::new(executable).args(["--libs-only-L", "hdf5"]))?;
				(
					flag_paths(&include.stdout, "-I")?,
					flag_paths(&library.stdout, "-L")?,
				)
			}
			Err(error) => {
				// Keep the same ordered defaults as locked hdf5-metno-sys.
				// System linker paths need no additional RUNPATH.
				let layout = linux_defaults.iter().find(|(include, _)| {
					target_os == "linux" && hdf5_header(Path::new(include)).is_some()
				});
				let Some((include, library)) = layout else {
					return Err(error);
				};
				(vec![PathBuf::from(include)], vec![PathBuf::from(library)])
			}
		}
	};
	let header = includes
		.iter()
		.find_map(|p| hdf5_header(p))
		.ok_or_else(|| {
			invalid(
				"serial HDF5 headers were not found; set HDF5_DIR to the same prefix used by hdf5-metno",
			)
		})?;
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
	// pkg-config omits standard system search directories. They need no RUNPATH.
	if env::var_os("HDF5_DIR").is_some() && directories.is_empty() {
		return Err(invalid(format!(
			"HDF5_DIR must contain a shared serial {library_name} in lib (matching hdf5-metno discovery)"
		)));
	}
	for argument in runtime_link_args(&target_os, &directories)? {
		println!("cargo::rustc-link-arg={argument}");
	}
	Ok(())
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
fn flag_paths(source: &[u8], prefix: &str) -> Result<Vec<PathBuf>> {
	let text =
		std::str::from_utf8(source).map_err(|_| invalid("HDF5 pkg-config paths must be UTF-8"))?;
	let tokens =
		shlex::split(text).ok_or_else(|| invalid("invalid HDF5 pkg-config path quoting"))?;
	let mut tokens = tokens.iter();
	let mut output = Vec::new();
	while let Some(token) = tokens.next() {
		let Some(value) = token.strip_prefix(prefix) else {
			return Err(invalid("unexpected HDF5 pkg-config path flag"));
		};
		let value = if value.is_empty() {
			tokens
				.next()
				.map(String::as_str)
				.ok_or_else(|| invalid("missing HDF5 pkg-config path"))?
		} else {
			value
		};
		output.push(PathBuf::from(value));
	}
	Ok(output)
}
#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
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
	#[gtest]
	fn package_paths_keep_quoted_spaces_and_reject_unpaired_flags() -> googletest::Result<()> {
		expect_eq!(
			flag_paths(b"-L'/opt/serial hdf5/lib' -L /usr/lib", "-L")?,
			vec![
				PathBuf::from("/opt/serial hdf5/lib"),
				PathBuf::from("/usr/lib")
			]
		);
		expect_true!(flag_paths(b"-L", "-L").is_err());
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
