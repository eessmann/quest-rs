use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use serde_json::Value;

fn main() {
	println!("cargo:rerun-if-env-changed=QUEST_DIR");
	println!("cargo:rerun-if-env-changed=QUEST_ROOT");
	println!("cargo:rerun-if-env-changed=CMAKE_PREFIX_PATH");
	println!("cargo:rerun-if-changed=CMakeLists.txt");
	println!("cargo:rerun-if-changed=src/wrapper.cpp");
	println!("cargo:rerun-if-changed=src/bindings.h");

	// Prepare CMake configuration with feature flags
	let mut config = cmake::Config::new(".");
	let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
	let is_windows = target_os == "windows";
	let is_macos = target_os == "macos";

	// On MacOS set compiler to clang if not specified
	if is_macos {
		if env::var("CC").is_err() {
			config.define("CMAKE_C_COMPILER", "clang");
		}
		if env::var("CXX").is_err() {
			config.define("CMAKE_CXX_COMPILER", "clang++");
		}
	}

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

	// Setup CMake file-api query
	let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
	let build_dir = out_dir.join("build");

	// Create the file-api query directory structure
	let file_api_dir = build_dir.join(".cmake").join("api").join("v1");
	let query_dir = file_api_dir.join("query");
	fs::create_dir_all(&query_dir).expect("Failed to create CMake file-api query directory");

	// Create a codemodel query file to request target information
	let query_file = query_dir.join("codemodel-v2");
	fs::write(query_file, "").expect("Failed to create codemodel query file");

	// Run CMake configuration
	let cmake_output = config
		.build_target("quest_dummy")
		.very_verbose(true)
		.build();

	// Process the CMake file-api reply
	let reply_dir = file_api_dir.join("reply");
	if !reply_dir.exists() {
		panic!("CMake file-api reply directory not found. CMake might have failed to run properly.");
	}

	// Find the codemodel file which has a name pattern like "codemodel-v2-xxxxx.json"
	let codemodel_file = fs::read_dir(&reply_dir)
		.expect("Failed to read CMake file-api reply directory")
		.filter_map(Result::ok)
		.find_map(|entry| {
			let path = entry.path();
			let file_raw = entry.file_name();
			let file_name = file_raw.to_string_lossy();

			// The file will either be directly in the reply dir or inside a directory
			if path.is_dir() && file_name.starts_with("codemodel-v2") {
				// If it's a directory, look for the json file inside
				Some(path.join("codemodel-v2.json"))
			} else if path.is_file() && file_name.starts_with("codemodel-v2") && file_name.ends_with(".json") {
				// If it's a file with the right pattern, use it directly
				Some(path)
			} else {
				None
			}
		})
		.unwrap_or_else(|| {
			// If we still can't find it, fallback to index.json which might point to the codemodel
			let index_file = fs::read_dir(&reply_dir)
				.expect("Failed to read CMake file-api reply directory")
				.filter_map(Result::ok)
				.find(|entry| {
					let file_raw = entry.file_name();
					let file_name = file_raw.to_string_lossy();
					file_name.starts_with("index-") && file_name.ends_with(".json")
				});

			if let Some(index_entry) = index_file {
				// Try to parse the index to find the codemodel reference
				if let Ok(index_content) = fs::read_to_string(index_entry.path()) {
					if let Ok(index_data) = serde_json::from_str::<Value>(&index_content) {
						if let Some(objects) = index_data["objects"].as_array() {
							for obj in objects {
								if let Some(kind) = obj["kind"].as_str() {
									if kind == "codemodel" {
										if let Some(jsonFile) = obj["jsonFile"].as_str() {
											return reply_dir.join(jsonFile);
										}
									}
								}
							}
						}
					}
				}
			}

			panic!("Failed to find codemodel file in CMake file-api reply directory. Available files: {:?}",
			       fs::read_dir(&reply_dir)
				       .unwrap_or_else(|_| panic!("Cannot read reply dir"))
				       .filter_map(Result::ok)
				       .map(|e| e.file_name())
				       .collect::<Vec<_>>())
		});

	println!("cargo:warning=Reading codemodel file: {}", codemodel_file.display());
	let codemodel_content = match fs::read_to_string(&codemodel_file) {
		Ok(content) => content,
		Err(e) => {
			println!("cargo:warning=Failed to read codemodel file: {} (error: {})", codemodel_file.display(), e);
			println!("cargo:warning=File exists: {}", codemodel_file.exists());

			// Try to read the first few bytes as a fallback to see if it's a binary file
			if let Ok(bytes) = fs::read(&codemodel_file) {
				let preview = if bytes.len() > 20 {
					format!("{:?}...", &bytes[0..20])
				} else {
					format!("{:?}", bytes)
				};
				println!("cargo:warning=File content preview: {}", preview);
			}

			panic!("Failed to read codemodel file");
		}
	};

	let codemodel_data: Value = match serde_json::from_str(&codemodel_content) {
		Ok(data) => data,
		Err(e) => {
			println!("cargo:warning=Failed to parse codemodel JSON: {}", e);
			// Print the first 100 chars of the file to help debug
			let preview = if codemodel_content.len() > 100 {
				format!("{}...", &codemodel_content[0..100])
			} else {
				codemodel_content.clone()
			};
			println!("cargo:warning=Content preview: {}", preview);
			panic!("Failed to parse codemodel JSON");
		}
	};

	// Find the quest_dummy target
	let target_info = find_quest_dummy_target(&codemodel_data, &reply_dir);

	let mut include_dirs = Vec::new();
	let mut define_map = HashMap::new();
	let mut found_quest = false;

	if let Some(target) = target_info {
		found_quest = process_quest_target(&target, &mut include_dirs, &mut define_map, is_windows);
	}

	if !found_quest {
		println!("cargo:warning=Failed to find quest_dummy target information via CMake. Trying pkg-config fallback...");

		// Fallback to pkg-config
		if let Ok(quest_lib) = pkg_config::Config::new().probe("QuEST") {
			found_quest = true;

			// Process include paths
			for include_path in quest_lib.include_paths {
				let path_str = include_path.to_string_lossy();
				println!("cargo:include={}", path_str);
				include_dirs.push(path_str.to_string());
			}

			// Process libraries
			for lib in quest_lib.libs {
				println!("cargo:rustc-link-lib={}", lib);
			}

			// Process library paths
			for path in quest_lib.link_paths {
				println!("cargo:rustc-link-search=native={}", path.display());
			}

			// Process compile flags
			for flag in quest_lib.defines {
				let (name, value) = flag;
				match value {
					Some(val) => {
						println!("cargo:rustc-cfg=quest_def=\"{}={}\"", name, val);
						define_map.insert(name, val);
					},
					None => {
						println!("cargo:rustc-cfg=quest_def=\"{}\"", name);
						define_map.insert(name, "1".to_string());
					}
				}
			}
		}
	}

	if !found_quest {
		println!("cargo:warning=");
		println!("cargo:warning=============================================================");
		println!("cargo:warning=                     QUEST NOT FOUND                        ");
		println!("cargo:warning=");
		println!("cargo:warning= Please install QuEST and ensure it can be found by:        ");
		println!("cargo:warning= 1. Setting QUEST_DIR or QUEST_ROOT environment variable    ");
		println!("cargo:warning= 2. Installing to a standard system location                ");
		println!("cargo:warning= 3. Using pkg-config                                        ");
		println!("cargo:warning=");
		println!("cargo:warning= For installation instructions, visit:                      ");
		println!("cargo:warning= https://github.com/QuEST-Kit/QuEST                        ");
		println!("cargo:warning=============================================================");
		panic!("Failed to find QuEST installation");
	}

	// Build the cxx bridge
	let mut builder = cxx_build::bridge("src/lib.rs");

	// Add include directories to the cxx bridge
	for inc_dir in &include_dirs {
		builder.include(inc_dir);
	}

	// Add the required QuEST definitions from CMake to the cxx build
	println!("cargo:warning=Adding compile definitions to cxx_build:");

	// Make sure we always have the essential QuEST defines, with fallback values if not found in CMake
	let required_defines = [
		("COMPILE_MPI", "0"),
		("COMPILE_OPENMP", "0"),
		("COMPILE_CUDA", "0"),
		("COMPILE_CUQUANTUM", "0"),
		("FLOAT_PRECISION", "2")
	];

	for (key, default_value) in required_defines {
		let default_flags = default_value.to_string();
		let value = define_map.get(key).unwrap_or(&default_flags);
		println!("cargo:warning=  {}={}", key, value);
		builder.define(key, Some(value.as_str()));
	}

	// Also add any other defines we found
	for (key, value) in &define_map {
		if !["COMPILE_MPI", "COMPILE_OPENMP", "COMPILE_CUDA", "COMPILE_CUQUANTUM", "FLOAT_PRECISION"]
			.contains(&key.as_str()) {
			println!("cargo:warning=  {}={}", key, value);
			builder.define(key.as_str(), Some(value.as_str()));
		}
	}

	// Add the wrapper implementation
	builder
		.file("src/wrapper.cpp")
		.std("c++20")
		.flag_if_supported("-Wno-unused-parameter");

	// Additional flags for different platforms
	if is_windows {
		builder.flag_if_supported("/EHsc");
	} else {
		builder.flag_if_supported("-Wno-unknown-pragmas");
	}

	builder.compile("quest-sys-cxx");
}

