//! Workspace-only precision dependency regression checks.
use googletest::prelude::*;
use std::path::Path;

fn check_sources(path: &Path, arithmetic_root: &Path) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            check_sources(&path, arithmetic_root)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path)?;
            for forbidden in [
                "dashu_float::",
                "crate::precision",
                "crate::certification",
                "crate::offline",
            ] {
                // MP point/interval arithmetic is now a first-class numerics
                // capability. The independent QSP verifier remains separate.
                let admitted_mp = forbidden == "dashu_float::"
                    && (path == arithmetic_root.with_extension("rs")
                        || path.starts_with(arithmetic_root));
                expect_false!(
                    !admitted_mp && text.contains(forbidden),
                    "{} admits cold arithmetic through {forbidden}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

#[gtest]
fn numerical_arithmetic_and_native_execution_keep_precision_boundaries() -> Result<()> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The generic arithmetic module owns point/enclosure MP. Other numerical
    // kernels use its statically dispatched capabilities; native execution
    // consumes frozen binary64 controls. None imports QSP verifier arithmetic.
    let arithmetic_root = crate_root.join("../quest-numerics/src/arithmetic");
    for directory in [
        "../quest-numerics/src",
        "../quest-polynomial/src",
        "../quest/src/qsvt",
    ] {
        check_sources(&crate_root.join(directory), &arithmetic_root)?;
    }
    Ok(())
}

#[gtest]
fn packaged_high_degree_qsp_fixture_matches_the_canonical_catalog() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let canonical =
        std::fs::read(root.join("../quest-qsvt-io/data/inverse/coeffs_kappa_1500_eps_0p001.bin"))?;
    let packaged = std::fs::read(root.join("../quest-qsp/tests/data/inverse-degree-8105.bin"))?;
    expect_eq!(packaged, canonical);
    Ok(())
}
