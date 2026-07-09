use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use clang::diagnostic::Severity;
use clang::{Availability, Clang, Entity, EntityKind, Index};

use super::DynError;
use super::model::{ApiArgument, ApiItem, overload_key};

const QUEST_ENV_VARS: &[&str] = &["QUEST_DIR", "QUEST_ROOT", "QuEST_DIR", "QuEST_ROOT"];

#[derive(Debug, Clone)]
pub struct QuestRoot {
    path: PathBuf,
    source: &'static str,
}

impl QuestRoot {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn source_label(&self) -> &'static str {
        self.source
    }
}

pub fn find_quest_root() -> Result<QuestRoot, DynError> {
    if let Some(root) = quest_root_from_env_vars() {
        return Ok(root);
    }

    if let Some(root) = quest_root_from_cmake_prefix_path() {
        return Ok(root);
    }

    Err(quest_root_error().into())
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

pub fn collect_quest_api(quest_root: &Path) -> Result<Vec<ApiItem>, DynError> {
    let include_root = quest_root.join("include");
    let quest_h = include_root.join("quest.h");

    if !quest_h.is_file() {
        return Err(format!("missing QuEST umbrella header at {}", quest_h.display()).into());
    }

    let include_root = canonicalize_existing(&include_root)?;
    let quest_h = canonicalize_existing(&quest_h)?;
    let _guard = clang_lock()
        .lock()
        .map_err(|_| "libclang mutex was poisoned")?;
    configure_libclang_path();

    let clang = Clang::new().map_err(|error| format_libclang_error(&error))?;
    let index = Index::new(&clang, false, false);
    let args = clang_arguments(&include_root);
    let mut parser = index.parser(&quest_h);
    parser.arguments(&args);

    let translation_unit = parser
        .parse()
        .map_err(|error| format!("libclang failed to parse {}: {error:?}", quest_h.display()))?;

    let diagnostics = translation_unit
        .get_diagnostics()
        .into_iter()
        .filter(|diagnostic| matches!(diagnostic.get_severity(), Severity::Error | Severity::Fatal))
        .map(|diagnostic| format!("{:?}: {}", diagnostic.get_severity(), diagnostic.get_text()))
        .collect::<Vec<_>>();
    if !diagnostics.is_empty() {
        return Err(format!(
            "libclang reported errors while parsing {}:\n{}",
            quest_h.display(),
            diagnostics.join("\n")
        )
        .into());
    }

    let mut items = Vec::new();
    collect_function_decls(
        translation_unit.get_entity(),
        &include_root,
        &mut BTreeSet::new(),
        &mut items,
    );
    items.sort_by(|left, right| {
        left.overload_key
            .cmp(&right.overload_key)
            .then_with(|| left.header.cmp(&right.header))
            .then_with(|| left.line.cmp(&right.line))
    });

    Ok(items)
}

fn clang_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn configure_libclang_path() {
    if env::var_os("LIBCLANG_PATH").is_some() {
        return;
    }

    if let Some(libdir) = llvm_config_path("--libdir").filter(|path| contains_libclang(path)) {
        // The xtask is single-threaded at runtime. Tests serialize libclang access
        // through `clang_lock`, so setting this process variable happens before
        // clang-sys attempts to load libclang.
        unsafe {
            env::set_var("LIBCLANG_PATH", libdir);
        }
    }
}

pub fn format_libclang_error(error: &str) -> String {
    format!(
        "could not load libclang: {error}\n\
         Set LIBCLANG_PATH to a directory containing libclang, make llvm-config available, \
         or install LLVM so the system loader can find libclang."
    )
}

fn clang_arguments(include_root: &Path) -> Vec<String> {
    let mut args = vec![
        "-x".to_owned(),
        "c++".to_owned(),
        "-std=c++20".to_owned(),
        format!("-I{}", include_root.display()),
        format!("-I{}", include_root.join("quest/include").display()),
    ];

    if let Some(resource_dir) = clang_resource_dir() {
        args.push("-resource-dir".to_owned());
        args.push(resource_dir.display().to_string());
        let resource_include = resource_dir.join("include");
        if resource_include.is_dir() {
            args.push("-isystem".to_owned());
            args.push(resource_include.display().to_string());
        }
    }

    for include in discovered_include_dirs() {
        args.push(format!("-I{}", include.display()));
    }

    if let Some(target) = clang_target() {
        args.push("-target".to_owned());
        args.push(target);
    }

    if let Some(sdk_path) = macos_sdk_path() {
        args.push("-isysroot".to_owned());
        args.push(sdk_path.display().to_string());
        let sdk_include = sdk_path.join("usr").join("include");
        if sdk_include.is_dir() {
            args.push("-isystem".to_owned());
            args.push(sdk_include.display().to_string());
        }
        let sdk_frameworks = sdk_path.join("System").join("Library").join("Frameworks");
        if sdk_frameworks.is_dir() {
            args.push("-iframework".to_owned());
            args.push(sdk_frameworks.display().to_string());
        }
    }

    args
}

fn clang_target() -> Option<String> {
    let output = Command::new(cxx_command())
        .arg("-dumpmachine")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let target = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!target.is_empty()).then_some(target)
}

