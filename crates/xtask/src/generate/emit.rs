use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::DynError;
use super::clang::QuestRoot;
use super::model::{
    AdapterEntry, AdapterManifest, AdapterRegistry, AdapterSourceKind, ApiItem, CoverageManifest,
    macro_value, rust_name,
};

const GENERATOR_NAME: &str = "xtask generate-quest-bindings";
const ADAPTER_MANIFEST_PATH: &str = "crates/quest-sys/generated/generated_adapters.json";
const COVERAGE_MANIFEST_PATH: &str = "crates/quest-sys/generated/api_coverage.json";
const GENERATED_NAMES_PATH: &str = "crates/quest-sys/generated/generated_names.txt";
const GENERATED_RUST_PATH: &str = "crates/quest-sys/src/generated_api.rs";
const GENERATED_HEADER_PATH: &str =
    "crates/quest-sys/src/cxx_bindings/include/quest_generated_bindings.hpp";
const GENERATED_CPP_PATH: &str = "crates/quest-sys/src/cxx_bindings/quest_generated_bindings.cpp";

const GENERATED_RUST_TEMPLATE: &str = include_str!("../../templates/generated_api.rs");
const GENERATED_HEADER_TEMPLATE: &str =
    include_str!("../../templates/quest_generated_bindings.hpp");
const GENERATED_CPP_TEMPLATE: &str = include_str!("../../templates/quest_generated_bindings.cpp");

pub struct GeneratedOutput {
    pub path: &'static str,
    pub contents: String,
}

#[derive(serde::Deserialize)]
struct CoverageBootstrap {
    items: Vec<ApiItem>,
}

pub fn load_adapter_registry(workspace: &Path, check: bool) -> Result<AdapterRegistry, DynError> {
    let path = workspace.join(ADAPTER_MANIFEST_PATH);
    if path.is_file() {
        let text = fs::read_to_string(&path)?;
        let manifest: AdapterManifest = serde_json::from_str(&text)?;
        return AdapterRegistry::new(sorted_entries(manifest.entries)).map_err(Into::into);
    }

    if check {
        return Err(format!(
            "{ADAPTER_MANIFEST_PATH} is missing; rerun xtask generate-quest-bindings"
        )
        .into());
    }

    bootstrap_adapter_registry(workspace)
}

pub fn render_outputs(
    quest_root: &QuestRoot,
    items: &[ApiItem],
    registry: &AdapterRegistry,
) -> Result<Vec<GeneratedOutput>, DynError> {
    Ok(vec![
        GeneratedOutput {
            path: GENERATED_RUST_PATH,
            contents: GENERATED_RUST_TEMPLATE.to_owned(),
        },
        GeneratedOutput {
            path: GENERATED_HEADER_PATH,
            contents: GENERATED_HEADER_TEMPLATE.to_owned(),
        },
        GeneratedOutput {
            path: GENERATED_CPP_PATH,
            contents: GENERATED_CPP_TEMPLATE.to_owned(),
        },
        GeneratedOutput {
            path: GENERATED_NAMES_PATH,
            contents: render_generated_names(registry),
        },
        GeneratedOutput {
            path: ADAPTER_MANIFEST_PATH,
            contents: render_adapter_manifest(registry)?,
        },
        GeneratedOutput {
            path: COVERAGE_MANIFEST_PATH,
            contents: render_coverage_manifest(quest_root, items)?,
        },
    ])
}

