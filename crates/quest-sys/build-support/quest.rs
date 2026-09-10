use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const QUEST_ENV_VARS: &[&str] = &["QUEST_DIR", "QUEST_ROOT", "QuEST_DIR", "QuEST_ROOT"];

pub fn configured_prefix_paths() -> Vec<PathBuf> {
    let explicit_root = QUEST_ENV_VARS.iter().find_map(|name| {
        env::var_os(name).and_then(|candidate| normalize_quest_root(Path::new(&candidate)))
    });

    collect_prefix_paths(
        explicit_root,
        env::var_os("CMAKE_PREFIX_PATH").as_deref(),
    )
}

pub fn collect_prefix_paths(
    explicit_root: Option<PathBuf>,
    cmake_prefix_path: Option<&OsStr>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(root) = explicit_root {
        paths.push(root);
    }

    if let Some(prefix_path) = cmake_prefix_path {
        for path in env::split_paths(prefix_path) {
            if !path.as_os_str().is_empty() && !paths.contains(&path) {
                paths.push(path);
            }
        }
    }

    paths
}

pub fn normalize_quest_root(candidate: &Path) -> Option<PathBuf> {
    let mut current = candidate;
    loop {
        if current.join("include/quest.h").is_file() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

pub fn runtime_library_directories(
    target_os: &str,
    location: Option<&str>,
    link_libraries: &[String],
) -> Vec<PathBuf> {
    if !matches!(target_os, "linux" | "macos" | "darwin") {
        return Vec::new();
    }

    location
        .into_iter()
        .chain(link_libraries.iter().map(String::as_str))
        .map(Path::new)
        .filter(|path| path.is_absolute() && is_dynamic_library(path, target_os))
        .filter_map(Path::parent)
        .map(Path::to_path_buf)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn encode_runtime_library_paths(
    paths: &[PathBuf],
) -> Result<OsString, env::JoinPathsError> {
    env::join_paths(paths)
}

pub fn rpath_link_arg(directory: &Path) -> String {
    format!("-Wl,-rpath,{}", directory.display())
}

fn is_dynamic_library(path: &Path, target_os: &str) -> bool {
    let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };

    let is_elf_shared = file_name.ends_with(".so") || file_name.contains(".so.");
    match target_os {
        "linux" => is_elf_shared,
        "macos" | "darwin" => is_elf_shared || file_name.ends_with(".dylib"),
        _ => false,
    }
}
