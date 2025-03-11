use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
	// 1) Run CMake, which calls find_package(QuEST).
	//    This writes out quest_export.cmake with QuEST info.
	let cmake_output = cmake::Config::new(".")
		.build_target("quest_dummy")
		.build();

	// 2) The cmake crate puts outputs in something like:
	//    <target>/build/quest-sys-<hash>/out/build.
	//    Adjust if your structure differs.
	let exported_file = cmake_output.join("build").join("quest_export.cmake");
	let content = fs::read_to_string(&exported_file)
		.expect("Failed to read quest_export.cmake from CMake output");

	// Parse out the variables we wrote:
	let quest_include_dirs = parse_cmake_variable(&content, "QUEST_INCLUDE_DIRS");
	let quest_link_libs    = parse_cmake_variable(&content, "QUEST_LINK_LIBS");
	let quest_defines      = parse_cmake_variable(&content, "QUEST_COMPILE_DEFINITIONS");

	// Split them on semicolons:
	let include_paths: Vec<&str> = quest_include_dirs
		.split(';')
		.map(|s| s.trim())
		.filter(|s| !s.is_empty())
		.collect();

	let link_flags: Vec<&str> = quest_link_libs
		.split(';')
		.map(|s| s.trim())
		.filter(|s| !s.is_empty())
		.collect();

	// Now gather compile definitions (e.g. "USE_MPI", "QUEST_SOME_FLAG").
	// If QuEST defines them with a value (like "QUEST_SOME_FLAG=1"), you'll see that too.
	let definitions: Vec<&str> = quest_defines
		.split(';')
		.map(|s| s.trim())
		.filter(|s| !s.is_empty())
		.collect();

	// 3) Print out link info to Cargo
	//    We'll interpret the "link_flags" similarly to the previous example
	for flag in &link_flags {
		if flag.starts_with("-l") {
			// e.g. -lpthread
			let libname = &flag[2..];
			println!("cargo:rustc-link-lib={}", libname);
		} else if flag.starts_with("-L") {
			// e.g. -L/some/path
			println!("cargo:rustc-link-search=native={}", &flag[2..]);
		} else if flag.starts_with("-") {
			// Some other compiler/linker option
			println!("cargo:warning=Unhandled QuEST link flag: {}", flag);
		} else if flag.ends_with(".so") || flag.ends_with(".dylib") || flag.ends_with(".dll") || flag.ends_with(".lib") || flag.ends_with(".a") {
			// A direct library path, e.g. /usr/local/libQuEST.so
			let path = PathBuf::from(flag);
			if let Some(parent) = path.parent() {
				println!("cargo:rustc-link-search=native={}", parent.display());
			}
			if let Some(stem) = path.file_stem() {
				// Strip "lib" prefix if present
				let fname = stem.to_string_lossy();
				let libname = fname.strip_prefix("lib").unwrap_or(&fname);
				println!("cargo:rustc-link-lib={}", libname);
			}
		} else {
			// Possibly a library name without -l, e.g. "QuEST"
			println!("cargo:rustc-link-lib={}", flag);
		}
	}

	// 4) Use cxx_build for your bridging code, applying definitions & includes.
	let mut builder = cxx_build::bridge("src/lib.rs");
	builder.file("src/wrapper.cpp")
		.std("c++20");
	
	let quest_inc_dir = PathBuf::from("src/");
	builder.include(quest_inc_dir);
	
	// Add the discovered include dirs:
	for inc in &include_paths {
		builder.include(inc);
	}

	for def in &definitions {
		let flag = format!("-D{}", def);
		builder.flag(&flag);
	}

	// Build the bridging library
	builder.compile("quest-cxx-bridge");
}

fn parse_cmake_variable(content: &str, var: &str) -> String {
	let needle = format!("set({}", var);
	for line in content.lines() {
		let trimmed = line.trim();
		if trimmed.starts_with(&needle) {
			// e.g. set(QUEST_COMPILE_DEFINITIONS "FOO;BAR=1")
			// let's find the content between the first and last double quotes
			let start = trimmed.find('"').unwrap_or(trimmed.len());
			let end = trimmed.rfind('"').unwrap_or(start);
			return trimmed.get((start + 1)..end).unwrap_or("").to_string();
		}
	}
	String::new()
}
