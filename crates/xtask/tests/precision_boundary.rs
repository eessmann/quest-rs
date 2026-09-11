//! Workspace-only precision dependency regression checks.
use googletest::prelude::*;
use std::path::Path;

fn check_sources(path: &Path) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            check_sources(&path)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path)?;
            for forbidden in [
                "astro_float",
                "crate::precision",
                "crate::certification",
                "crate::offline",
            ] {
                expect_false!(
                    text.contains(forbidden),
                    "{} admits cold arithmetic through {forbidden}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

#[gtest]
fn native_and_binary64_kernels_have_no_cold_backend_entrypoint() -> Result<()> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // These crates are upstream of the optional QSP backend, or consume only
    // frozen binary64 controls. None may acquire a precision dependency.
    for directory in [
        "../quest-numerics/src",
        "../quest-polynomial/src",
        "../quest/src/qsvt",
    ] {
        check_sources(&crate_root.join(directory))?;
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
