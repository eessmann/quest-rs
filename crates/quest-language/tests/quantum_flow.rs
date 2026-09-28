use googletest::prelude::*;
use quest_language::{
    SourceId, SourceSnapshot,
    semantic::{CompileLimits, admit},
    ssa::{AliasRelation, InstructionKind, Place, QuantumFlow, QuantumFlowLimits, Type},
    syntax::parse_source,
};

fn compile(
    text: &str,
) -> std::result::Result<quest_language::ssa::VerifiedProgram, std::io::Error> {
    let source = SourceSnapshot::new(SourceId::new(9), "flow", text);
    let parse = parse_source(&source).map_err(|error| std::io::Error::other(error.to_string()))?;
    let admitted = admit(parse, CompileLimits::default())
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    admitted
        .into_ssa()
        .map_err(|error| std::io::Error::other(error.to_string()))
}

#[gtest]
fn static_wires_have_distinct_versions_and_dynamic_index_clobbers_its_root() -> Result<()> {
    let program = compile("qubit[2] q; input int i; x q[0]; h q[1]; x q[i]; z q[0];")?;
    let flow = QuantumFlow::analyze(&program, QuantumFlowLimits::default())?;
    let block = program
        .blocks()
        .iter()
        .find(|block| {
            block
                .instructions
                .iter()
                .filter(|item| matches!(item.kind, InstructionKind::Gate { .. }))
                .count()
                == 4
        })
        .ok_or_else(|| std::io::Error::other("missing gate block"))?;
    let handle = program
        .block_handle(block.id)
        .ok_or_else(|| std::io::Error::other("missing block handle"))?;
    let events = block
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item.kind, InstructionKind::Gate { .. }))
        .map(|(position, _)| flow.event(handle, position).expect("gate event"))
        .collect::<Vec<_>>();
    verify_eq!(events.len(), 4)?;
    verify_ne!(events[0].outputs[0], events[1].outputs[0])?;
    let static_output = |event: &quest_language::ssa::QuantumEvent, index| {
        event
            .outputs
            .iter()
            .copied()
            .find(|version| {
                flow.fact(*version)
                    .is_some_and(|fact| fact.storage.index == Some(index))
            })
            .expect("static output")
    };
    verify_that!(
        events[2].inputs.contains(&static_output(events[0], 0)),
        eq(true)
    )?;
    verify_that!(
        events[2].inputs.contains(&static_output(events[1], 1)),
        eq(true)
    )?;
    let clobbered_zero = events[2]
        .outputs
        .iter()
        .copied()
        .find(|version| {
            flow.fact(*version)
                .is_some_and(|fact| fact.storage.index == Some(0))
        })
        .ok_or_else(|| std::io::Error::other("missing clobbered zero"))?;
    verify_that!(events[3].inputs.contains(&clobbered_zero), eq(true))?;
    let places = block
        .instructions
        .iter()
        .filter_map(|item| match &item.kind {
            InstructionKind::Gate { operands, .. } => operands.first(),
            _ => None,
        })
        .collect::<Vec<_>>();
    verify_eq!(
        flow.alias(handle, places[0], places[1]),
        AliasRelation::Disjoint
    )?;
    verify_eq!(
        flow.alias(handle, places[0], places[2]),
        AliasRelation::MayAlias
    )?;
    verify_eq!(
        flow.alias(handle, places[0], places[3]),
        AliasRelation::Same
    )?;
    Ok(())
}

#[gtest]
fn branches_and_backedges_have_finite_edge_tagged_versions() -> Result<()> {
    let program = compile(
        "input bool flag; qubit q; int n=2; if(flag) { x q; } else { h q; } while(n>0) { z q; n-=1; }",
    )?;
    let flow = QuantumFlow::analyze(&program, QuantumFlowLimits::default())?;
    verify_eq!(flow.snapshot(), program.snapshot())?;
    verify_that!(
        flow.edges().iter().filter(|edge| edge.backedge).count(),
        ge(1)
    )?;
    verify_that!(
        flow.edges()
            .iter()
            .filter(|edge| edge.arm.is_some())
            .count(),
        ge(2)
    )?;
    verify_that!(flow.facts().len(), lt(1000))?;
    let other = compile("qubit q; x q;")?;
    let foreign = other
        .block_handle(other.blocks()[0].id)
        .ok_or_else(|| std::io::Error::other("missing foreign block"))?;
    verify_that!(flow.event(foreign, 0), none())?;
    verify_that!(
        QuantumFlowLimits::new(0, 200_000, 1_000_000, 10_000_000, 64 * 1024 * 1024),
        err(anything())
    )?;
    let tiny = QuantumFlowLimits::new(1_000_000, 200_000, 1_000_000, 10_000_000, 1)?;
    verify_that!(QuantumFlow::analyze(&program, tiny), err(anything()))?;
    verify_that!(flow.usage().work, ge(flow.usage().alias_comparisons))?;
    verify_eq!(flow.usage().facts, flow.facts().len())?;
    Ok(())
}

