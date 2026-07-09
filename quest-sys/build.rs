// build.rs
use cmake_package::find_package;
use miette::{IntoDiagnostic, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> miette::Result<()> {
    println!("cargo:rerun-if-env-changed=QuEST_DIR");
    println!("cargo:rerun-if-env-changed=QuEST_ROOT");
    println!("cargo:rerun-if-env-changed=CMAKE_PREFIX_PATH");
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

    // Try to find QuEST package via cmake_package
    let quest_package = find_package("QuEST")
        .find()
        .map_err(|_| miette::miette!("Could not find QuEST package"))?;
    let quest_target = quest_package.target("QuEST::QuEST").ok_or(miette::miette!(
        "QuEST package does not have a target: QuEST::QuEST"
    ))?;

    // Print some debug info
    println!("cargo:warning=Found target: {}", quest_target.name);

    // Identify OS
    let host_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_macos = host_os == "macos" || host_os == "darwin";
    let is_windows = host_os == "windows";

    if is_macos {
        let quest_lib_dir = quest_target
            .location
            .as_deref()
            .and_then(|location| Path::new(location).parent())
            .map(Path::to_path_buf)
            .or_else(|| {
                quest_target.link_libraries.iter().find_map(|library| {
                    let path = Path::new(library);
                    path.parent().map(PathBuf::from)
                })
            });

        if let Some(dir) = quest_lib_dir {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
        }
    }

    // Build the C++ bridges
    let mut builder = cxx_build::bridges(["src/lib.rs", "src/generated_api.rs"]);
    builder
        .cpp(true)
        .std("c++20")
        .include("src/cxx_bindings/include")
        .includes(quest_target.include_directories.clone());

    // Optional custom clang++ on macOS
    if is_macos {
        let clangxx = "/opt/homebrew/opt/llvm/bin/clang++";
        if Path::new(clangxx).is_file() {
            println!("cargo:warning=Using custom clang++: {}", clangxx);
            builder.compiler(clangxx);
        }
    }

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