fn macos_sdk_path() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    let output = Command::new("xcrun").arg("--show-sdk-path").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let path = PathBuf::from(text.trim());
    path.is_dir().then_some(path)
}

fn quest_root_from_env_vars() -> Option<QuestRoot> {
    QUEST_ENV_VARS.iter().find_map(|source| {
        let value = env::var_os(source)?;
        normalize_quest_root(Path::new(&value)).map(|path| QuestRoot { path, source })
    })
}

fn quest_root_from_cmake_prefix_path() -> Option<QuestRoot> {
    env::var_os("CMAKE_PREFIX_PATH")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .find_map(|path| {
            normalize_quest_root(&path).map(|path| QuestRoot {
                path,
                source: "CMAKE_PREFIX_PATH",
            })
        })
}

fn quest_root_error() -> String {
    "could not locate QuEST headers; set QUEST_ROOT, QUEST_DIR, QuEST_ROOT, QuEST_DIR, or CMAKE_PREFIX_PATH to a QuEST install containing include/quest.h".to_owned()
}

fn contains_libclang(path: &Path) -> bool {
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name == "libclang.dylib"
                || name == "libclang.so"
                || name.starts_with("libclang.so.")
                || name == "libclang.dll"
        })
}

fn llvm_config_path(arg: &str) -> Option<PathBuf> {
    let output = Command::new("llvm-config").arg(arg).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(PathBuf::from(value))
}

fn discovered_include_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(includedir) = llvm_config_path("--includedir").filter(|path| path.is_dir()) {
        dirs.push(includedir);
    }

    if let Some(prefix) = llvm_config_path("--prefix") {
        let libcxx = prefix.join("include").join("c++").join("v1");
        if libcxx.is_dir() {
            dirs.push(libcxx);
        }
    }

    for var in ["CPATH", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH"] {
        if let Some(value) = env::var_os(var) {
            dirs.extend(env::split_paths(&value).filter(|path| path.is_dir()));
        }
    }

    dirs.sort();
    dirs.dedup();
    dirs
}

fn clang_resource_dir() -> Option<PathBuf> {
    let output = Command::new(clang_command())
        .arg("-print-resource-dir")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    let path = PathBuf::from(value);
    path.is_dir().then_some(path)
}

fn clang_command() -> String {
    env::var("CLANG").unwrap_or_else(|_| {
        llvm_config_path("--bindir")
            .map(|bindir| bindir.join("clang"))
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "clang".to_owned())
    })
}

fn cxx_command() -> String {
    env::var("CXX").unwrap_or_else(|_| {
        llvm_config_path("--bindir")
            .map(|bindir| bindir.join("clang++"))
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "clang++".to_owned())
    })
}

