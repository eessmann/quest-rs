use std::env;
use std::path::{Path, PathBuf};

pub mod clang;
pub mod classify;
pub mod emit;
pub mod model;
pub mod type_rules;

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

pub fn run(check: bool) -> Result<(), DynError> {
    let workspace = find_workspace_root()?;
    let package = quest_build::discover_for_tooling(
        workspace.join("target/xtask-binding-native-discovery"),
        None,
    )?;
    let quest_root = clang::QuestRoot::from_package(&package);
    let registry = emit::load_adapter_registry(&workspace, check)?;

    let mut items = clang::collect_quest_api(&package)?;
    classify::classify_items(&mut items, &registry)?;

    let outputs = emit::render_outputs(&quest_root, &items, &registry)?;
    for output in outputs {
        emit::write_or_check(
            check,
            &workspace.join(output.path),
            output.path,
            output.contents,
        )?;
    }

    Ok(())
}

pub fn find_workspace_root() -> Result<PathBuf, DynError> {
    if let Ok(value) = env::var("CARGO_MANIFEST_DIR")
        && let Some(root) = workspace_root_from(Path::new(&value))
    {
        return Ok(root);
    }

    let current = env::current_dir()?;
    workspace_root_from(&current).ok_or_else(|| "could not locate quest-rs workspace root".into())
}

fn workspace_root_from(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        if current.join("Cargo.toml").is_file() && current.join("crates/quest-sys").is_dir() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}
