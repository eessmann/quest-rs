use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};

pub mod clang;
pub mod classify;
pub mod emit;
pub mod model;
pub mod type_rules;

pub type DynError = Box<dyn std::error::Error + Send + Sync>;

pub fn run(check: bool) -> Result<(), DynError> {
	let workspace = find_workspace_root()?;
	let work =
		crate::tooling::native_work_directory(&workspace, "xtask-binding-native-discovery-")?;
	let context = quest_build::NativeBuildContext::for_tooling(work.path(), None)?;
	let package = context.discover()?;
	let quest_root = clang::QuestRoot::from_package(&package);
	let registry = emit::load_adapter_registry(&workspace)?;

	let mut items = clang::collect_quest_api(&package)?;
	classify::classify_items(&mut items, &registry)?;

	let outputs = emit::render_outputs(&quest_root, &items, &registry)?;
	for output in outputs {
		let contents = if Path::new(output.path)
			.extension()
			.is_some_and(|ext| ext == "rs")
		{
			format_rust(&workspace, &output.contents)?
		} else {
			output.contents
		};
		emit::write_or_check(check, &workspace.join(output.path), output.path, contents)?;
	}

	Ok(())
}

// Compare exactly the same representation that workspace `cargo fmt` retains.
// Raw templates intentionally remain independent of workspace indentation.
fn format_rust(workspace: &Path, source: &str) -> Result<String, DynError> {
	let mut file = tempfile::Builder::new().suffix(".rs").tempfile()?;
	file.write_all(source.as_bytes())?;
	file.flush()?;
	let output = std::process::Command::new("rustfmt")
		.arg("--config-path")
		.arg(workspace.join(".rustfmt.toml"))
		.arg(file.path())
		.output()?;
	if !output.status.success() {
		return Err(format!(
			"formatting generated Rust bindings failed: {}",
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}
	Ok(std::fs::read_to_string(file.path())?)
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