fn collect_function_decls(
    entity: Entity<'_>,
    include_root: &Path,
    seen: &mut BTreeSet<String>,
    items: &mut Vec<ApiItem>,
) {
    if entity.get_kind() == EntityKind::FunctionDecl
        && let Some(item) = api_item_from_entity(entity, include_root)
        && seen.insert(item.overload_key.clone())
    {
        items.push(item);
    }

    for child in entity.get_children() {
        collect_function_decls(child, include_root, seen, items);
    }
}

fn api_item_from_entity(entity: Entity<'_>, include_root: &Path) -> Option<ApiItem> {
    if entity.get_availability() == Availability::Deprecated {
        return None;
    }

    let name = entity.get_name()?;
    if name.starts_with('_') {
        return None;
    }

    let location = entity.get_location()?.get_file_location();
    let line = location.line;
    let file = location.file?.get_path();
    let file = canonicalize_existing(&file).ok()?;
    if !file.starts_with(include_root)
        || file.file_name().is_some_and(|name| name == "deprecated.h")
    {
        return None;
    }

    let header = file
        .strip_prefix(include_root)
        .unwrap_or(file.as_path())
        .display()
        .to_string();
    let result_type = entity.get_result_type()?;
    let result_type_name = result_type.get_display_name();
    let result_canonical_type = result_type.get_canonical_type().get_display_name();
    let arguments = entity
        .get_arguments()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, argument)| {
            let ty = argument.get_type()?;
            Some(ApiArgument {
                name: argument
                    .get_name()
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("arg{index}")),
                ty: ty.get_display_name(),
                canonical_type: ty.get_canonical_type().get_display_name(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let overload_key = overload_key(&name, &result_canonical_type, &arguments);
    let display_name = entity.get_display_name().unwrap_or_else(|| {
        let args = arguments
            .iter()
            .map(|argument| format!("{} {}", argument.ty, argument.name))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{name}({args})")
    });
    let signature = format!("{result_type_name} {display_name}");

    Some(ApiItem {
        name,
        overload_key,
        header,
        line,
        linkage: entity
            .get_linkage()
            .map(|linkage| format!("{linkage:?}"))
            .unwrap_or_else(|| "none".to_owned()),
        availability: format!("{:?}", entity.get_availability()),
        result_type: result_type_name,
        result_canonical_type,
        arguments,
        signature,
        status: "unclassified".to_owned(),
        reason: String::new(),
    })
}

pub(crate) fn canonicalize_existing(path: &Path) -> Result<PathBuf, DynError> {
    path.canonicalize()
        .map_err(|error| format!("failed to canonicalize {}: {error}", path.display()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root() -> Option<QuestRoot> {
        find_quest_root().ok()
    }

    #[test]
    fn libclang_error_mentions_expected_lookup_paths() {
        let message = format_libclang_error("not found");

        assert!(message.contains("LIBCLANG_PATH"));
        assert!(message.contains("llvm-config"));
    }

    #[test]
    fn libclang_extracts_representative_overloads() {
        let Some(root) = fixture_root() else {
            eprintln!("skipping test because no QuEST root was provided by environment");
            return;
        };

        let items = collect_quest_api(root.path()).expect("libclang should parse QuEST headers");
        let overloads = items
            .iter()
            .filter(|item| item.name == "applyCompMatr")
            .collect::<Vec<_>>();

        assert!(
            overloads.iter().any(|item| item
                .arguments
                .iter()
                .any(|arg| arg.ty.contains("std::vector<int>"))),
            "expected C++ std::vector overload"
        );
        assert!(
            overloads
                .iter()
                .any(|item| item.arguments.iter().any(|arg| arg.ty.contains('*'))),
            "expected C pointer overload"
        );
        assert!(overloads.iter().all(|item| item.line > 0));
    }
}
