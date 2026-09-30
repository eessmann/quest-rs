#[allow(unused_imports)]
use crate::{
    BoundParityPasses, ExactPasses, LinearPasses, NumericalPasses, OracleExport, ParityPasses,
    TerminalPasses,
};
pub use quest_language::quantum::program::*;

#[cfg(test)]
mod snapshot_tests {
    use crate::*;
    use googletest::{Result, prelude::*};
    #[gtest]
    fn independently_published_rewrites_get_distinct_snapshot_tokens() -> Result<()> {
        let mut builder = QuantumRegionBuilder::new(1, 0)?;
        let qubit = builder.qubit(0)?;
        builder.gate(Gate::H, &[qubit], &[])?;
        builder.gate(Gate::H, &[qubit], &[])?;
        let original = builder.finish()?;
        let clone = original.clone();
        expect_eq!(original.snapshot_id(), clone.snapshot_id());
        let left = clone.optimize_exact()?.0;
        let right = original.clone().optimize_exact()?.0;
        expect_ne!(left.snapshot_id(), right.snapshot_id());
        expect_ne!(left.snapshot_id(), original.snapshot_id());
        let first_bound = original.clone().bind(&[])?;
        let second_bound = original.clone().bind(&[])?;
        expect_eq!(first_bound.source_snapshot_id(), original.snapshot_id());
        expect_ne!(first_bound.snapshot_id(), second_bound.snapshot_id());
        let bound_clone = first_bound.clone();
        expect_eq!(first_bound.snapshot_id(), bound_clone.snapshot_id());
        let fused = bound_clone.fuse(crate::FusionOptions::default())?.0;
        expect_ne!(first_bound.snapshot_id(), fused.snapshot_id());
        expect_eq!(first_bound.source_snapshot_id(), fused.source_snapshot_id());
        Ok(())
    }
}