fn find_quest_dummy_target(codemodel: &Value, reply_dir: &Path) -> Option<Value> {
	// Extract the targets list
	let targets = codemodel["configurations"][0]["targets"].as_array()?;

	// Find the quest_dummy target
	for target_ref in targets {
		let target_file_name = target_ref["jsonFile"].as_str()?;
		let target_file_path = reply_dir.join(target_file_name);

		// Debug output to help diagnose issues
		println!("cargo:warning=Examining target file: {}", target_file_path.display());

		// Read the target file
		if let Ok(target_content) = fs::read_to_string(&target_file_path) {
			if let Ok(target_data) = serde_json::from_str::<Value>(&target_content) {
				if let Some(name) = target_data["name"].as_str() {
					println!("cargo:warning=Found target with name: {}", name);
					if name == "quest_dummy" {
						println!("cargo:warning=Found quest_dummy target!");
						return Some(target_data);
					}
				}
			} else {
				println!("cargo:warning=Failed to parse target file JSON: {}", target_file_path.display());
			}
		} else {
			println!("cargo:warning=Failed to read target file: {}", target_file_path.display());
		}
	}

	// If we get here, we couldn't find the quest_dummy target. Try to list all files in the reply dir
	println!("cargo:warning=Could not find quest_dummy target. Files in reply dir:");
	if let Ok(entries) = fs::read_dir(reply_dir) {
		for entry in entries.filter_map(Result::ok) {
			println!("cargo:warning=  {}", entry.path().display());
		}
	}

	None
}

