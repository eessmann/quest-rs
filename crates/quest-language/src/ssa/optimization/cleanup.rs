use super::{Budget, OptimizationReport, Program, SemanticError, scalar_equal};
use crate::{
    classical::ScalarType,
    semantic::promote::{rewrite_kind, rewrite_terminator},
    ssa::{Effect, InstructionKind as K, Type, ValueId},
    syntax::{BinaryOperator as B, UnaryOperator as U},
};
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn predecessors(program: &mut Program) -> Result<(), SemanticError> {
    let edges = program
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .terminator
                .iter()
                .flat_map(|term| term.edges())
                .map(move |edge| (block.id, edge.target))
        })
        .collect::<Vec<_>>();
    for block in &mut program.blocks {
        block.predecessors.clear();
    }
    for (source, target) in edges {
        let block = program
            .blocks
            .get_mut(target.index())
            .ok_or_else(|| SemanticError::invalid("optimizer edge target missing"))?;
        if !block.predecessors.contains(&source) {
            block.predecessors.push(source);
        }
    }
    Ok(())
}
const fn eligible(kind: &K) -> bool {
    matches!(
        kind,
        K::Constant(_)
            | K::Unary { .. }
            | K::Binary { .. }
            | K::Cast { .. }
            | K::GateParameter { .. }
            | K::Builtin { .. }
    )
}
fn same(left: &K, right: &K) -> bool {
    match (left, right) {
        (K::Constant(left), K::Constant(right)) => scalar_equal(*left, *right),
        _ => left == right,
    }
}
pub(super) fn common_expressions(
    program: &mut Program,
    budget: &mut Budget,
    report: &mut OptimizationReport,
) -> Result<(), SemanticError> {
    let mut replacements = BTreeMap::new();
    for block in &mut program.blocks {
        let mut available = Vec::<(K, ValueId)>::new();
        let mut retained = Vec::new();
        for mut item in std::mem::take(&mut block.instructions) {
            budget.tick()?;
            rewrite_kind(&mut item.kind, &replacements)?;
            item.accesses = item.kind.accesses();
            if eligible(&item.kind) && item.results.len() == 1 {
                let mut found = None;
                for (kind, value) in &available {
                    budget.tick()?;
                    if same(kind, &item.kind) {
                        found = Some(*value);
                        break;
                    }
                }
                if let Some(value) = found {
                    let result = item
                        .results
                        .first()
                        .ok_or_else(|| SemanticError::invalid("CSE result missing"))?;
                    replacements.insert(result.id, value);
                    report.common_expressions_removed =
                        report.common_expressions_removed.saturating_add(1);
                    continue;
                }
                if let Some(result) = item.results.first() {
                    available.push((item.kind.clone(), result.id));
                }
            }
            retained.push(item);
        }
        block.instructions = retained;
    }
    // Block-local reuse is dominated; this final rewrite updates uses in successor blocks.
    for block in &mut program.blocks {
        for item in &mut block.instructions {
            budget.tick()?;
            rewrite_kind(&mut item.kind, &replacements)?;
            item.accesses = item.kind.accesses();
        }
        if let Some(term) = &mut block.terminator {
            rewrite_terminator(term, &replacements)?;
        }
    }
    Ok(())
}
fn nontrapping(kind: &K, types: &BTreeMap<ValueId, Type>) -> bool {
    let boolean = |id: &ValueId| types.get(id) == Some(&Type::Scalar(ScalarType::Bool));
    match kind {
        K::Constant(_) | K::GateParameter { .. } => true,
        K::Unary {
            operator: U::Not,
            value,
        } => boolean(value),
        K::Binary {
            operator: B::And | B::Or | B::Equal | B::NotEqual,
            left,
            right,
        } => boolean(left) && boolean(right),
        K::Cast { value, ty } => types.get(value) == Some(&Type::Scalar(*ty)),
        _ => false,
    }
}
pub(super) fn dead_instructions(
    program: &mut Program,
    budget: &mut Budget,
    report: &mut OptimizationReport,
) -> Result<(), SemanticError> {
    let mut definitions = BTreeMap::new();
    let mut types = BTreeMap::new();
    for (block_index, block) in program.blocks.iter().enumerate() {
        for value in &block.arguments {
            types.insert(value.id, value.ty.clone());
        }
        for (index, item) in block.instructions.iter().enumerate() {
            budget.tick()?;
            for result in &item.results {
                definitions.insert(result.id, (block_index, index));
                types.insert(result.id, result.ty.clone());
            }
        }
    }
    let mut pending = Vec::new();
    let mut live = BTreeSet::new();
    for (block_index, block) in program.blocks.iter().enumerate() {
        if let Some(term) = &block.terminator {
            pending.extend(term.operands());
        }
        for (index, item) in block.instructions.iter().enumerate() {
            budget.tick()?;
            if item.effect != Effect::Pure || !nontrapping(&item.kind, &types) {
                live.insert((block_index, index));
                pending.extend(item.kind.operands());
            }
        }
    }
    let mut seen = BTreeSet::new();
    while let Some(value) = pending.pop() {
        budget.tick()?;
        if !seen.insert(value) {
            continue;
        }
        if let Some(&(block_index, index)) = definitions.get(&value)
            && live.insert((block_index, index))
        {
            let item = program
                .blocks
                .get(block_index)
                .and_then(|block| block.instructions.get(index))
                .ok_or_else(|| SemanticError::invalid("dead analysis definition missing"))?;
            pending.extend(item.kind.operands());
        }
    }
    for (block_index, block) in program.blocks.iter_mut().enumerate() {
        let mut index = 0usize;
        block.instructions.retain(|_| {
            let keep = live.contains(&(block_index, index));
            index = index.saturating_add(1);
            if !keep {
                report.dead_instructions_removed =
                    report.dead_instructions_removed.saturating_add(1);
            }
            keep
        });
    }
    Ok(())
}
