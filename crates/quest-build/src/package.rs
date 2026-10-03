use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use crate::{Result, invalid, io, runtime_link_args};

/// Generated CXX translation units and local wrapper inputs, without compiling.
#[derive(Clone, Debug, Default)]
pub struct BridgeInputs {
	pub sources: Vec<PathBuf>,
	pub include_directories: Vec<PathBuf>,
}

/// `CMake`-evaluated C++ header context for binding generation.
#[derive(Clone, Debug, Default)]
pub struct HeaderContext {
	pub include_dirs: Vec<PathBuf>,
	/// Evaluated compiler implicit system includes (including the C++ library).
	pub implicit_include_dirs: Vec<PathBuf>,
	/// Evaluated Darwin SDK shared by `CMake` and binding generation.
	pub sysroot: Option<PathBuf>,
	pub system_include_dirs: Vec<PathBuf>,
	pub definitions: Vec<String>,
	pub frontend_flags: Vec<String>,
}

/// An installed package and its evaluated consumer requirements.
#[derive(Clone, Debug)]
pub struct NativePackage {
	/// Validated native Cargo target triple.
	pub target: String,
	pub prefix: PathBuf,
	pub version: String,
	pub mpi_enabled: bool,
	pub subcommunicators_enabled: bool,
	pub compiler: PathBuf,
	pub compiler_id: String,
	pub compiler_version: String,
	pub headers: HeaderContext,
	pub library: PathBuf,
	pub link_search_dirs: Vec<PathBuf>,
	pub framework_search_dirs: Vec<PathBuf>,
	pub link_libraries: Vec<String>,
	pub link_options: Vec<String>,
	pub runtime_library_dirs: Vec<PathBuf>,
	pub(crate) bridge_archive: Option<PathBuf>,
	pub(crate) build_directory: PathBuf,
	pub(crate) mpi_probe: PathBuf,
	pub(crate) exact_library_files: BTreeMap<String, PathBuf>,
}

impl NativePackage {
	/// Whether the evaluated installed package supports the public MPI adapter.
	#[must_use]
	pub const fn supports_mpi_subcommunicators(&self) -> bool {
		self.mpi_enabled && self.subcommunicators_enabled
	}

	/// Emit a checked native capability cfg for this Cargo package and metadata
	/// for immediate dependents of the `quest-sys` links package. Dependents
	/// must emit their own cfg; Cargo does not propagate dependency cfg values.
	pub fn emit_native_capability_cfg(&self) {
		println!("cargo:rustc-check-cfg=cfg(quest_native_mpi)");
		if self.supports_mpi_subcommunicators() {
			println!("cargo:rustc-cfg=quest_native_mpi");
		}
		println!("cargo::metadata=mpi_enabled={}", u8::from(self.mpi_enabled));
		println!(
			"cargo::metadata=subcommunicators_enabled={}",
			u8::from(self.subcommunicators_enabled)
		);
	}

	/// Emit the `CMake` bridge archive and ordered native library requirements.
	///
	/// # Errors
	/// Returns an error for paths or options that Cargo cannot represent.
	pub fn emit_cargo_link_metadata(&self) -> Result<()> {
		self.validate_link_search()?;
		if let Some(archive) = &self.bridge_archive {
			let parent = archive
				.parent()
				.ok_or_else(|| invalid("bridge archive has no parent"))?;
			println!("cargo:rustc-link-search=native={}", parent.display());
			println!("cargo:rustc-link-lib=static=quest_bridge");
		}
		for directory in &self.link_search_dirs {
			println!("cargo:rustc-link-search=native={}", directory.display());
		}
		for directory in &self.framework_search_dirs {
			println!("cargo:rustc-link-search=framework={}", directory.display());
		}
		for library in &self.link_libraries {
			println!("cargo:rustc-link-lib={library}");
		}
		self.emit_runtime_paths()
	}

	pub(crate) fn validate_link_search(&self) -> Result<()> {
		let directories = self
			.bridge_archive
			.as_ref()
			.and_then(|archive| archive.parent())
			.map(std::path::Path::to_owned)
			.into_iter()
			.chain(self.link_search_dirs.iter().cloned())
			.collect::<Vec<_>>();
		validate_library_resolution(&directories, &self.exact_library_files)
	}

	/// Emit the non-library arguments needed by this final executable package.
	///
	/// # Errors
	/// Returns an error when a runtime directory cannot be safely represented.
	pub fn emit_runtime_paths(&self) -> Result<()> {
		for option in runtime_options(&self.target, &self.link_options, &self.runtime_library_dirs)?
		{
			println!("cargo:rustc-link-arg={option}");
		}
		Ok(())
	}
}

fn runtime_options(
	target: &str,
	link_options: &[String],
	directories: &[PathBuf],
) -> Result<Vec<String>> {
	let target_os = if target.ends_with("-apple-darwin") {
		"macos"
	} else {
		"linux"
	};
	let mut options = link_options.to_vec();
	options.extend(runtime_link_args(target_os, directories)?);
	if target_os == "macos" {
		let mut seen_rpaths = BTreeSet::new();
		options.retain(|option| {
			!option.starts_with("-Wl,-rpath,") || seen_rpaths.insert(option.clone())
		});
	}
	Ok(options)
}

