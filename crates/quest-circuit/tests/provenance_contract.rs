use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::*;

fn rotations(count: usize) -> quest_circuit::Result<(QuantumRegion, Vec<OccurrenceId>)> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    let mut sources = Vec::new();
    for _ in 0..count {
        sources.push(builder.gate(Gate::Rz(Angle::pi(1, 7)?), &[q], &[])?);
    }
    Ok((builder.finish()?, sources))
}

#[gtest]
fn long_merge_history_is_linear_and_expands_every_source_without_recursion() -> Result<()> {
    let (input, sources) = rotations(4096)?;
    let (output, report) = input.optimize_exact_with_options(ExactOptions {
        max_work: 50_000,
        max_bytes: 2 * 1024 * 1024,
    })?;
    expect_eq!(output.schedule().len(), 1);
    expect_eq!(report.provenance.node_count(), 8191);
    expect_eq!(report.provenance.edge_count(), 8190);
    expect_lt!(report.provenance.retained_bytes()?, 1024 * 1024);
    let bound = output.bind(&[])?;
    let root = bound.instructions()[0].provenance();
    expect_eq!(
        bound
            .provenance()
            .source_leaves(root, ExpansionLimits::default())?,
        sources
    );
    for limits in [
        ExpansionLimits {
            max_work: 1,
            ..ExpansionLimits::default()
        },
        ExpansionLimits {
            max_leaves: 4095,
            ..ExpansionLimits::default()
        },
        ExpansionLimits {
            max_bytes: 1,
            ..ExpansionLimits::default()
        },
    ] {
        expect_true!(bound.provenance().source_leaves(root, limits).is_err());
    }
    Ok(())
}

#[gtest]
fn exact_rewrite_work_and_history_storage_budgets_reject_without_publishing() -> Result<()> {
    let (input, _) = rotations(32)?;
    expect_true!(
        input
            .clone()
            .optimize_exact_with_options(ExactOptions {
                max_work: 32,
                max_bytes: 1024 * 1024,
            })
            .is_err()
    );
    expect_true!(
        input
            .clone()
            .optimize_exact_with_options(ExactOptions {
                max_work: 10_000,
                max_bytes: 1,
            })
            .is_err()
    );
    expect_eq!(input.schedule().len(), 32);
    let (_, report) = input.optimize_exact()?;
    expect_eq!(report.after_operations, 1);
    Ok(())
}

#[gtest]
fn diverging_snapshot_edits_cannot_exchange_history_roots() -> Result<()> {
    let (input, _) = rotations(3)?;
    let left = input.clone().optimize_exact()?.0.bind(&[])?;
    let right = input.optimize_exact()?.0.bind(&[])?;
    let left_root = left.instructions()[0].provenance();
    let right_root = right.instructions()[0].provenance();
    expect_ne!(left_root, right_root);
    expect_true!(
        left.provenance()
            .source_leaves(right_root, ExpansionLimits::default())
            .is_err()
    );
    expect_true!(
        right
            .provenance()
            .source_leaves(left_root, ExpansionLimits::default())
            .is_err()
    );
    Ok(())
}

#[gtest]
fn exact_then_fusion_preserves_history_and_current_dependency_endpoints() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(2, 0)?;
    let q = builder.qubit(0)?;
    let mut sources = Vec::new();
    for _ in 0..4 {
        sources.push(builder.gate(Gate::Rz(Angle::pi(1, 7)?), &[q], &[])?);
    }
    sources.push(builder.gate(Gate::H, &[q], &[])?);
    sources.push(builder.gate(Gate::X, &[q], &[])?);
    let (exact, _) = builder.finish()?.optimize_exact()?;
    let (fused, report) = exact.bind(&[])?.fuse(FusionOptions::default())?;
    expect_eq!(fused.instructions().len(), 1);
    expect_eq!(fused.dependency_depth(), 1);
    let root = fused.instructions()[0].provenance();
    expect_eq!(
        report
            .provenance
            .source_leaves(root, ExpansionLimits::default())?,
        sources
    );
    let plan = fused.plan()?;
    expect_eq!(
        plan.provenance()
            .source_leaves(root, ExpansionLimits::default())?,
        sources
    );
    Ok(())
}

