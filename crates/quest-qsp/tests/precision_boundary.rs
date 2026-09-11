//! Architectural regression guard for the binary64 execution boundary. Together
//! with the feature-disabled build, this rules out cold-backend dispatch from
//! production kernels. It is not an allocation benchmark for the cold backend.
use googletest::prelude::*;
use std::path::Path;

#[gtest]
fn production_kernels_and_native_execution_cannot_dispatch_to_the_cold_backend() -> Result<()> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for module in ["admission.rs", "kernel.rs", "sequence.rs", "stages.rs"] {
        let text = std::fs::read_to_string(crate_root.join("src").join(module))?;
        for forbidden in [
            "astro_float",
            "crate::precision",
            "crate::certification",
            "crate::offline",
        ] {
            expect_false!(
                text.contains(forbidden),
                "{module} admits cold arithmetic through {forbidden}"
            );
        }
    }
    Ok(())
}
