use std::env;
use std::path::{Path, PathBuf};

pub mod clang;
pub mod classify;
pub mod emit;
pub mod model;
pub mod type_rules;

pub type DynError = Box<dyn std::error::Error>;

pub fn run(check: bool) -> Result<(), DynError> {
    let workspace = find_workspace_root()?;
    let quest_root = clang::find_quest_root()?;
    let registry = emit::load_adapter_registry(&workspace, check)?;

    let mut items = clang::collect_quest_api(quest_root.path())?;
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

pub(crate) fn find_workspace_root() -> Result<PathBuf, DynError> {
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
