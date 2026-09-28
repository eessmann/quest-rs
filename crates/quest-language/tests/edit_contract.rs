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
