use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use super::DynError;
use super::clang::QuestRoot;
use super::model::{
    AdapterEntry, AdapterManifest, AdapterRegistry, AdapterSourceKind, ApiItem, CoverageManifest,
    macro_value,
};

const GENERATOR_NAME: &str = "xtask generate-quest-bindings";
const ADAPTER_MANIFEST_PATH: &str = "crates/quest-sys/generated/generated_adapters.json";
const COVERAGE_MANIFEST_PATH: &str = "crates/quest-sys/generated/api_coverage.json";
const GENERATED_RUST_PATH: &str = "crates/quest-sys/src/generated_api.rs";
const GENERATED_HEADER_PATH: &str =
    "crates/quest-sys/src/cxx_bindings/include/quest_generated_bindings.hpp";
const GENERATED_CPP_PATH: &str = "crates/quest-sys/src/cxx_bindings/quest_generated_bindings.cpp";

const GENERATED_RUST_TEMPLATE: &str = include_str!("../../templates/generated_api.rs");
const GENERATED_HEADER_TEMPLATE: &str =
    include_str!("../../templates/quest_generated_bindings.hpp");
const GENERATED_CPP_TEMPLATE: &str = include_str!("../../templates/quest_generated_bindings.cpp");

// These two homogeneous native families share one reviewed signature. Pauli
// gadgets and Pauli strings have different inputs and remain explicit templates.
struct PauliFamily {
    marker: &'static str,
    rust_prefix: &'static str,
    native_prefix: &'static str,
    has_states: bool,
}
const PAULI_FAMILIES: [PauliFamily; 2] = [
    PauliFamily {
        marker: "multi",
        rust_prefix: "apply_multi_controlled_pauli_",
        native_prefix: "applyMultiControlledPauli",
        has_states: false,
    },
    PauliFamily {
        marker: "state",
        rust_prefix: "apply_multi_state_controlled_pauli_",
        native_prefix: "applyMultiStateControlledPauli",
        has_states: true,
    },
];

struct FamilyOutput {
    ffi: String,
    rust: String,
    header: String,
    cpp: String,
}

fn render_pauli_family(
    family: &PauliFamily,
    registry: &AdapterRegistry,
) -> Result<FamilyOutput, DynError> {
    let mut output = FamilyOutput {
        ffi: String::new(),
        rust: String::new(),
        header: String::new(),
        cpp: String::new(),
    };
    let mut expected = BTreeSet::new();
    for (axis, upper) in [('x', 'X'), ('y', 'Y'), ('z', 'Z')] {
        let rust_name = format!("{}{axis}", family.rust_prefix);
        let quest_name = format!("{}{upper}", family.native_prefix);
        let signature = if family.has_states {
            "(Qureg, std::vector<int>, std::vector<int>, int) -> void"
        } else {
            "(Qureg, std::vector<int>, int) -> void"
        };
        let overload_key = format!("{quest_name}{signature}");
        let entry = registry
            .get(&overload_key)
            .ok_or_else(|| format!("descriptor has no QuEST overload: {overload_key}"))?;
        if entry.quest_name != quest_name
            || entry.adapter_name != rust_name
            || entry.rust_name != rust_name
            || entry.source_kind != AdapterSourceKind::Generated
        {
            return Err(format!("descriptor differs from adapter registry: {overload_key}").into());
        }
        expected.insert(overload_key);

        let rust_states = if family.has_states {
            "            states: &[i32],\n"
        } else {
            ""
        };
        let rust_public_states = if family.has_states {
            "    states: &[i32],\n"
        } else {
            ""
        };
        let rust_call = if family.has_states {
            format!(
                "    map_quest_result(ffi::{rust_name}(\n        qureg, controls, states, target,\n    ))\n"
            )
        } else {
            format!("    map_quest_result(ffi::{rust_name}(qureg, controls, target))\n")
        };
        write!(
            output.ffi,
            "        fn {rust_name}(\n            qureg: Pin<&mut Qureg>,\n            controls: &[i32],\n{rust_states}            target: i32,\n        ) -> Result<()>;\n"
        )?;
        write!(
            output.rust,
            "pub fn {rust_name}(\n    qureg: Pin<&mut Qureg>,\n    controls: &[i32],\n{rust_public_states}    target: i32,\n) -> QuestResult<()> {{\n{rust_call}}}\n\n"
        )?;
        let cpp_states = if family.has_states {
            "    rust::Slice<const std::int32_t> states,\n"
        } else {
            ""
        };
        let cpp_indent = " ".repeat(quest_name.len().checked_add(5).ok_or("indent overflow")?);
        let cpp_call = if family.has_states {
            format!(
                "qureg.raw(), to_int_vec(controls),\n{cpp_indent}to_int_vec(states),\n{cpp_indent}static_cast<int>(target)"
            )
        } else {
            format!("qureg.raw(), to_int_vec(controls),\n{cpp_indent}static_cast<int>(target)")
        };
        let cpp_params = if family.has_states {
            format!(
                "void {rust_name}(\n    Qureg& qureg,\n    rust::Slice<const std::int32_t> controls,\n{cpp_states}    std::int32_t target)"
            )
        } else {
            let indent = " ".repeat(rust_name.len().checked_add(6).ok_or("indent overflow")?);
            format!(
                "void {rust_name}(Qureg& qureg,\n{indent}rust::Slice<const std::int32_t> controls,\n{indent}std::int32_t target)"
            )
        };
        writeln!(output.header, "{cpp_params};")?;
        write!(
            output.cpp,
            "{cpp_params} {{\n  const auto admission = admit_native_call();\n  ::{quest_name}({cpp_call});\n}}\n\n"
        )?;
    }

    verify_pauli_family_exceptions(family, registry, &expected)?;
    Ok(output)
}