#[gtest]
fn coupled_gate_outputs_depend_on_the_complete_input_tuple() -> Result<()> {
    let program = compile("qubit[2] q; h q[0]; x q[1]; cx q[0], q[1];")?;
    let flow = QuantumFlow::analyze(&program, QuantumFlowLimits::default())?;
    let block = program
        .blocks()
        .iter()
        .find(|block| {
            block
                .instructions
                .iter()
                .filter(|item| matches!(item.kind, InstructionKind::Gate { .. }))
                .count()
                == 3
        })
        .ok_or_else(|| std::io::Error::other("missing gate block"))?;
    let handle = program
        .block_handle(block.id)
        .ok_or_else(|| std::io::Error::other("missing block handle"))?;
    let events = block
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item.kind, InstructionKind::Gate { .. }))
        .map(|(position, _)| flow.event(handle, position).expect("gate event"))
        .collect::<Vec<_>>();
    verify_eq!(events[2].outputs.len(), 2)?;
    for output in &events[2].outputs {
        let fact = flow
            .fact(*output)
            .ok_or_else(|| std::io::Error::other("missing output fact"))?;
        verify_eq!(&fact.inputs, &events[2].inputs)?;
    }
    Ok(())
}

#[gtest]
fn fact_alias_work_and_edge_caps_fail_before_unbounded_growth() -> Result<()> {
    let program = compile("qubit[2] q; h q[0]; x q[1]; cx q[0], q[1];")?;
    for limits in [
        QuantumFlowLimits::new(1, 200_000, 1_000_000, 10_000_000, 64 * 1024 * 1024)?,
        QuantumFlowLimits::new(1_000_000, 200_000, 1, 10_000_000, 64 * 1024 * 1024)?,
        QuantumFlowLimits::new(1_000_000, 200_000, 1_000_000, 1, 64 * 1024 * 1024)?,
    ] {
        verify_that!(QuantumFlow::analyze(&program, limits), err(anything()))?;
    }
    let branch = compile("input bool flag; qubit q; if(flag) { x q; } else { h q; }")?;
    let one_edge = QuantumFlowLimits::new(1_000_000, 1, 1_000_000, 10_000_000, 64 * 1024 * 1024)?;
    verify_that!(QuantumFlow::analyze(&branch, one_edge), err(anything()))?;
    Ok(())
}

#[gtest]
fn alias_query_requires_same_snapshot_and_region() -> Result<()> {
    let program = compile("def flip(qubit q) { x q; } qubit q; flip(q);")?;
    let flow = QuantumFlow::analyze(&program, QuantumFlowLimits::default())?;
    let entry = program
        .regions()
        .iter()
        .find(|region| region.id == program.program().entry)
        .ok_or_else(|| std::io::Error::other("missing entry"))?;
    let handle = program
        .block_handle(entry.entry)
        .ok_or_else(|| std::io::Error::other("missing entry handle"))?;
    let caller = program
        .slots()
        .iter()
        .find(|slot| slot.region == entry.id && matches!(slot.ty, Type::Qubit(_)))
        .ok_or_else(|| std::io::Error::other("missing caller slot"))?;
    let callee = program
        .slots()
        .iter()
        .find(|slot| slot.region != entry.id && matches!(slot.ty, Type::Qubit(_)))
        .ok_or_else(|| std::io::Error::other("missing callee slot"))?;
    let caller = Place {
        slot: caller.id,
        indices: vec![],
    };
    let callee = Place {
        slot: callee.id,
        indices: vec![],
    };
    verify_eq!(flow.alias(handle, &caller, &caller), AliasRelation::Same)?;
    verify_eq!(
        flow.alias(handle, &caller, &callee),
        AliasRelation::MayAlias
    )?;
    let foreign = compile("qubit q; x q;")?;
    let foreign_handle = foreign
        .block_handle(foreign.blocks()[0].id)
        .ok_or_else(|| std::io::Error::other("missing foreign handle"))?;
    verify_eq!(
        flow.alias(foreign_handle, &caller, &caller),
        AliasRelation::MayAlias
    )?;
    Ok(())
}
