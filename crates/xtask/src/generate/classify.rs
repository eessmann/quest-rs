use super::DynError;
use super::model::{AdapterRegistry, ApiItem, CoverageStatus};
use super::type_rules::{has_callback_signature, has_opaque_pointer, has_raw_pointer, type_text};

const COVERED_BY_RAII: &[(&str, &str)] = &[
    ("destroyCompMatr", "CompMatr Drop"),
    ("destroyDiagMatr", "DiagMatr Drop"),
    ("destroyFullStateDiagMatr", "FullStateDiagMatr Drop"),
    ("destroyKrausMap", "KrausMap Drop"),
    ("destroyPauliStrSum", "PauliStrSum Drop"),
    ("destroyQureg", "Qureg Drop"),
    ("destroySuperOp", "SuperOp Drop"),
];

pub fn classify_items(items: &mut [ApiItem], registry: &AdapterRegistry) -> Result<(), DynError> {
    for item in items.iter_mut() {
        let (status, reason) = classify_item(item, registry);
        item.status = status.as_str().to_owned();
        item.reason = reason;
    }

    let unclassified = items
        .iter()
        .filter(|item| item.status == "unclassified")
        .collect::<Vec<_>>();
    if !unclassified.is_empty() {
        return Err(format!(
            "{} non-deprecated QuEST API items were left unclassified",
            unclassified.len()
        )
        .into());
    }

    Ok(())
}

fn classify_item(item: &ApiItem, registry: &AdapterRegistry) -> (CoverageStatus, String) {
    if let Some((_, owner)) = COVERED_BY_RAII
        .iter()
        .find(|(name, _)| *name == item.name.as_str())
    {
        return (
            CoverageStatus::CoveredByRaii,
            format!("covered by Rust-owned {owner}"),
        );
    }

    if type_text(item).contains("MPI_Comm") {
        return (
            CoverageStatus::GatedMpi,
            "MPI_Comm requires a dedicated mpi Cargo feature and type strategy".to_owned(),
        );
    }

    if has_callback_signature(item) {
        return (
            CoverageStatus::GatedCallback,
            "signature contains a callback/function-pointer shape that needs a manual adapter"
                .to_owned(),
        );
    }

    if has_opaque_pointer(item) {
        return (
            CoverageStatus::GatedOpaquePointer,
            "signature contains an opaque void pointer that needs a manual ownership strategy"
                .to_owned(),
        );
    }

    if let Some(adapter) = registry.get(&item.overload_key) {
        return (
            CoverageStatus::Generated,
            adapter.source_kind.reason().to_owned(),
        );
    }

    if has_raw_pointer(item) {
        return (
            CoverageStatus::UnsupportedCApi,
            "C pointer signature is not exposed; target the C++ overload or a safe manual adapter"
                .to_owned(),
        );
    }

    (
        CoverageStatus::GatedManualAdapterNeeded,
        "requires a dedicated cxx-safe wrapper before exposing through quest-sys".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::clang::{collect_quest_api, find_quest_root};
    use crate::generate::emit::load_adapter_registry;
    use crate::generate::find_workspace_root;
    use crate::generate::model::{AdapterEntry, AdapterRegistry, AdapterSourceKind, ApiArgument};
    use googletest::prelude::*;

    fn fake_apply_comp_matr_item(overload_key: &str) -> ApiItem {
        ApiItem {
            name: "applyCompMatr".to_owned(),
            overload_key: overload_key.to_owned(),
            header: "quest/include/operations.h".to_owned(),
            line: 1,
            linkage: "External".to_owned(),
            availability: "Available".to_owned(),
            result_type: "void".to_owned(),
            result_canonical_type: "void".to_owned(),
            arguments: vec![
                ApiArgument {
                    name: "qureg".to_owned(),
                    ty: "Qureg".to_owned(),
                    canonical_type: "Qureg".to_owned(),
                },
                ApiArgument {
                    name: "target".to_owned(),
                    ty: "int".to_owned(),
                    canonical_type: "int".to_owned(),
                },
                ApiArgument {
                    name: "matrix".to_owned(),
                    ty: "CompMatr".to_owned(),
                    canonical_type: "CompMatr".to_owned(),
                },
            ],
            signature: "void applyCompMatr(Qureg, int, CompMatr)".to_owned(),
            status: "unclassified".to_owned(),
            reason: String::new(),
        }
    }

    #[gtest]
    fn generated_classification_requires_exact_overload_key() -> googletest::Result<()> {
        let mut items = vec![fake_apply_comp_matr_item(
            "applyCompMatr(Qureg, int, CompMatr) -> void",
        )];

        classify_items(&mut items, &AdapterRegistry::empty()).or_fail()?;

        verify_that!(items[0].status.as_str(), eq("gated-manual-adapter-needed"))
    }

    #[gtest]
    fn exact_adapter_entry_marks_only_that_overload_generated() -> googletest::Result<()> {
        let registry = AdapterRegistry::new(vec![AdapterEntry {
            overload_key: "applyCompMatr(Qureg, std::vector<int>, CompMatr) -> void".to_owned(),
            quest_name: "applyCompMatr".to_owned(),
            adapter_name: "applyCompMatr".to_owned(),
            rust_name: "apply_comp_matr".to_owned(),
            source_kind: AdapterSourceKind::Core,
        }])
        .or_fail()?;
        let mut items = vec![
            fake_apply_comp_matr_item("applyCompMatr(Qureg, int, CompMatr) -> void"),
            fake_apply_comp_matr_item("applyCompMatr(Qureg, std::vector<int>, CompMatr) -> void"),
        ];

        classify_items(&mut items, &registry).or_fail()?;

        expect_that!(items[0].status.as_str(), eq("gated-manual-adapter-needed"));
        verify_that!(items[1].status.as_str(), eq("generated"))
    }

    #[gtest]
    fn classification_distinguishes_overloads_by_signature_shape() -> googletest::Result<()> {
        let Ok(root) = find_quest_root() else {
            eprintln!("skipping test because no QuEST root was provided by environment");
            return Ok(());
        };

        let workspace = find_workspace_root().or_fail()?;
        let registry = load_adapter_registry(&workspace, false).or_fail()?;
        let mut items = collect_quest_api(root.path()).or_fail()?;
        classify_items(&mut items, &registry).or_fail()?;

        let vector = items
            .iter()
            .find(|item| {
                item.name == "applyCompMatr"
                    && item
                        .arguments
                        .iter()
                        .any(|arg| arg.ty.contains("std::vector<int>"))
            })
            .or_fail()?;
        let pointer = items
            .iter()
            .find(|item| {
                item.name == "applyCompMatr"
                    && item.arguments.iter().any(|arg| arg.ty.contains('*'))
            })
            .or_fail()?;

        expect_that!(
            vector.overload_key.as_str(),
            not(eq(pointer.overload_key.as_str()))
        );
        expect_that!(vector.status.as_str(), eq("generated"));
        verify_that!(pointer.status.as_str(), eq("unsupported-c-api"))
    }
}
