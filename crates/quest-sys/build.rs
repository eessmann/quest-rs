#[path = "build-support/quest.rs"]
mod quest_build_support;

use cmake_package::find_package;
use miette::{IntoDiagnostic, Result};
use std::env;
use std::fs;

fn main() -> miette::Result<()> {
    println!("cargo:rerun-if-env-changed=QUEST_DIR");
    println!("cargo:rerun-if-env-changed=QUEST_ROOT");
    println!("cargo:rerun-if-env-changed=QuEST_DIR");
    println!("cargo:rerun-if-env-changed=QuEST_ROOT");
    println!("cargo:rerun-if-env-changed=CMAKE_PREFIX_PATH");
    println!("cargo:rerun-if-env-changed=CXX");
    println!("cargo:rerun-if-changed=src/lib.rs");

    // Monitor all C++ source files
    for entry in fs::read_dir("src/cxx_bindings").into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    for entry in fs::read_dir("src/cxx_bindings/include").into_diagnostic()? {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    // Find QuEST once and pass explicit prefixes directly to CMake. This lets
    // CMake handle platform layouts such as lib/ and lib64/ itself.
    let mut quest_search = find_package("QuEST");
    let prefix_paths = quest_build_support::configured_prefix_paths();
    if !prefix_paths.is_empty() {
        quest_search = quest_search.prefix_paths(prefix_paths);
    }
    let quest_package = quest_search
        .find()
        .map_err(|_| miette::miette!("{}", quest_discovery_error()))?;
    let quest_target = quest_package.target("QuEST::QuEST").ok_or(miette::miette!(
        "QuEST package does not have a target: QuEST::QuEST"
    ))?;

    // Print some debug info
    println!("cargo:warning=Found target: {}", quest_target.name);

    // Identify OS
    let host_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_windows = host_os == "windows";

    let runtime_library_directories = quest_build_support::runtime_library_directories(
        &host_os,
        quest_target.location.as_deref(),
        &quest_target.link_libraries,
    );
    for directory in &runtime_library_directories {
        println!(
            "cargo:rustc-link-arg={}",
            quest_build_support::rpath_link_arg(directory)
        );
    }
    let encoded_runtime_paths =
        quest_build_support::encode_runtime_library_paths(&runtime_library_directories)
            .into_diagnostic()?;
    let encoded_runtime_paths = encoded_runtime_paths.to_str().ok_or_else(|| {
        miette::miette!("QuEST runtime library paths must be valid UTF-8 for Cargo metadata")
    })?;
    println!("cargo::metadata=runtime_library_paths={encoded_runtime_paths}");

    // Build the C++ bridges
    let mut builder = cxx_build::bridges(["src/lib.rs", "src/generated_api.rs"]);
    builder
        .cpp(true)
        .std("c++20")
        .include("src/cxx_bindings/include")
        .includes(quest_target.include_directories.clone());

    // Add .cpp files in src/cxx_bindings
    let cpp_files = fs::read_dir("src/cxx_bindings")
        .into_diagnostic()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect::<Vec<_>>();
    builder
        .files(&cpp_files)
        .flag_if_supported("-Wno-unused-parameter");

    // Extra warnings for different compilers
    if is_windows {
        builder.flag_if_supported("/EHsc").flag_if_supported("/W4");
    } else {
        builder
            .flag_if_supported("-Wno-unknown-pragmas")
            .flag_if_supported("-Wall")
            .flag_if_supported("-Wpedantic")
            .flag_if_supported("-Wconversion")
            .flag_if_supported("-Wextra")
            .flag_if_supported("-Wno-dollar-in-identifier-extension");
    }

    quest_target.link();

    builder.compile("quest-sys-cxx");
    Ok(())
}

fn quest_discovery_error() -> String {
    "Could not find QuEST package. Set CMAKE_PREFIX_PATH to a QuEST install prefix, or set QUEST_ROOT/QUEST_DIR/QuEST_ROOT/QuEST_DIR to a QuEST tree containing include/quest.h and an installed QuEST CMake package.".to_owned()
}
