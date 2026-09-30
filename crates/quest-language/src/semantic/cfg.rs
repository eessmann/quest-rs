//! Reachability cleanup before scalar promotion.
use super::SemanticError;
use crate::ssa::{BlockId, Program, Terminator};
use std::collections::{BTreeMap, BTreeSet};

#[must_use]
pub fn reachable(program: &Program, entry: BlockId) -> BTreeSet<BlockId> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(term) = program
            .blocks
            .get(id.index())
            .and_then(|block| block.terminator.as_ref())
        {
            pending.extend(term.edges().iter().map(|edge| edge.target));
        }
    }
    seen
}
pub fn prune(mut program: Program) -> Result<Program, SemanticError> {
    let reachable = program
        .regions
        .iter()
        .flat_map(|region| reachable(&program, region.entry))
        .collect::<BTreeSet<_>>();
    program.blocks.retain(|block| reachable.contains(&block.id));
    let mapping: BTreeMap<_, _> = program
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, BlockId::new(program.id, index)))
        .collect();
    let mapped = |id: BlockId| {
        mapping
            .get(&id)
            .copied()
            .ok_or_else(|| SemanticError::invalid("reachable edge targets removed block"))
    };
    for region in &mut program.regions {
        region.entry = mapped(region.entry)?;
    }
    for block in &mut program.blocks {
        block.id = mapped(block.id)?;
        block.predecessors = block
            .predecessors
            .iter()
            .filter_map(|id| mapping.get(id).copied())
            .collect();
        if let Some(term) = &mut block.terminator {
            match term {
                Terminator::Jump(edge) => {
                    edge.target = mapped(edge.target)?;
                }
                Terminator::Branch {
                    then_edge,
                    else_edge,
                    ..
                } => {
                    then_edge.target = mapped(then_edge.target)?;
                    else_edge.target = mapped(else_edge.target)?;
                }
                _ => {}
            }
        }
    }
    Ok(program)
}
