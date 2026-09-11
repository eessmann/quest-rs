//! Final-target loader support for explicitly selected serial HDF5.
use crate::{BuildError, Result, invalid, run, runtime_link_args};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Emit direct serial-HDF5 RUNPATHs for final executable packages.
///
/// Selection matches hdf5-metno-sys: explicit `HDF5_DIR`, then the `hdf5`
/// pkg-config entry. Other fallback installations require explicit `HDF5_DIR`.
/// Parallel HDF5 is rejected so serial IO cannot load a different MPI ABI into
/// a process whose runtime belongs to rsmpi and `QuEST`.
///
/// # Errors
/// Reports missing headers/libraries, parallel HDF5, unsupported targets or
/// paths that the supported Linux linker interface cannot represent.
pub fn emit_serial_hdf5_runtime_paths() -> Result<()> {
    for key in [
        "HDF5_DIR",
        "HDF5_VERSION",
        "PKG_CONFIG",
        "PKG_CONFIG_PATH",
        "PKG_CONFIG_LIBDIR",
    ] {
        println!("cargo::rerun-if-env-changed={key}");
    }
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
        let include = run(Command::new(&executable).args(["--cflags-only-I", "hdf5"]))?;
        let library = run(Command::new(executable).args(["--libs-only-L", "hdf5"]))?;
        (
            flag_paths(&include.stdout, "-I")?,
            flag_paths(&library.stdout, "-L")?,
        )
    };
    let header = includes.iter().map(|p| p.join("H5pubconf.h")).find(|p| p.is_file())
        .ok_or_else(|| invalid("serial HDF5 headers were not found; set HDF5_DIR to the same prefix used by hdf5-metno"))?;
    check_serial_header(&header)?;
    println!("cargo::rerun-if-changed={}", header.display());
    let mut directories = Vec::new();
    for directory in libraries {
        let library = directory.join("libhdf5.so");
        if library.is_file() {
            println!("cargo::rerun-if-changed={}", library.display());
            directories.push(directory);
        }
    }
    // pkg-config omits standard system search directories. They need no RUNPATH.
    if env::var_os("HDF5_DIR").is_some() && directories.is_empty() {
        return Err(invalid(
            "HDF5_DIR must contain a shared serial libhdf5.so in lib (matching hdf5-metno discovery)",
        ));
    }
    for argument in runtime_link_args(
        &env::var("CARGO_CFG_TARGET_OS").unwrap_or_default(),
        &directories,
    )? {
        println!("cargo::rustc-link-arg={argument}");
    }
    Ok(())
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
}