fn process_quest_target(target: &Value, include_dirs: &mut Vec<String>, define_map: &mut HashMap<String, String>, is_windows: bool) -> bool {
	let mut found_quest = false;

	println!("cargo:warning=Processing target: {}", target["name"].as_str().unwrap_or("unknown"));

	// Extract include directories
	if let Some(compile_groups) = target["compileGroups"].as_array() {
		for group in compile_groups {
			// First check for defines to identify if this is the right compile group
			let is_quest_group = if let Some(defines) = group["defines"].as_array() {
				defines.iter().any(|def| {
					if let Some(name) = def["define"].as_str() {
						name.contains("QUEST") || name.contains("FLOAT_PRECISION")
					} else {
						false
					}
				})
			} else {
				false
			};

			if is_quest_group {
				println!("cargo:warning=Found compile group with QuEST-related defines");
				found_quest = true;
			}

			// Extract include directories
			if let Some(includes) = group["includes"].as_array() {
				for include in includes {
					if let Some(path) = include["path"].as_str() {
						// If we find QuEST in the include path, mark as found
						if path.contains("QuEST") || path.contains("quest") {
							println!("cargo:warning=Found QuEST in include path: {}", path);
							found_quest = true;
						}

						if !include_dirs.contains(&path.to_string()) {
							include_dirs.push(path.to_string());
							println!("cargo:include={}", path);
							println!("cargo:rustc-link-search=native={}", path);
						}
					}
				}
			}

			// Extract compile definitions
			if let Some(defines) = group["defines"].as_array() {
				for def in defines {
					if let Some(name) = def["define"].as_str() {
						if !name.is_empty() {
							println!("cargo:warning=Found define: {}", name);
							println!("cargo:rustc-cfg=quest_def=\"{}\"", name);

							// Store the definition in our map
							if let Some((key, value)) = parse_define(name) {
								define_map.insert(key, value);
							}
						}
					}
				}
			}
		}
	} else {
		println!("cargo:warning=No compile groups found in target");
	}

	// Extract link libraries and flags
	if let Some(link) = target["link"].as_object() {
		// In newer CMake, libraries are in commandFragments with role="libraries"
		if let Some(command_fragments) = link.get("commandFragments").and_then(|v| v.as_array()) {
			println!("cargo:warning=Found {} command fragments in link section", command_fragments.len());

			for fragment in command_fragments {
				if let (Some(role), Some(frag_str)) = (
					fragment.get("role").and_then(|v| v.as_str()),
					fragment.get("fragment").and_then(|v| v.as_str())
				) {
					println!("cargo:warning=Found fragment with role '{}': {}", role, frag_str);

					if role == "libraries" {
						// This is a library or rpath
						if frag_str.contains("libQuEST") || frag_str.contains("quest") {
							println!("cargo:warning=Found QuEST library in fragment: {}", frag_str);
							found_quest = true;
						}

						if frag_str.starts_with("-Wl,-rpath,") {
							// Extract the rpath
							let rpath = &frag_str[11..]; // Skip "-Wl,-rpath,"
							println!("cargo:warning=Found rpath: {}", rpath);
							println!("cargo:rustc-link-search=native={}", rpath);
						} else if frag_str.ends_with(".dylib") ||
							frag_str.ends_with(".so") ||
							frag_str.ends_with(".dll") ||
							frag_str.ends_with(".a") ||
							frag_str.ends_with(".lib") {
							// This is a direct path to a library file
							process_library_name(frag_str, is_windows);
						} else if !frag_str.is_empty() {
							// Other library-related fragment
							process_library_name(frag_str, is_windows);
						}
					}
				}
			}
		} else {
			// Old-style libraries array
			if let Some(libraries) = link.get("libraries").and_then(|v| v.as_array()) {
				println!("cargo:warning=Found {} libraries in link section", libraries.len());
				for lib in libraries {
					if let Some(lib_name) = lib.get("name").and_then(|v| v.as_str()) {
						println!("cargo:warning=Processing library: {}", lib_name);

						// Look for QuEST in library names
						if lib_name.contains("QuEST") || lib_name.contains("quest") {
							println!("cargo:warning=Found QuEST library: {}", lib_name);
							found_quest = true;
						}

						// Handle different library name formats
						process_library_name(lib_name, is_windows);
					}
				}
			} else {
				println!("cargo:warning=No libraries found in link section");
			}
		}

		// Process library search directories
		if let Some(sysroot_link) = link.get("sysroot").and_then(|v| v.as_object()) {
			if let Some(lib_paths) = sysroot_link.get("libraryDirectories").and_then(|v| v.as_array()) {
				for path in lib_paths {
					if let Some(path_str) = path.get("path").and_then(|v| v.as_str()) {
						println!("cargo:warning=Adding library search path: {}", path_str);
						println!("cargo:rustc-link-search=native={}", path_str);
					}
				}
			}
		}
	} else {
		println!("cargo:warning=No link section found in target");
	}

	// Also check the artifacts for direct links to the QuEST library
	if let Some(artifacts) = target["artifacts"].as_array() {
		for artifact in artifacts {
			if let Some(artifact_path) = artifact["path"].as_str() {
				println!("cargo:warning=Found artifact: {}", artifact_path);

				if artifact_path.contains("QuEST") || artifact_path.contains("quest") {
					found_quest = true;

					// Add the directory containing the artifact to the search path
					if let Some(parent) = Path::new(artifact_path).parent() {
						if parent.as_os_str().len() > 0 {
							println!("cargo:warning=Adding artifact directory to search path: {}", parent.display());
							println!("cargo:rustc-link-search=native={}", parent.display());
						}
					}
				}
			}
		}
	}

	found_quest
}