pub fn validate_library_resolution(
	directories: &[PathBuf],
	files: &BTreeMap<String, PathBuf>,
) -> Result<()> {
	for (name, expected) in files {
		let first = directories
			.iter()
			.flat_map(|directory| {
				// ld64 prefers a text stub to the dylib in the same directory.
				// A matching earlier archive also shadows a later dylib.
				name.strip_suffix(".dylib").map_or_else(
					|| vec![directory.join(name)],
					|stem| {
						vec![
							directory.join(format!("{stem}.tbd")),
							directory.join(name),
							directory.join(format!("{stem}.a")),
						]
					},
				)
			})
			.find(|candidate| candidate.is_file())
			.ok_or_else(|| {
				invalid(format!(
					"native library {name} is absent from the emitted link search paths"
				))
			})?;
		let resolved = fs::canonicalize(&first).map_err(|error| io(&first, error))?;
		if &resolved != expected {
			return Err(invalid(format!(
				"native library {name} is shadowed by {}; Cargo's global link search paths would not select {}",
				first.display(),
				expected.display()
			)));
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;

	#[gtest]
	fn bridge_archive_directory_cannot_shadow_an_exact_native_library() -> googletest::Result<()> {
		let fixture = tempfile::tempdir().or_fail()?;
		let bridge = fixture.path().join("bridge");
		let native = fixture.path().join("native");
		fs::create_dir_all(&bridge).or_fail()?;
		fs::create_dir_all(&native).or_fail()?;
		fs::write(bridge.join("libsame.so"), "shadow").or_fail()?;
		let library = native.join("libsame.so");
		fs::write(&library, "selected").or_fail()?;
		let mut package = NativePackage {
			target: "x86_64-unknown-linux-gnu".into(),
			prefix: fixture.path().to_owned(),
			version: "4.3.9".into(),
			mpi_enabled: false,
			subcommunicators_enabled: false,
			compiler: PathBuf::new(),
			compiler_id: String::new(),
			compiler_version: String::new(),
			headers: HeaderContext::default(),
			library: library.clone(),
			link_search_dirs: vec![native],
			framework_search_dirs: Vec::new(),
			link_libraries: Vec::new(),
			link_options: Vec::new(),
			runtime_library_dirs: Vec::new(),
			bridge_archive: Some(bridge.join("libquest_bridge.a")),
			build_directory: PathBuf::new(),
			mpi_probe: PathBuf::new(),
			exact_library_files: BTreeMap::from([(
				"libsame.so".to_owned(),
				library.canonicalize().or_fail()?,
			)]),
		};
		expect_that!(package.validate_link_search().is_err(), eq(true));
		package.bridge_archive = None;
		package.validate_link_search().or_fail()?;
		Ok(())
	}
	#[gtest]
	fn darwin_archive_in_earlier_search_directory_cannot_shadow_selected_dylib()
	-> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let earlier = fixture.path().join("earlier");
		let selected = fixture.path().join("selected");
		fs::create_dir_all(&earlier)?;
		fs::create_dir_all(&selected)?;
		fs::write(earlier.join("libsame.7.a"), "archive")?;
		let dylib = selected.join("libsame.7.dylib");
		fs::write(&dylib, "selected")?;
		expect_true!(
			validate_library_resolution(
				&[earlier, selected],
				&BTreeMap::from([("libsame.7.dylib".into(), dylib.canonicalize()?)])
			)
			.is_err()
		);
		Ok(())
	}
	#[gtest]
	fn darwin_text_stub_beside_selected_dylib_is_rejected() -> googletest::Result<()> {
		let fixture = tempfile::tempdir()?;
		let dylib = fixture.path().join("libsame.7.dylib");
		fs::write(&dylib, "selected")?;
		fs::write(fixture.path().join("libsame.7.tbd"), "shadow")?;
		expect_true!(
			validate_library_resolution(
				&[fixture.path().to_owned()],
				&BTreeMap::from([("libsame.7.dylib".into(), dylib.canonicalize()?)])
			)
			.is_err()
		);
		Ok(())
	}
	#[gtest]
	fn darwin_runtime_paths_are_unique_across_evaluated_and_direct_options()
	-> googletest::Result<()> {
		let evaluated = vec![
			"-Wl,-rpath,/opt/quest/lib".into(),
			"-pthread".into(),
			"-Wl,-rpath,/opt/other/lib".into(),
			"-pthread".into(),
			"-Wl,-rpath,/opt/quest/lib".into(),
		];
		let directories = vec![
			PathBuf::from("/opt/quest/lib"),
			PathBuf::from("/opt/omp/lib"),
		];
		expect_eq!(
			runtime_options("aarch64-apple-darwin", &evaluated, &directories)?,
			vec![
				"-Wl,-rpath,/opt/quest/lib",
				"-pthread",
				"-Wl,-rpath,/opt/other/lib",
				"-pthread",
				"-Wl,-rpath,/opt/omp/lib"
			]
		);
		let linux = runtime_options("x86_64-unknown-linux-gnu", &evaluated, &directories)?;
		expect_eq!(&linux[..evaluated.len()], evaluated.as_slice());
		expect_eq!(
			&linux[evaluated.len()..],
			&[
				"-Wl,--enable-new-dtags",
				"-Wl,-rpath,/opt/quest/lib:/opt/omp/lib"
			]
		);
		Ok(())
	}
}
