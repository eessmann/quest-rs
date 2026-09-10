use googletest::prelude::*;
use quest_language::{
    SourceId, SourceSnapshot,
    semantic::{self, CompileLimits},
    ssa::Type,
    syntax,
};
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