pub fn write_or_check(
    check: bool,
    path: &Path,
    label: &str,
    contents: String,
) -> Result<(), DynError> {
    if check {
        let existing = fs::read_to_string(path).map_err(|error| {
            format!(
                "could not read generated artifact {label}: {error}; rerun xtask generate-quest-bindings"
            )
        })?;
        if existing != contents {
            return Err(format!("{label} is stale; rerun xtask generate-quest-bindings").into());
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn bootstrap_adapter_registry(workspace: &Path) -> Result<AdapterRegistry, DynError> {
    let coverage_path = workspace.join(COVERAGE_MANIFEST_PATH);
    let text = fs::read_to_string(&coverage_path).map_err(|error| {
        format!(
            "could not bootstrap {ADAPTER_MANIFEST_PATH} from {COVERAGE_MANIFEST_PATH}: {error}"
        )
    })?;
    let coverage: CoverageBootstrap = serde_json::from_str(&text)?;
    let entries = coverage
        .items
        .into_iter()
        .filter(|item| item.status == "generated")
        .map(|item| {
            let source_kind = if item.reason.contains("hand-written core") {
                AdapterSourceKind::Core
            } else {
                AdapterSourceKind::Generated
            };
            AdapterEntry {
                overload_key: item.overload_key,
                quest_name: item.name.clone(),
                adapter_name: item.name.clone(),
                rust_name: rust_name(&item.name),
                source_kind,
            }
        })
        .collect::<Vec<_>>();

    AdapterRegistry::new(sorted_entries(entries)).map_err(Into::into)
}

fn sorted_entries(mut entries: Vec<AdapterEntry>) -> Vec<AdapterEntry> {
    entries.sort_by(|left, right| {
        left.overload_key
            .cmp(&right.overload_key)
            .then_with(|| left.adapter_name.cmp(&right.adapter_name))
    });
    entries
}

fn render_generated_names(registry: &AdapterRegistry) -> String {
    let mut names = BTreeSet::new();
    for entry in registry.entries() {
        if entry.source_kind == AdapterSourceKind::Generated {
            names.insert(entry.adapter_name.as_str());
        }
    }

    let mut out = names.into_iter().collect::<Vec<_>>().join("\n");
    out.push('\n');
    out
}

fn render_adapter_manifest(registry: &AdapterRegistry) -> Result<String, DynError> {
    let manifest = AdapterManifest {
        generator: GENERATOR_NAME.to_owned(),
        entries: registry.entries().to_vec(),
    };
    let mut out = serde_json::to_string_pretty(&manifest)?;
    out.push('\n');
    Ok(out)
}

fn render_coverage_manifest(quest_root: &QuestRoot, items: &[ApiItem]) -> Result<String, DynError> {
    let config = fs::read_to_string(quest_root.path().join("include/quest/include/config.h"))?;
    let version =
        macro_value(&config, "QUEST_VERSION_STRING").unwrap_or_else(|| "unknown".to_owned());
    let deprecated = macro_value(&config, "QUEST_INCLUDE_DEPRECATED_FUNCTIONS")
        .unwrap_or_else(|| "unknown".to_owned());

    let mut counts = BTreeMap::<String, usize>::new();
    for item in items {
        *counts.entry(item.status.clone()).or_default() += 1;
    }

    let manifest = CoverageManifest {
        generator: GENERATOR_NAME,
        parser: "clang crate libclang semantic AST",
        quest_version: version,
        quest_root: quest_root.source_label().to_owned(),
        deprecated_apis_included: deprecated,
        counts,
        items,
    };

    let mut out = serde_json::to_string_pretty(&manifest)?;
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_generated_file_detects_stale_contents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("artifact.rs");
        fs::write(&path, "old").expect("write fixture");

        let error = write_or_check(true, &path, "artifact.rs", "new".to_owned())
            .expect_err("stale artifact should fail")
            .to_string();

        assert!(error.contains("artifact.rs is stale"));
    }

    #[test]
    fn generated_source_artifacts_are_stale_checked() {
        let dir = tempfile::tempdir().expect("tempdir");

        for label in [
            GENERATED_RUST_PATH,
            GENERATED_HEADER_PATH,
            GENERATED_CPP_PATH,
        ] {
            let path = dir.path().join(label);
            fs::create_dir_all(path.parent().expect("parent")).expect("create fixture parent");
            fs::write(&path, "stale").expect("write fixture");

            let error = write_or_check(true, &path, label, "fresh".to_owned())
                .expect_err("stale source artifact should fail")
                .to_string();

            assert!(error.contains(label));
            assert!(error.contains("is stale"));
        }
    }

    #[test]
    fn checked_in_generator_files_do_not_contain_machine_paths() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let files = [
            "build.rs",
            "crates/quest-sys/build.rs",
            "crates/quest-sys/generated/api_coverage.json",
            "crates/quest-sys/generated/generated_adapters.json",
            "crates/quest-sys/generated/generated_names.txt",
            "crates/xtask/src/generate/clang.rs",
            "crates/xtask/src/generate/classify.rs",
            "crates/xtask/src/generate/emit.rs",
            "crates/xtask/templates/generated_api.rs",
            "crates/xtask/templates/quest_generated_bindings.cpp",
            "crates/xtask/templates/quest_generated_bindings.hpp",
        ];
        let forbidden = [
            ["/", "Users", "/"].concat(),
            ["/", "opt", "/", "homebrew"].concat(),
            ["DEFAULT", "_QUEST", "_ROOT"].concat(),
            ["HOMEBREW", "_LLVM", "_LIB"].concat(),
        ];

        for file in files {
            let contents = fs::read_to_string(workspace.join(file)).expect("read checked-in file");
            for pattern in &forbidden {
                assert!(
                    !contents.contains(pattern),
                    "{file} contains forbidden machine-specific fragment {pattern}"
                );
            }
        }
    }
}
