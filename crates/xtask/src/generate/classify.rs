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

    #[test]
    fn generated_classification_requires_exact_overload_key() {
        let mut items = vec![fake_apply_comp_matr_item(
            "applyCompMatr(Qureg, int, CompMatr) -> void",
        )];

        classify_items(&mut items, &AdapterRegistry::empty())
            .expect("classification should be exhaustive");

        assert_eq!(items[0].status, "gated-manual-adapter-needed");
    }

    #[test]
    fn exact_adapter_entry_marks_only_that_overload_generated() {
        let registry = AdapterRegistry::new(vec![AdapterEntry {
            overload_key: "applyCompMatr(Qureg, std::vector<int>, CompMatr) -> void".to_owned(),
            quest_name: "applyCompMatr".to_owned(),
            adapter_name: "applyCompMatr".to_owned(),
            rust_name: "apply_comp_matr".to_owned(),
            source_kind: AdapterSourceKind::Core,
        }])
        .expect("registry");
        let mut items = vec![
            fake_apply_comp_matr_item("applyCompMatr(Qureg, int, CompMatr) -> void"),
            fake_apply_comp_matr_item("applyCompMatr(Qureg, std::vector<int>, CompMatr) -> void"),
        ];

        classify_items(&mut items, &registry).expect("classification should be exhaustive");

        assert_eq!(items[0].status, "gated-manual-adapter-needed");
        assert_eq!(items[1].status, "generated");
    }

    #[test]
    fn classification_distinguishes_overloads_by_signature_shape() {
        let Ok(root) = find_quest_root() else {
            eprintln!("skipping test because no QuEST root was provided by environment");
            return;
        };

        let workspace = find_workspace_root().expect("workspace");
        let registry = load_adapter_registry(&workspace, false).expect("adapter registry");
        let mut items =
            collect_quest_api(root.path()).expect("libclang should parse QuEST headers");
        classify_items(&mut items, &registry).expect("classification should be exhaustive");

        let vector = items
            .iter()
            .find(|item| {
                item.name == "applyCompMatr"
                    && item
                        .arguments
                        .iter()
                        .any(|arg| arg.ty.contains("std::vector<int>"))
            })
            .expect("vector overload");
        let pointer = items
            .iter()
            .find(|item| {
                item.name == "applyCompMatr"
                    && item.arguments.iter().any(|arg| arg.ty.contains('*'))
            })
            .expect("pointer overload");

        assert_ne!(vector.overload_key, pointer.overload_key);
        assert_eq!(vector.status, "generated");
        assert_eq!(pointer.status, "unsupported-c-api");
    }
}
