use std::env;
use std::path::{Path, PathBuf};

use cmake_package::find_package;

fn main() -> miette::Result<()> {
    println!("cargo:rerun-if-env-changed=QuEST_DIR");
    println!("cargo:rerun-if-env-changed=QuEST_ROOT");
    println!("cargo:rerun-if-env-changed=CMAKE_PREFIX_PATH");

    let host_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_macos = host_os == "macos" || host_os == "darwin";
    if !is_macos {
        return Ok(());
    }

    let quest_package = find_package("QuEST")
        .find()
        .map_err(|_| miette::miette!("Could not find QuEST package"))?;
    let quest_target = quest_package.target("QuEST::QuEST").ok_or(miette::miette!(
        "QuEST package does not have a target: QuEST::QuEST"
    ))?;

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

    Ok(())
}
