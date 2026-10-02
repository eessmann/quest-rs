use googletest::prelude::*;
use quest_compile::{Angle, Constructed, Gate, Program, ProgramBuilder, QuantumRegionBuilder};
use quest_language::{
    GateKind,
    semantic::{CompileLimits, builder::Builder, finite::FiniteOperation as F},
    ssa::InstructionKind,
};

#[gtest]
fn finite_insertion_preserves_exact_angles_without_reconstructed_gate_syntax() -> Result<()> {
    let mut region = QuantumRegionBuilder::new(1, 0)?;
    let q = region.qubit(0)?;
    region.gate(Gate::Rz(Angle::pi(1, 7)?), &[q], &[])?;
    let program = Program::<Constructed>::from_region(region.finish()?, &[])?
        .verify()?
        .lower()?
        .plan()?;
    expect_eq!(program.captures().len(), 1);
    expect_eq!(program.exact_captures().len(), 1);
    expect_true!(
        program
            .syntax()
            .statements
            .iter()
            .any(|s| matches!(s.kind, quest_language::syntax::StatementKind::Finite { .. }))
    );
    expect_eq!(
        program
            .ssa()
            .blocks()
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind, InstructionKind::Capture { .. }))
            .count(),
        1
    );
    Ok(())
}

#[gtest]
fn semantic_fragment_rejects_foreign_handles_bad_indices_shapes_and_aliases() -> Result<()> {
    let mut a = Builder::new()?;
    let mut b = Builder::new()?;
    let foreign = b.qubit("foreign", 1)?;
    expect_true!(a.append_finite_fragment(vec![], &[foreign], &[]).is_err());
    for (width, target) in [(1, 1), (2, 0)] {
        let mut b = Builder::new()?;
        let q = b.qubit("q", width)?;
        b.append_finite_fragment(
            vec![F::Gate {
                gate: GateKind::X,
                arguments: vec![],
                targets: vec![target],
                controls: vec![],
            }],
            &[q],
            &[],
        )?;
        expect_true!(b.finish(CompileLimits::default()).is_err());
    }
    let mut b = Builder::new()?;
    let q = b.qubit("q", 1)?;
    b.append_finite_fragment(
        vec![F::Gate {
            gate: GateKind::X,
            arguments: vec![],
            targets: vec![0],
            controls: vec![(1, true)],
        }],
        &[q.clone(), q],
        &[],
    )?;
    expect_true!(b.finish(CompileLimits::default()).is_err());
    Ok(())
}

#[gtest]
fn scalar_and_oracle_capture_banks_need_no_placeholder_values() -> Result<()> {
    let mut finite = QuantumRegionBuilder::new(1, 0)?;
    finite.gate(Gate::H, &[finite.qubit(0)?], &[])?;
    let oracle = quest_compile::OracleFragment::builder(finite.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let mut b = ProgramBuilder::new()?;
    let q = b.qubit("q", 1)?;
    let definition = b.oracle("oracle", oracle)?;
    let angle = b.angle(Angle::pi(1, 8)?)?;
    b.gate(GateKind::Rz, &[angle], std::slice::from_ref(&q))?;
    b.call_gate(&definition, &[], &[q], &[])?;
    let program = b.finish()?.verify()?.lower()?.plan()?;
    expect_eq!(program.captures().len(), 1);
    expect_eq!(program.oracle_captures().len(), 1);
    expect_eq!(program.exact_captures().len(), 1);
    Ok(())
}

#[gtest]
fn concrete_finite_source_remains_exportable() -> Result<()> {
    let mut b = QuantumRegionBuilder::new(1, 0)?;
    b.gate(Gate::H, &[b.qubit(0)?], &[])?;
    let program = Program::<Constructed>::from_region(b.finish()?, &[])?
        .verify()?
        .lower()?
        .plan()?;
    let source = program.export_source(quest_compile::qasm::ExportLimits::default())?;
    Program::<Constructed>::parse(&source, "finite-export")?.verify()?;
    Ok(())
}

#[cfg(feature = "macros")]
#[gtest]
fn mixed_macro_captures_are_evaluated_once_in_source_order_without_placeholders() -> Result<()> {
    let mut finite = QuantumRegionBuilder::new(1, 0)?;
    finite.gate(Gate::H, &[finite.qubit(0)?], &[])?;
    let oracle = quest_compile::OracleFragment::builder(finite.finish()?.bind(&[])?)
        .matrix_tolerance(1e-12)?
        .build()?;
    let mut visits = Vec::new();
    let program = quest_compile::circuit! {
        oracle body[1] = ${{ visits.push(0); oracle }};
        qubit q;
        rz(${{ visits.push(1); Angle::pi(1, 8)? }}) q;
        body q;
    }?
    .verify()?
    .lower()?
    .plan()?;
    expect_eq!(visits, vec![0, 1]);
    expect_eq!(program.captures().len(), 1);
    expect_eq!(program.exact_captures().len(), 1);
    expect_eq!(program.oracle_captures().len(), 1);
    Ok(())
}
