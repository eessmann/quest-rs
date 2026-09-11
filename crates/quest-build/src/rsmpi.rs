//! Check rsmpi's selected compiler against the evaluated `QuEST` target.
use std::{env, fs, path::Path, process::Command};

use crate::{NativePackage, Result, invalid, io, run};

/// Verify the rsmpi `MPICC` selection against the actual `QuEST::QuEST` target.
///
/// Requires an explicit absolute `MPICC` path and rejects higher-priority rsmpi
/// discovery overrides. Compiles and runs independent non-initializing witnesses
/// with that wrapper and with the evaluated `QuEST` target; their MPI library
/// realpaths, version strings, handle/status sizes and constants must match.
///
/// # Errors
/// Returns an error for ambiguous discovery, compilation failure or ABI mismatch.
pub fn verify_rsmpi_compatibility(native: &NativePackage) -> Result<()> {
    println!("cargo:rerun-if-env-changed=MPICH_CC");
    for key in [
        "MPICC",
        "MPI_PKG_CONFIG",
        "CRAY_MPICH_DIR",
        "CFLAGS",
        "CPPFLAGS",
        "BINDGEN_EXTRA_CLANG_ARGS",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        if key != "MPICC" && env::var_os(key).is_some_and(|value| !value.is_empty()) {
            return Err(invalid(format!(
                "unset {key}: the supported rsmpi recipe selects MPI exclusively through MPICC"
            )));
        }
    }
    let requested = env::var_os("MPICC").ok_or_else(|| invalid("quest-sys mpi feature requires an explicit absolute MPICC path matching QuEST; set it before Cargo builds mpi-sys"))?;
    let wrapper = Path::new(&requested);
    if !wrapper.is_absolute() {
        return Err(invalid("MPICC must be an absolute compiler-wrapper path"));
    }
    let wrapper = fs::canonicalize(wrapper).map_err(|error| io(wrapper, error))?;
    println!("cargo:rerun-if-changed={}", wrapper.display());
    // rsmpi's build-probe-mpi uses this exact discovery query. Ensure it
    // succeeded instead of allowing its pkg-config fallback recipe.
    let shown = run(Command::new(&wrapper).arg("-show"))?;
    let shown =
        String::from_utf8(shown.stdout).map_err(|_| invalid("MPICC -show output is not UTF-8"))?;
    let flags =
        shlex::split(&shown).ok_or_else(|| invalid("MPICC -show output is not shell words"))?;
    if !flags.iter().any(|flag| flag.starts_with("-I"))
        || !flags.iter().any(|flag| flag.starts_with("-l"))
    {
        return Err(invalid(
            "MPICC -show must expose -I and -l arguments understood by rsmpi",
        ));
    }
    run(Command::new("cmake")
        .arg("--build")
        .arg(&native.build_directory)
        .args(["--target", "quest_mpi_abi"]))?;
    let reference = run(&mut Command::new(&native.mpi_probe))?;
    let source = native.build_directory.join("rsmpi-abi.c");
    let executable = native.build_directory.join("rsmpi-abi");
    fs::write(&source, include_str!("../native/mpi_abi.c")).map_err(|error| io(&source, error))?;
    let mut compile = Command::new(&wrapper);
    compile.arg(&source).arg("-o").arg(&executable).arg("-ldl");
    // Ensure this witness remains runnable when the wrapper omits RUNPATH.
    for flag in &flags {
        if let Some(directory) = flag.strip_prefix("-L").filter(|path| !path.is_empty()) {
            compile.arg(format!("-Wl,-rpath,{directory}"));
        }
    }
    run(&mut compile)?;
    let selected = run(&mut Command::new(&executable))?;
    let library = compare_witnesses(&reference.stdout, &selected.stdout)?;
    println!("cargo:rustc-env=QUEST_RSMPI_LIBRARY={}", library.display());
    println!(
        "cargo:warning=Verified rsmpi MPICC {} matches QuEST MPI ABI and loaded library",
        wrapper.display()
    );
    Ok(())
}

fn compare_witnesses(reference: &[u8], selected: &[u8]) -> Result<std::path::PathBuf> {
    let parse = |bytes: &[u8]| -> Result<(std::path::PathBuf, String)> {
        let text =
            std::str::from_utf8(bytes).map_err(|_| invalid("MPI witness output is not UTF-8"))?;
        let (path, signature) = text
            .split_once('\n')
            .ok_or_else(|| invalid("MPI witness omitted its library identity"))?;
        let path = Path::new(path);
        let canonical = fs::canonicalize(path).map_err(|error| io(path, error))?;
        if signature.is_empty() {
            return Err(invalid("MPI witness omitted ABI signature"));
        }
        Ok((canonical, signature.to_owned()))
    };
    let expected = parse(reference)?;
    let actual = parse(selected)?;
    if expected != actual {
        return Err(invalid(format!(
            "rsmpi MPICC MPI ABI/library differs from QuEST::QuEST: native {expected:?}; MPICC {actual:?}"
        )));
    }
    Ok(expected.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn mpi_witness_rejects_same_name_different_library_or_layout() -> googletest::Result<()> {
        let fixture = tempfile::tempdir().or_fail()?;
        let first = fixture.path().join("first.so");
        let second = fixture.path().join("second.so");
        fs::write(&first, "a").or_fail()?;
        fs::write(&second, "b").or_fail()?;
        let reference = format!("{}\ncomm=4 fint=4 status=20\nMPICH", first.display());
        compare_witnesses(reference.as_bytes(), reference.as_bytes()).or_fail()?;
        let other = format!("{}\ncomm=4 fint=4 status=20\nMPICH", second.display());
        verify_that!(
            compare_witnesses(reference.as_bytes(), other.as_bytes()).is_err(),
            eq(true)
        )?;
        let other = format!("{}\ncomm=8 fint=4 status=20\nMPICH", first.display());
        verify_that!(
            compare_witnesses(reference.as_bytes(), other.as_bytes()).is_err(),
            eq(true)
        )
    }
}
