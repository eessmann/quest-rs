use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::*;

#[gtest]
fn shared_definitions_expand_with_fresh_occurrences_and_ordered_arguments() -> Result<()> {
    let mut body = QuantumRegionBuilder::new(2, 0)?;
    let theta = body.parameter("theta")?;
    body.gate(Gate::Rx(Angle::parameter(theta)?), &[body.qubit(0)?], &[])?;
    body.gate(Gate::Z, &[body.qubit(1)?], &[])?;
    let unitary = body.finish()?.into_unitary()?;
    let mut b = QuantumRegionBuilder::new(4, 0)?;
    let definition = b.define("pair", unitary)?;
    let first = b.call(
        definition,
        &[b.qubit(3)?, b.qubit(0)?],
        &[Angle::pi(1, 2)?],
        &[],
    )?;
    let second = b.call(
        definition,
        &[b.qubit(3)?, b.qubit(0)?],
        &[Angle::pi(1, 2)?],
        &[],
    )?;
    expect_ne!(first, second);
    let plan = b.finish()?.bind(&[])?.plan()?;
    expect_eq!(plan.instructions().len(), 4);
    if let Operation::Gate { targets, .. } = plan.instructions()[0].operation() {
        expect_eq!(targets[0].index(), 3);
    } else {
        fail!("expected gate")?;
    }
    Ok(())
}

#[gtest]
fn failed_definition_expansion_does_not_partially_append() -> Result<()> {
    let mut body = QuantumRegionBuilder::new(1, 0)?;
    let q = body.qubit(0)?;
    body.gate(Gate::X, &[q], &[])?;
    body.gate(Gate::Z, &[q], &[])?;
    let mut b = QuantumRegionBuilder::with_limits(
        1,
        0,
        ProgramLimits {
            max_operations: 1,
            ..Default::default()
        },
    )?;
    let d = b.define("pair", body.finish()?.into_unitary()?)?;
    expect_true!(b.call(d, &[b.qubit(0)?], &[], &[]).is_err());
    expect_eq!(b.finish()?.schedule().len(), 0);
    Ok(())
}

#[gtest]
fn definition_expansion_preserves_explicit_body_dependencies() -> Result<()> {
    let mut body = QuantumRegionBuilder::new(2, 0)?;
    let a = body.gate(Gate::H, &[body.qubit(0)?], &[])?;
    let z = body.gate(Gate::Z, &[body.qubit(1)?], &[])?;
    body.depend(z, a)?;
    let mut caller = QuantumRegionBuilder::new(2, 0)?;
    let definition = caller.define("ordered", body.finish()?.into_unitary()?)?;
    let ids = caller.call(definition, &[caller.qubit(0)?, caller.qubit(1)?], &[], &[])?;
    let p = caller.finish()?;
    expect_true!(p.has_dependency(ids[0], ids[1])?);
    expect_eq!(p.dependency_depth(), 2);
    Ok(())
}

#[gtest]
fn adjoint_reverses_explicit_edges_without_serializing_independent_gates() -> Result<()> {
    let mut body = QuantumRegionBuilder::new(3, 0)?;
    let a = body.gate(Gate::H, &[body.qubit(0)?], &[])?;
    let z = body.gate(Gate::Z, &[body.qubit(1)?], &[])?;
    body.gate(Gate::X, &[body.qubit(2)?], &[])?;
    body.depend(a, z)?;
    let inverse = body.finish()?.into_unitary()?.adjoint()?.into_program();
    expect_true!(inverse.has_dependency(z, a)?);
    expect_eq!(inverse.dependency_depth(), 2);
    Ok(())
}