// Helper function to parse a define string (e.g., "COMPILE_CUDA=0") into key-value pair
fn parse_define(define: &str) -> Option<(String, String)> {
	if let Some(pos) = define.find('=') {
		let (name, value) = define.split_at(pos);
		// value starts with '=', so remove it
		Some((name.to_string(), value[1..].to_string()))
	} else {
		// Defines without values are treated as defined to 1
		Some((define.to_string(), "1".to_string()))
	}
}

fn process_library_name(lib_name: &str, is_windows: bool) {
	if lib_name.is_empty() {
		return;
	}

	println!("cargo:warning=Processing library: {}", lib_name);

	// Handle different library naming patterns
	if lib_name.starts_with("-l") {
		// Standard library flag: -lfoo
		let name = &lib_name[2..];
		println!("cargo:warning=Found -l flag, adding lib: {}", name);
		println!("cargo:rustc-link-lib={}", name);
	} else if lib_name.starts_with("-L") {
		// Standard library path: -L/path/to/libs
		let path = &lib_name[2..];
		println!("cargo:warning=Found -L flag, adding search path: {}", path);
		println!("cargo:rustc-link-search=native={}", path);
	} else if lib_name.contains('/') || lib_name.contains('\\') {
		// This is likely a path to a library
		let path = Path::new(lib_name);

		println!("cargo:warning=Handling library path: {}", path.display());

		// Add the directory to the search path
		if let Some(parent) = path.parent() {
			if parent.as_os_str().len() > 0 {
				println!("cargo:warning=Adding search path: {}", parent.display());
				println!("cargo:rustc-link-search=native={}", parent.display());
			}
		}

		// Extract the library name from the filename
		if let Some(file_stem) = path.file_stem() {
			let name = file_stem.to_string_lossy();

			// Check if the name starts with "lib" on non-Windows
			let lib_name = if !is_windows && name.starts_with("lib") {
				name.strip_prefix("lib").unwrap()
			} else {
				&name
			};

			println!("cargo:warning=Extracted library name: {}", lib_name);

			// Determine if it's a static or dynamic library
			let extension = path.extension().map(|ext| ext.to_string_lossy().to_lowercase());

			if extension.as_deref() == Some("a") || extension.as_deref() == Some("lib") {
				println!("cargo:warning=Adding static library: {}", lib_name);
				println!("cargo:rustc-link-lib=static={}", lib_name);
			} else {
				println!("cargo:warning=Adding dynamic library: {}", lib_name);
				println!("cargo:rustc-link-lib={}", lib_name);
			}
		} else {
			// If we can't extract the name, just use the path as is
			println!("cargo:warning=Could not extract filename, using full path");
			println!("cargo:rustc-link-lib={}", lib_name);
		}
	} else if lib_name.contains('.') && (
		lib_name.contains(".lib") || lib_name.contains(".a") ||
			lib_name.contains(".so") || lib_name.contains(".dylib") ||
			lib_name.contains(".dll")) {
		// This is a library name with extension but no path
		let name = if lib_name.ends_with(".lib") || lib_name.ends_with(".a") {
			let base_name = lib_name.rsplit('.').next().unwrap_or(lib_name);
			println!("cargo:warning=Found static library: {}", base_name);
			println!("cargo:rustc-link-lib=static={}", base_name);
			base_name
		} else {
			let base_name = lib_name.rsplit('.').next().unwrap_or(lib_name);
			println!("cargo:warning=Found dynamic library: {}", base_name);
			println!("cargo:rustc-link-lib={}", base_name);
			base_name
		};

		// If this is QuEST, also try alternative names
		if name.eq_ignore_ascii_case("quest") {
			println!("cargo:warning=Also trying QuEST with different casing");
			println!("cargo:rustc-link-lib=QuEST");
			println!("cargo:rustc-link-lib=quest");
			println!("cargo:rustc-link-lib=QUEST");
		}
	} else {
		// Assume it's a library name (without path or extension)
		println!("cargo:warning=Using as plain library name: {}", lib_name);
		println!("cargo:rustc-link-lib={}", lib_name);

		// If this is QuEST, also try alternative names
		if lib_name.eq_ignore_ascii_case("quest") {
			println!("cargo:warning=Also trying QuEST with different casing");
			println!("cargo:rustc-link-lib=QuEST");
			println!("cargo:rustc-link-lib=quest");
			println!("cargo:rustc-link-lib=QUEST");
		}
	}
}