#[gtest]
fn parity_outputs_share_history_and_later_fusion_deduplicates_sources() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let q = builder.qubit(0)?;
    let mut sources = Vec::new();
    for _ in 0..4 {
        sources.push(builder.gate(Gate::Phase(Angle::pi(1, 4)?), &[q], &[])?);
    }
    sources.push(builder.gate(Gate::X, &[q], &[])?);
    let (parity, report) = builder
        .finish()?
        .optimize_parity(ParityOptions::default())?;
    expect_eq!(report.accepted_windows, 1);
    let bound = parity.bind(&[])?;
    expect_eq!(bound.instructions().len(), 2);
    for item in bound.instructions() {
        expect_eq!(
            bound
                .provenance()
                .source_leaves(item.provenance(), ExpansionLimits::default())?,
            sources
        );
    }
    let (fused, _) = bound.fuse(FusionOptions::default())?;
    expect_eq!(fused.instructions().len(), 1);
    expect_eq!(
        fused.provenance().source_leaves(
            fused.instructions()[0].provenance(),
            ExpansionLimits::default()
        )?,
        sources
    );
    Ok(())
}

#[gtest]
fn empty_replacements_retain_distinct_rewrite_events() -> Result<()> {
    let mut builder = QuantumRegionBuilder::new(1, 0)?;
    let source = builder.gate(Gate::Id, &[builder.qubit(0)?], &[])?;
    let (output, report) = builder.finish()?.optimize_exact()?;
    expect_true!(output.schedule().is_empty());
    let rewrite = &report.rewrites[0];
    expect_ne!(rewrite.provenance, rewrite.inputs[0]);
    expect_true!(matches!(
        report.provenance.node(rewrite.provenance)?,
        ProvenanceNode::Rewrite(_)
    ));
    expect_eq!(
        report
            .provenance
            .source_leaves(rewrite.provenance, ExpansionLimits::default())?,
        vec![source]
    );
    Ok(())
}

#[gtest]
fn composed_passes_budget_retained_history_even_with_one_surviving_operation() -> Result<()> {
    let (input, _) = rotations(128)?;
    let (merged, _) = input.optimize_exact()?;
    expect_eq!(merged.schedule().len(), 1);
    expect_true!(
        merged
            .clone()
            .optimize_exact_with_options(ExactOptions {
                max_work: 128,
                ..ExactOptions::default()
            })
            .is_err()
    );
    let bound = merged.bind(&[])?;
    expect_true!(
        bound
            .clone()
            .fuse(FusionOptions {
                max_provenance_work: 128,
                ..FusionOptions::default()
            })
            .is_err()
    );
    expect_true!(
        bound
            .fuse(FusionOptions {
                max_provenance_bytes: 128,
                ..FusionOptions::default()
            })
            .is_err()
    );
    Ok(())
}

#[gtest]
fn definition_history_budget_rejection_keeps_the_builder_transactional() -> Result<()> {
    let (reference, _) = rotations(4)?;
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    for _ in 0..3 {
        body.gate(Gate::H, &[body.qubit(0)?], &[])?;
    }
    let mut builder = QuantumRegionBuilder::with_limits(
        1,
        0,
        ProgramLimits {
            max_provenance_bytes: reference.provenance().retained_bytes()?,
            ..ProgramLimits::default()
        },
    )?;
    let definition = builder.define("triple", body.finish()?.into_unitary()?)?;
    let q = builder.qubit(0)?;
    let ids = builder.call(definition, &[q], &[], &[])?;
    expect_true!(builder.call(definition, &[q], &[], &[]).is_err());
    let tail = builder.gate(Gate::X, &[q], &[])?;
    expect_eq!(tail.index(), 3);
    let program = builder.finish()?.bind(&[])?;
    expect_eq!(program.instructions().len(), 4);
    expect_eq!(program.provenance().node_count(), 4);
    let expected = ids.into_iter().chain([tail]);
    for (item, source) in program.instructions().iter().zip(expected) {
        expect_eq!(
            program
                .provenance()
                .source_leaves(item.provenance(), ExpansionLimits::default())?,
            vec![source]
        );
    }
    Ok(())
}
