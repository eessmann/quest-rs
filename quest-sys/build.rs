use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
	println!("cargo:rerun-if-env-changed=QUEST_DIR");
	println!("cargo:rerun-if-env-changed=QUEST_ROOT");
	println!("cargo:rerun-if-env-changed=CMAKE_PREFIX_PATH");

	let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
	let is_windows = target_os == "windows";

	// Prepare CMake configuration with feature flags
	let mut config = cmake::Config::new(".");

	// Apply feature flags to CMake
	if cfg!(feature = "openmp") {
		println!("cargo:rustc-cfg=feature=\"openmp\"");
		config.define("ENABLE_MULTITHREADING", "ON");
	}

	if cfg!(feature = "mpi") {
		println!("cargo:rustc-cfg=feature=\"mpi\"");
		config.define("ENABLE_DISTRIBUTION", "ON");
	}

	if cfg!(feature = "cuda") {
		println!("cargo:rustc-cfg=feature=\"cuda\"");
		config.define("ENABLE_CUDA", "ON");

		if cfg!(feature = "cuquantum") {
			println!("cargo:rustc-cfg=feature=\"cuquantum\"");
			config.define("ENABLE_CUQUANTUM", "ON");
		}
	}

	if cfg!(feature = "hip") {
		println!("cargo:rustc-cfg=feature=\"hip\"");
		config.define("ENABLE_HIP", "ON");
	}

	// Set CMAKE_PREFIX_PATH if QUEST_DIR or QUEST_ROOT is specified
	if let Ok(quest_dir) = env::var("QUEST_DIR").or_else(|_| env::var("QUEST_ROOT")) {
		let path = Path::new(&quest_dir);
		if !path.exists() {
			eprintln!("Warning: QUEST_DIR/QUEST_ROOT points to non-existent path: {}", quest_dir);
		}
		config.define("CMAKE_PREFIX_PATH", quest_dir);
	}

	// Run the CMake process to find QuEST and generate quest_export.cmake
	let cmake_output = config
		.build_target("quest_dummy")
		.very_verbose(true)
		.build();

	// Read the generated quest_export.cmake file to get QuEST info
	let exported_file = cmake_output.join("build").join("quest_export.cmake");

	if !exported_file.exists() {
		println!("cargo:warning=Failed to find QuEST installation. Ensure QuEST is installed or set QUEST_DIR/QUEST_ROOT to the QuEST installation directory.");
		panic!("QuEST installation not found. Please install QuEST or set QUEST_DIR/QUEST_ROOT to the QuEST installation directory.");
	}

	let content = std::fs::read_to_string(&exported_file)
		.expect("Failed to read quest_export.cmake");

	// Parse the exported variables
	let quest_include_dirs = parse_cmake_variable(&content, "QUEST_INCLUDE_DIRS");
	let quest_link_libs = parse_cmake_variable(&content, "QUEST_LINK_LIBS");
	let quest_compile_definitions = parse_cmake_variable(&content, "QUEST_COMPILE_DEFINITIONS");

	// Process include directories
	for inc_dir in quest_include_dirs.split(';').filter(|s| !s.is_empty()) {
		println!("cargo:include={}", inc_dir);
	}

	// Process link flags and libraries
	for flag in quest_link_libs.split(';').filter(|s| !s.is_empty()) {
		process_link_flag(flag, is_windows);
	}

	// Process compile definitions
	for def in quest_compile_definitions.split(';').filter(|s| !s.is_empty()) {
		if !def.is_empty() {
			println!("cargo:rustc-cfg=quest_def=\"{}\"", def);
		}
	}

	// Build the cxx bridge
	let mut builder = cxx_build::bridge("src/lib.rs");

	// Add include directories
	for inc_dir in quest_include_dirs.split(';').filter(|s| !s.is_empty()) {
		builder.include(inc_dir);
	}

	// Add compile definitions
	for def in quest_compile_definitions.split(';').filter(|s| !s.is_empty()) {
		builder.flag(&format!("-D{}", def));
	}

	// Add the wrapper implementation
	builder
		.file("src/wrapper.cpp")
		.std("c++20")
		.flag_if_supported("-Wno-unused-parameter")
		.compile("quest-sys-cxx");

	// Output extra debug info if QUEST_DEBUG is set
	if env::var("QUEST_DEBUG").is_ok() {
		println!("cargo:warning=QuEST include dirs: {}", quest_include_dirs);
		println!("cargo:warning=QuEST link libs: {}", quest_link_libs);
		println!("cargo:warning=QuEST compile definitions: {}", quest_compile_definitions);
	}
}

/// Parse a variable from the CMake export file
fn parse_cmake_variable(content: &str, var_name: &str) -> String {
	let prefix = format!("set({}", var_name);

	for line in content.lines() {
		let trimmed = line.trim();
		if trimmed.starts_with(&prefix) {
			// Look for content between quotes
			let start = trimmed.find('"').unwrap_or(trimmed.len());
			let end = trimmed.rfind('"').unwrap_or(start);

			if start < end {
				return trimmed[(start + 1)..end].to_string();
			}
		}
	}

	String::new()
}

/// Process a link flag or library path
fn process_link_flag(flag: &str, is_windows: bool) {
	if flag.is_empty() {
		return;
	}

	if flag.starts_with("-l") {
		// Standard library flag: -lfoo
		let lib_name = &flag[2..];
		println!("cargo:rustc-link-lib={}", lib_name);
	} else if flag.starts_with("-L") {
		// Standard library path: -L/path/to/libs
		let path = &flag[2..];
		println!("cargo:rustc-link-search=native={}", path);
	} else if flag.ends_with(".a") || flag.ends_with(".lib") {
		// Static library path
		let path = Path::new(flag);
		if let Some(parent) = path.parent() {
			println!("cargo:rustc-link-search=native={}", parent.display());
		}

		if let Some(file_stem) = path.file_stem() {
			let name = file_stem.to_string_lossy();
			// Remove 'lib' prefix on Unix-like systems
			let lib_name = if !is_windows && name.starts_with("lib") {
				name.strip_prefix("lib").unwrap()
			} else {
				&name
			};
			println!("cargo:rustc-link-lib=static={}", lib_name);
		}
	} else if flag.ends_with(".so") || flag.ends_with(".dylib") || flag.ends_with(".dll") {
		// Dynamic library path
		let path = Path::new(flag);
		if let Some(parent) = path.parent() {
			println!("cargo:rustc-link-search=native={}", parent.display());
		}

		if let Some(file_stem) = path.file_stem() {
			let name = file_stem.to_string_lossy();
			// Remove 'lib' prefix on Unix-like systems
			let lib_name = if !is_windows && name.starts_with("lib") {
				name.strip_prefix("lib").unwrap()
			} else {
				&name
			};
			println!("cargo:rustc-link-lib={}", lib_name);
		}
	} else if Path::new(flag).exists() {
		// Direct path to a library file
		println!("cargo:warning=Unhandled library path: {}", flag);
	} else {
		// Assume it's a library name
		println!("cargo:rustc-link-lib={}", flag);
	}
}