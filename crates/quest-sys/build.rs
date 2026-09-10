use std::fs;

use quest_build::{BuildError, Result};

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/generated_api.rs");
    let native = quest_build::discover_from_env()?;
    println!(
        "cargo:warning=Using QuEST {} for {} from {}",
        native.version,
        native.target,
        native.prefix.display()
    );

    let mut builder = cxx_build::bridges(["src/lib.rs", "src/generated_api.rs"]);
    builder
        .cpp(true)
        .std("c++20")
        // CMake's recorded requirements are authoritative for the native ABI.
        // Rust target flags must not silently add C++ code-generation options.
        .inherit_rustflags(false)
        .compiler(&native.compiler)
        .include("src/cxx_bindings/include");
    for directory in &native.include_dirs {
        if native.system_include_dirs.contains(directory) {
            let directory = directory.to_str().ok_or_else(|| {
                BuildError::InvalidConfiguration("system include paths must be UTF-8".into())
            })?;
            builder.flag("-isystem").flag(directory);
        } else {
            builder.include(directory);
        }
    }
    for definition in &native.compile_definitions {
        let (name, value) = definition
            .split_once('=')
            .map_or((definition.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        builder.define(name, value);
    }
    for option in &native.compile_options {
        builder.flag(option);
    }

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
            if !path.is_file() {
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
            if path.extension().is_some_and(|extension| extension == "cpp") {
                builder.file(&path);
            }
        }
    }
    builder
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unknown-pragmas")
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wpedantic")
        .flag_if_supported("-Wconversion")
        .flag_if_supported("-Wextra");
    builder.try_compile("quest-sys-cxx").map_err(|error| {
        BuildError::InvalidConfiguration(format!(
            "CXX bridge compilation failed using {}: {error}",
            native.compiler.display()
        ))
    })?;
    native.emit_cargo_link_metadata()
}