fn verify_pauli_family_exceptions(
    family: &PauliFamily,
    registry: &AdapterRegistry,
    expected: &BTreeSet<String>,
) -> Result<(), DynError> {
    let mut observed = BTreeSet::new();
    for entry in registry
        .entries()
        .iter()
        .filter(|entry| entry.quest_name.starts_with(family.native_prefix))
    {
        if expected.contains(&entry.overload_key) {
            continue;
        }
        let suffix = entry
            .quest_name
            .strip_prefix(family.native_prefix)
            .unwrap_or_default();
        if !matches!(suffix, "Gadget" | "Str")
            || entry.source_kind != AdapterSourceKind::Generated
            || !GENERATED_CPP_TEMPLATE.contains(&format!("void {}(", entry.rust_name))
            || !GENERATED_HEADER_TEMPLATE.contains(&format!("void {}(", entry.rust_name))
            || !GENERATED_RUST_TEMPLATE.contains(&format!("pub fn {}(", entry.rust_name))
        {
            return Err(format!(
                "unreviewed or missing exceptional adapter: {}",
                entry.overload_key
            )
            .into());
        }
        observed.insert(suffix);
    }
    for required in ["Gadget", "Str"] {
        if !observed.contains(required) {
            return Err(format!(
                "missing explicit exceptional adapter: {}{required}",
                family.native_prefix
            )
            .into());
        }
    }
    Ok(())
}

fn insert_once(template: &mut String, marker: &str, rendered: &str) -> Result<(), DynError> {
    if template.matches(marker).count() != 1 {
        return Err(format!("expected one generator insertion point: {marker}").into());
    }
    *template = template.replace(marker, rendered.trim_end());
    Ok(())
}

fn render_source_templates(
    registry: &AdapterRegistry,
) -> Result<(String, String, String), DynError> {
    let mut rust = GENERATED_RUST_TEMPLATE.to_owned();
    let mut header = GENERATED_HEADER_TEMPLATE.to_owned();
    let mut cpp = GENERATED_CPP_TEMPLATE.to_owned();
    for family in &PAULI_FAMILIES {
        let output = render_pauli_family(family, registry)?;
        insert_once(
            &mut rust,
            &format!("// @pauli-{}-ffi@", family.marker),
            &output.ffi,
        )?;
        insert_once(
            &mut rust,
            &format!("// @pauli-{}-rust@", family.marker),
            &output.rust,
        )?;
        insert_once(
            &mut header,
            &format!("// @pauli-{}-header@", family.marker),
            &output.header,
        )?;
        insert_once(
            &mut cpp,
            &format!("// @pauli-{}-cpp@", family.marker),
            &output.cpp,
        )?;
    }
    Ok((rust, header, cpp))
}

