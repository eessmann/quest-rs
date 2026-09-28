use googletest::prelude::*;
use quest_language::{
    SourceId, SourceSnapshot,
    semantic::{self, CompileLimits},
    ssa::Type,
    syntax,
};

#[gtest]
fn verified_snapshots_reject_handles_from_other_edit_publications() -> Result<()> {
    let source = SourceSnapshot::new(SourceId::new(1), "snapshot.qasm", "qubit q; h q;");
    let typed = semantic::admit(syntax::parse_source(&source)?, CompileLimits::default())?;
    let original = typed.clone().into_ssa()?;
    let transferred = typed.into_ssa()?;
    expect_eq!(original.snapshot(), transferred.snapshot());
    let block = original
        .blocks()
        .first()
        .ok_or_else(|| std::io::Error::other("entry"))?;
    let handle = original
        .block_handle(block.id)
        .ok_or_else(|| std::io::Error::other("handle"))?;
    expect_true!(original.clone().block(handle).is_some());
    let replacement = original
        .clone()
        .into_unverified()
        .verify(CompileLimits::default())?;
    expect_ne!(replacement.snapshot(), original.snapshot());
    expect_true!(replacement.block(handle).is_none());
    Ok(())
}
#[gtest]
fn allocated_ids_are_owned_unique_and_carry_no_publication_proof() -> Result<()> {
    let source = SourceSnapshot::new(SourceId::new(1), "edit.qasm", "qubit q; h q;");
    let original = semantic::admit(syntax::parse_source(&source)?, CompileLimits::default())?
        .into_ssa()?
        .into_unverified();
    let mut allocator = original.value_allocator(CompileLimits::default())?;
    let a = allocator.allocate(Type::Memory)?;
    let b = allocator.allocate(Type::Memory)?;
    expect_eq!(a.id.owner(), original.id);
    expect_ne!(a.id, b.id);
    let mut broken = original.clone();
    broken
        .blocks
        .first_mut()
        .expect("entry block")
        .instructions
        .last_mut()
        .expect("quantum instruction")
        .results
        .push(a);
    expect_true!(broken.verify(CompileLimits::default()).is_err());
    expect_true!(original.verify(CompileLimits::default()).is_ok());
    Ok(())
}

#[gtest]
fn synthetic_oracle_region_is_bounded_owned_and_independently_verified() -> Result<()> {
    let source = SourceSnapshot::new(SourceId::new(8), "oracle-edit.qasm", "qubit q; h q;");
    let mut program = semantic::admit(syntax::parse_source(&source)?, CompileLimits::default())?
        .into_ssa()?
        .into_unverified();
    let mut values = program.value_allocator(CompileLimits::default())?;
    expect_true!(
        program
            .append_oracle_region(5, 0, &mut values, CompileLimits::default())
            .is_err()
    );
    let region = program.append_oracle_region(5, 2, &mut values, CompileLimits::default())?;
    expect_eq!(region.owner(), program.id);
    expect_true!(
        program
            .append_oracle_region(5, 2, &mut values, CompileLimits::default())
            .is_err()
    );
    let verified = program.verify(CompileLimits::default())?;
    expect_eq!(
        verified
            .regions()
            .last()
            .ok_or_else(|| std::io::Error::other("region"))?
            .parameters
            .len(),
        2
    );
    Ok(())
}

#[gtest]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Independent node count over a fixed four-instruction fixture"
)]
fn synthetic_oracle_checks_all_existing_nodes_before_mutation() -> Result<()> {
    let source = SourceSnapshot::new(
        SourceId::new(9),
        "node-budget.qasm",
        "qubit q; h q; x q; t q;",
    );
    let mut program = semantic::admit(syntax::parse_source(&source)?, CompileLimits::default())?
        .into_ssa()?
        .into_unverified();
    let mut values = program.value_allocator(CompileLimits::default())?;
    let nodes = program.regions.len()
        + program
            .blocks
            .iter()
            .map(|block| {
                block.arguments.len()
                    + block.instructions.len()
                    + block
                        .instructions
                        .iter()
                        .map(|item| item.results.len())
                        .sum::<usize>()
            })
            .sum::<usize>();
    let before = program.clone();
    expect_true!(
        program
            .append_oracle_region(
                9,
                1,
                &mut values,
                CompileLimits {
                    nodes,
                    ..CompileLimits::default()
                }
            )
            .is_err()
    );
    expect_eq!(&program, &before);
    program.append_oracle_region(
        9,
        1,
        &mut values,
        CompileLimits {
            nodes: nodes + 2,
            ..CompileLimits::default()
        },
    )?;
    program.verify(CompileLimits {
        nodes: nodes + 2,
        ..CompileLimits::default()
    })?;
    Ok(())
}
