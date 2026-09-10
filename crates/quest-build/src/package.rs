use std::collections::BTreeMap;
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
    pub system_include_dirs: Vec<PathBuf>,
    pub definitions: Vec<String>,
    pub frontend_flags: Vec<String>,
}

/// An installed package and its evaluated consumer requirements.
#[derive(Clone, Debug)]
pub struct NativePackage {
    pub prefix: PathBuf,
    pub version: String,
    pub compiler: PathBuf,
    pub compiler_id: String,
    pub compiler_version: String,
    pub headers: HeaderContext,
    pub library: PathBuf,
    pub link_search_dirs: Vec<PathBuf>,
    pub link_libraries: Vec<String>,
    pub link_options: Vec<String>,
    pub runtime_library_dirs: Vec<PathBuf>,
    pub(crate) bridge_archive: Option<PathBuf>,
    pub(crate) exact_library_files: BTreeMap<String, PathBuf>,
}

impl NativePackage {
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
        for option in &self.link_options {
            println!("cargo:rustc-link-arg={option}");
        }
        for option in runtime_link_args("linux", &self.runtime_library_dirs)? {
            println!("cargo:rustc-link-arg={option}");
        }
        Ok(())
    }
}

pub fn validate_library_resolution(
    directories: &[PathBuf],
    files: &BTreeMap<String, PathBuf>,
) -> Result<()> {
    for (name, expected) in files {
        let first = directories
            .iter()
            .map(|directory| directory.join(name))
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
            prefix: fixture.path().to_owned(),
            version: "4.3.9".into(),
            compiler: PathBuf::new(),
            compiler_id: String::new(),
            compiler_version: String::new(),
            headers: HeaderContext::default(),
            library: library.clone(),
            link_search_dirs: vec![native],
            link_libraries: Vec::new(),
            link_options: Vec::new(),
            runtime_library_dirs: Vec::new(),
            bridge_archive: Some(bridge.join("libquest_bridge.a")),
            exact_library_files: BTreeMap::from([("libsame.so".to_owned(), library)]),
        };
        expect_that!(package.validate_link_search().is_err(), eq(true));
        package.bridge_archive = None;
        package.validate_link_search().or_fail()?;
        Ok(())
    }
}