pub struct GeneratedOutput {
    pub path: &'static str,
    pub contents: String,
}

pub fn load_adapter_registry(workspace: &Path) -> Result<AdapterRegistry, DynError> {
    let path = workspace.join(ADAPTER_MANIFEST_PATH);
    let text = fs::read_to_string(&path).map_err(|error| format!(
        "could not read reviewed adapter registry {ADAPTER_MANIFEST_PATH}: {error}; restore the reviewed registry from version control before generating bindings"
    ))?;
    let manifest: AdapterManifest = serde_json::from_str(&text)?;
    AdapterRegistry::new(sorted_entries(manifest.entries)).map_err(Into::into)
}

pub fn render_outputs(
    quest_root: &QuestRoot,
    items: &[ApiItem],
    registry: &AdapterRegistry,
) -> Result<Vec<GeneratedOutput>, DynError> {
    let (rust, header, cpp) = render_source_templates(registry)?;
    Ok(vec![
        GeneratedOutput {
            path: GENERATED_RUST_PATH,
            contents: rust,
        },
        GeneratedOutput {
            path: GENERATED_HEADER_PATH,
            contents: header,
        },
        GeneratedOutput {
            path: GENERATED_CPP_PATH,
            contents: cpp,
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

fn sorted_entries(mut entries: Vec<AdapterEntry>) -> Vec<AdapterEntry> {
    entries.sort_by(|left, right| {
        left.overload_key
            .cmp(&right.overload_key)
            .then_with(|| left.adapter_name.cmp(&right.adapter_name))
    });
    entries
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
        let count = counts.entry(item.status.clone()).or_default();
        *count = count
            .checked_add(1)
            .ok_or("coverage manifest item count overflow")?;
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
    use googletest::prelude::*;

    #[gtest]
    fn check_generated_file_detects_stale_contents() -> googletest::Result<()> {
        let dir = tempfile::tempdir().or_fail()?;
        let path = dir.path().join("artifact.rs");
        fs::write(&path, "old").or_fail()?;

        let Err(error) = write_or_check(true, &path, "artifact.rs", "new".to_owned()) else {
            return fail!("stale artifact should fail");
        };

        verify_that!(
            error.to_string(),
            contains_substring("artifact.rs is stale")
        )
    }

    #[gtest]
    fn generated_source_artifacts_are_stale_checked() -> googletest::Result<()> {
        let dir = tempfile::tempdir().or_fail()?;

        for label in [
            GENERATED_RUST_PATH,
            GENERATED_HEADER_PATH,
            GENERATED_CPP_PATH,
        ] {
            let path = dir.path().join(label);
            fs::create_dir_all(path.parent().or_fail()?).or_fail()?;
            fs::write(&path, "stale").or_fail()?;

            let Err(error) = write_or_check(true, &path, label, "fresh".to_owned()) else {
                return fail!("stale source artifact should fail");
            };

            expect_that!(error.to_string(), contains_substring(label));
            expect_that!(error.to_string(), contains_substring("is stale"));
        }

        Ok(())
    }

    #[gtest]
    fn generated_bridge_shares_the_public_complex_value_type() -> googletest::Result<()> {
        for source in [
            GENERATED_RUST_TEMPLATE,
            GENERATED_HEADER_TEMPLATE,
            GENERATED_CPP_TEMPLATE,
        ] {
            verify_that!(source, not(contains_substring("GeneratedComplex")))?;
        }
        verify_that!(
            GENERATED_RUST_TEMPLATE,
            contains_substring("type QuestComplex = crate::ffi::QuestComplex")
        )
    }

    #[gtest]
    fn pauli_control_families_have_generator_insertion_points() -> googletest::Result<()> {
        for (source, markers) in [
            (
                GENERATED_RUST_TEMPLATE,
                [
                    "@pauli-multi-ffi@",
                    "@pauli-state-ffi@",
                    "@pauli-multi-rust@",
                    "@pauli-state-rust@",
                ]
                .as_slice(),
            ),
            (
                GENERATED_HEADER_TEMPLATE,
                ["@pauli-multi-header@", "@pauli-state-header@"].as_slice(),
            ),
            (
                GENERATED_CPP_TEMPLATE,
                ["@pauli-multi-cpp@", "@pauli-state-cpp@"].as_slice(),
            ),
        ] {
            for marker in markers {
                verify_that!(source.contains(marker), eq(true))?;
            }
        }
        Ok(())
    }

    #[gtest]
    fn pauli_family_descriptors_cover_the_registry_and_generated_sources() -> googletest::Result<()>
    {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .or_fail()?;
        let registry = load_adapter_registry(workspace).or_fail()?;
        let (rust, header, cpp) = render_source_templates(&registry).or_fail()?;

        for family in &PAULI_FAMILIES {
            for axis in ['x', 'y', 'z'] {
                let name = format!("{}{axis}", family.rust_prefix);
                verify_that!(rust.matches(&format!("fn {name}(")).count(), eq(2))?;
                verify_that!(header.matches(&format!("void {name}(")).count(), eq(1))?;
                verify_that!(cpp.matches(&format!("void {name}(")).count(), eq(1))?;
            }
        }
        verify_that!(rust, not(contains_substring("@pauli-")))?;
        verify_that!(header, not(contains_substring("@pauli-")))?;
        verify_that!(cpp, not(contains_substring("@pauli-")))?;

        let omitted = "applyMultiControlledPauliX(Qureg, std::vector<int>, int) -> void";
        let entries = registry
            .entries()
            .iter()
            .filter(|entry| entry.overload_key != omitted)
            .cloned()
            .collect();
        let incomplete_registry = AdapterRegistry::new(entries).or_fail()?;
        let Err(error) = render_source_templates(&incomplete_registry) else {
            return fail!("missing family overload must fail generation");
        };
        verify_that!(error.to_string(), contains_substring(omitted))?;

        let exceptional = "applyMultiControlledPauliGadget";
        let entries = registry
            .entries()
            .iter()
            .filter(|entry| entry.quest_name != exceptional)
            .cloned()
            .collect();
        let incomplete_registry = AdapterRegistry::new(entries).or_fail()?;
        let Err(error) = render_source_templates(&incomplete_registry) else {
            return fail!("missing explicit exception must fail generation");
        };
        verify_that!(error.to_string(), contains_substring(exceptional))
    }

    #[gtest]
    fn generation_requires_the_reviewed_adapter_registry() -> googletest::Result<()> {
        let workspace = tempfile::tempdir().or_fail()?;
        let coverage = workspace.path().join(COVERAGE_MANIFEST_PATH);
        fs::create_dir_all(coverage.parent().or_fail()?).or_fail()?;
        // A coverage receipt is not authority to recreate reviewed adapters.
        fs::write(coverage, r#"{"items": []}"#).or_fail()?;
        {
            let result = load_adapter_registry(workspace.path());
            let Err(error) = result else {
                return fail!("missing reviewed registry must fail in every generation mode");
            };
            verify_that!(
                error.to_string(),
                contains_substring("reviewed adapter registry")
            )?;
        }
        Ok(())
    }

    #[gtest]
    fn checked_in_generator_files_do_not_contain_machine_paths() -> googletest::Result<()> {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .or_fail()?;
        let files = [
            "crates/quest/build.rs",
            "crates/quest-sys/build.rs",
            "crates/quest-sys/generated/api_coverage.json",
            "crates/quest-sys/generated/generated_adapters.json",
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
            let contents = fs::read_to_string(workspace.join(file)).or_fail()?;
            for pattern in &forbidden {
                expect_that!(contents.as_str(), not(contains_substring(pattern.as_str())));
            }
        }

        Ok(())
    }
}
