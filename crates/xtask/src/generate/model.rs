use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiArgument {
    pub name: String,
    pub ty: String,
    pub canonical_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiItem {
    pub name: String,
    pub overload_key: String,
    pub header: String,
    pub line: u32,
    pub linkage: String,
    pub availability: String,
    pub result_type: String,
    pub result_canonical_type: String,
    pub arguments: Vec<ApiArgument>,
    pub signature: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct CoverageManifest<'a> {
    pub generator: &'static str,
    pub parser: &'static str,
    pub quest_version: String,
    pub quest_root: String,
    pub deprecated_apis_included: String,
    pub counts: BTreeMap<String, usize>,
    pub items: &'a [ApiItem],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub generator: String,
    pub entries: Vec<AdapterEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterEntry {
    pub overload_key: String,
    pub quest_name: String,
    pub adapter_name: String,
    pub rust_name: String,
    pub source_kind: AdapterSourceKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterSourceKind {
    Core,
    Generated,
}

impl AdapterSourceKind {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Core => "emitted in hand-written core cxx bridge",
            Self::Generated => "emitted in checked-in generated cxx bridge",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdapterRegistry {
    entries: Vec<AdapterEntry>,
    by_overload_key: BTreeMap<String, AdapterEntry>,
}

impl AdapterRegistry {
    pub fn new(entries: Vec<AdapterEntry>) -> Result<Self, String> {
        let mut by_overload_key = BTreeMap::new();
        let mut generated_adapter_names = BTreeMap::<String, String>::new();
        let mut generated_rust_names = BTreeMap::<String, String>::new();
        for entry in &entries {
            if by_overload_key
                .insert(entry.overload_key.clone(), entry.clone())
                .is_some()
            {
                return Err(format!(
                    "duplicate adapter overload key in generated adapter manifest: {}",
                    entry.overload_key
                ));
            }

            if entry.source_kind == AdapterSourceKind::Generated {
                if entry.adapter_name != entry.rust_name {
                    return Err(format!(
                        "generated adapter name `{}` does not match rust name `{}` for overload `{}`",
                        entry.adapter_name, entry.rust_name, entry.overload_key
                    ));
                }
                reject_duplicate_generated_identifier(
                    "adapter name",
                    &mut generated_adapter_names,
                    &entry.adapter_name,
                    &entry.overload_key,
                )?;
                reject_duplicate_generated_identifier(
                    "rust name",
                    &mut generated_rust_names,
                    &entry.rust_name,
                    &entry.overload_key,
                )?;
            }
        }

        Ok(Self {
            entries,
            by_overload_key,
        })
    }

    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            by_overload_key: BTreeMap::new(),
        }
    }

    pub fn get(&self, overload_key: &str) -> Option<&AdapterEntry> {
        self.by_overload_key.get(overload_key)
    }

    pub fn entries(&self) -> &[AdapterEntry] {
        &self.entries
    }
}

fn reject_duplicate_generated_identifier(
    label: &str,
    seen: &mut BTreeMap<String, String>,
    identifier: &str,
    overload_key: &str,
) -> Result<(), String> {
    if let Some(existing_overload) = seen.insert(identifier.to_owned(), overload_key.to_owned()) {
        return Err(format!(
            "duplicate generated {label} `{identifier}` for overloads `{existing_overload}` and `{overload_key}`"
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageStatus {
    Generated,
    CoveredByRaii,
    UnsupportedCApi,
    GatedMpi,
    GatedCallback,
    GatedOpaquePointer,
    GatedManualAdapterNeeded,
}

impl CoverageStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::CoveredByRaii => "covered-by-raii",
            Self::UnsupportedCApi => "unsupported-c-api",
            Self::GatedMpi => "gated-mpi",
            Self::GatedCallback => "gated-callback",
            Self::GatedOpaquePointer => "gated-opaque-pointer",
            Self::GatedManualAdapterNeeded => "gated-manual-adapter-needed",
        }
    }
}

pub fn overload_key(name: &str, result_type: &str, arguments: &[ApiArgument]) -> String {
    let args = arguments
        .iter()
        .map(|argument| normalize_type_key(&argument.canonical_type))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({}) -> {}", name, args, normalize_type_key(result_type))
}

pub fn normalize_type_key(ty: &str) -> String {
    ty.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn macro_value(config: &str, name: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("#define ")?;
        let value = rest.strip_prefix(name)?.trim();
        Some(value.trim_matches('"').to_owned())
    })
}

pub fn rust_name(quest_name: &str) -> String {
    let mut out = String::new();
    let mut previous_was_lower_or_digit = false;

    for character in quest_name.chars() {
        if character.is_ascii_uppercase() {
            if previous_was_lower_or_digit {
                out.push('_');
            }
            out.push(character.to_ascii_lowercase());
            previous_was_lower_or_digit = false;
        } else {
            out.push(character);
            previous_was_lower_or_digit =
                character.is_ascii_lowercase() || character.is_ascii_digit();
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated_entry(overload_key: &str, adapter_name: &str, rust_name: &str) -> AdapterEntry {
        AdapterEntry {
            overload_key: overload_key.to_owned(),
            quest_name: adapter_name.to_owned(),
            adapter_name: adapter_name.to_owned(),
            rust_name: rust_name.to_owned(),
            source_kind: AdapterSourceKind::Generated,
        }
    }

    #[test]
    fn generated_adapter_names_must_identify_one_overload() {
        let error = AdapterRegistry::new(vec![
            generated_entry(
                "getPauliStr(std::string) -> PauliStr",
                "get_pauli_str",
                "get_pauli_str",
            ),
            generated_entry(
                "getPauliStr(std::string, std::vector<int>) -> PauliStr",
                "get_pauli_str",
                "get_pauli_str",
            ),
        ])
        .expect_err("duplicate generated adapter names must be rejected");

        assert!(error.contains("duplicate generated adapter name"));
    }

    #[test]
    fn generated_adapter_name_must_match_rust_bridge_symbol() {
        let error = AdapterRegistry::new(vec![generated_entry(
            "applyCompMatr1(Qureg, int, CompMatr1) -> void",
            "applyCompMatr1",
            "apply_comp_matr1",
        )])
        .expect_err("generated adapter names must be bridge symbols");

        assert!(error.contains("generated adapter name"));
        assert!(error.contains("does not match rust name"));
    }
}
