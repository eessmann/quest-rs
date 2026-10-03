use std::fs;
use std::path::PathBuf;

use quest_build::{BridgeInputs, BuildError, Result};

fn main() -> Result<()> {
	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-changed=src/lib.rs");
	println!("cargo:rerun-if-changed=src/mpi.rs");
	println!("cargo:rerun-if-changed=src/generated_api.rs");

	// Generate the audited CXX boundary; the CMake target owns compilation and
	// all imported QuEST usage requirements.
	let mpi_feature = std::env::var_os("CARGO_FEATURE_MPI").is_some();
	let mpi_enabled =
		mpi_feature && quest_build::discover_from_env()?.supports_mpi_subcommunicators();
	let mut bridges = vec!["src/lib.rs", "src/generated_api.rs"];
	if mpi_enabled {
		bridges.push("src/mpi.rs");
	}
	let generated = cxx_build::bridges(bridges);
	let mut sources: Vec<PathBuf> = generated.get_files().map(PathBuf::from).collect();
	for directory in ["src/cxx_bindings", "src/cxx_bindings/include"] {
		println!("cargo:rerun-if-changed={directory}");
		let entries = fs::read_dir(directory).map_err(|source| BuildError::Io {
			path: directory.into(),
			source,
		})?;
		for entry in entries {
			let path = entry
				.map_err(|source| BuildError::Io {
					path: directory.into(),
					source,
				})?
				.path();
			if path.is_file() {
				println!("cargo:rerun-if-changed={}", path.display());
				if path.extension().is_some_and(|extension| extension == "cpp") {
					sources.push(path);
				}
			}
		}
	}
	sources.sort();
	// CXX documents these generated header roots under OUT_DIR. cc::Build
	// exposes generated source paths, but has no include-directory accessor.
	let output = std::env::var_os("OUT_DIR")
		.map(PathBuf::from)
		.ok_or_else(|| BuildError::InvalidConfiguration("OUT_DIR is missing".into()))?;
	fs::write(
		output.join("quest_rsmpi_config.hpp"),
		format!(
			"#pragma once\n#define QUEST_SYS_RSMPI_ENABLED {}\n",
			u8::from(mpi_enabled)
		),
	)
	.map_err(|source| BuildError::Io {
		path: output.join("quest_rsmpi_config.hpp"),
		source,
	})?;
	let include_directories = vec![
		output.clone(),
		output.join("cxxbridge/include"),
		output.join("cxxbridge/crate"),
		PathBuf::from("src/cxx_bindings/include"),
	];
	let native = quest_build::build_bridge(&BridgeInputs {
		sources,
		include_directories,
	})?;
	println!(
		"cargo:warning=Using QuEST {} from {} with {}",
		native.version,
		native.prefix.display(),
		native.compiler.display()
	);
	native.emit_native_capability_cfg();
	if mpi_feature && mpi_enabled != native.supports_mpi_subcommunicators() {
		return Err(BuildError::InvalidConfiguration(
			"native MPI capabilities changed during bridge construction".into(),
		));
	}
	if mpi_enabled {
		quest_build::verify_rsmpi_compatibility(&native)?;
	}
	native.emit_cargo_link_metadata()
}
