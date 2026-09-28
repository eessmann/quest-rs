use super::{Block, BlockId, Program, RegionId, Type, ValueId};
use crate::semantic::retained::Heap as _;
use crate::semantic::{CompileLimits, SemanticError, storage_size};
use std::collections::{BTreeMap, BTreeSet};
mod assignment;
mod dominance;
mod instruction;
mod interfaces;
use dominance::Dominance;

pub(super) struct Definition {
    pub block: BlockId,
    pub order: usize,
    pub ty: Type,
}
pub(super) struct Context<'a> {
    pub program: &'a Program,
    pub values: BTreeMap<ValueId, Definition>,
    pub constants: BTreeMap<ValueId, crate::classical::ScalarValue>,
}
impl Context<'_> {
    pub fn ty(&self, value: ValueId) -> Result<&Type, SemanticError> {
        self.values
            .get(&value)
            .map(|definition| &definition.ty)
            .ok_or_else(|| SemanticError::invalid("unknown or foreign SSA value"))
    }
}
pub(super) fn verify(program: &Program, limits: CompileLimits) -> Result<(), SemanticError> {
    resources(program, limits)?;
    let context = definitions(program)?;
    interfaces::verify(program)?;
    let predecessors = edges(&context)?;
    for region in &program.regions {
        let blocks = program
            .blocks
            .iter()
            .filter(|block| block.region == region.id)
            .map(|block| block.id)
            .collect::<BTreeSet<_>>();
        let dominators = Dominance::new(region.entry, &blocks, &predecessors)?;
        for id in &blocks {
            let block = program
                .blocks
                .get(id.index())
                .ok_or_else(|| SemanticError::invalid("missing block"))?;
            check_block(&context, block, &dominators)?;
        }
    }
    assignment::verify(&context, &predecessors)?;
    calls(program, limits.call_depth)
}
fn resources(program: &Program, limits: CompileLimits) -> Result<(), SemanticError> {
    if program.blocks.len() > limits.blocks {
        return Err(SemanticError::limit(
            crate::ResourceKind::CompileBlocks,
            program.blocks.len(),
            limits.blocks,
            "block budget exceeded",
        ));
    }
    if program.slots.len() > limits.slots {
        return Err(SemanticError::limit(
            crate::ResourceKind::CompileSlots,
            program.slots.len(),
            limits.slots,
            "slot budget exceeded",
        ));
    }
    let mut nodes = program.regions.len();
    let retained =
        crate::semantic::retained::sum([std::mem::size_of::<Program>(), program.heap()?])?;
    let mut storage = 0usize;
    let mut qubits = 0usize;
    for slot in &program.slots {
        storage = storage
            .checked_add(storage_size(&slot.ty)?)
            .ok_or_else(|| SemanticError::budget("storage overflow"))?;
        if let Type::Qubit(count) = slot.ty {
            qubits = qubits
                .checked_add(count)
                .ok_or_else(|| SemanticError::budget("qubit count overflow"))?;
        }
    }
    for block in &program.blocks {
        nodes = nodes
            .checked_add(block.arguments.len())
            .and_then(|count| count.checked_add(block.instructions.len()))
            .ok_or_else(|| SemanticError::budget("IR node count overflow"))?;
        for instruction in &block.instructions {
            nodes = nodes
                .checked_add(instruction.results.len())
                .ok_or_else(|| SemanticError::budget("IR value count overflow"))?;
        }
    }
    let analysis = analysis_storage(program, nodes, storage, retained)?;
    if analysis > limits.storage_bytes {
        return Err(SemanticError::limit(
            crate::ResourceKind::StorageBytes,
            analysis,
            limits.storage_bytes,
            "verification working storage budget exceeded",
        ));
    }
    if nodes > limits.nodes {
        return Err(SemanticError::limit(
            crate::ResourceKind::CompileNodes,
            nodes,
            limits.nodes,
            "IR node budget exceeded",
        ));
    }
    if storage > limits.storage_bytes {
        return Err(SemanticError::limit(
            crate::ResourceKind::StorageBytes,
            storage,
            limits.storage_bytes,
            "storage budget exceeded",
        ));
    }
    if qubits > limits.qubits {
        return Err(SemanticError::limit(
            crate::ResourceKind::Qubits,
            qubits,
            limits.qubits,
            "qubit budget exceeded",
        ));
    }

    Ok(())
}
fn analysis_storage(
    program: &Program,
    nodes: usize,
    storage: usize,
    retained: usize,
) -> Result<usize, SemanticError> {
    let edge_count = program
        .blocks
        .iter()
        .try_fold(0usize, |count, block| {
            count.checked_add(block.predecessors.len())
        })
        .ok_or_else(|| SemanticError::budget("verification edge count overflow"))?;
    // Dominators retain a graph and immediate-parent tree, not a set per pair
    // of blocks. Definite assignment still keeps block-by-slot facts.
    program
        .blocks
        .len()
        .checked_add(edge_count)
        .and_then(|graph| {
            program
                .blocks
                .len()
                .checked_mul(program.slots.len())
                .and_then(|slots| graph.checked_add(slots))
        })
        .and_then(|entries| entries.checked_mul(512))
        .and_then(|bytes| {
            nodes
                .checked_mul(256)
                .and_then(|values| bytes.checked_add(values))
        })
        .and_then(|bytes| bytes.checked_add(storage))
        .and_then(|bytes| {
            retained
                .checked_mul(4)
                .and_then(|retained| bytes.checked_add(retained))
        })
        .and_then(|bytes| {
            partial_working_set(program).and_then(|partial| bytes.checked_add(partial))
        })
        .ok_or_else(|| SemanticError::budget("verification storage overflow"))
}
fn partial_working_set(program: &Program) -> Option<usize> {
    let writes = program.blocks.iter().flat_map(|block| &block.instructions).filter(|item| matches!(&item.kind, super::InstructionKind::Store { place, .. } if !place.indices.is_empty())).count();
    let rank = program
        .slots
        .iter()
        .filter_map(|slot| match &slot.ty {
            Type::Array { dimensions, .. } => Some(dimensions.len()),
            _ => None,
        })
        .max()
        .unwrap_or(1);
    rank.checked_mul(std::mem::size_of::<usize>())
        .and_then(|bytes| bytes.checked_add(128))
        .and_then(|bytes| bytes.checked_mul(writes))
        .and_then(|bytes| bytes.checked_mul(program.blocks.len()))
        .and_then(|bytes| bytes.checked_mul(4))
}
fn definitions(program: &Program) -> Result<Context<'_>, SemanticError> {
    if program
        .regions
        .get(program.entry.index())
        .is_none_or(|region| region.id != program.entry)
    {
        return Err(SemanticError::invalid("missing program entry"));
    }
    for (index, region) in program.regions.iter().enumerate() {
        if region.id.owner() != program.id || region.id.index() != index {
            return Err(SemanticError::invalid(
                "foreign or duplicate region identity",
            ));
        }
        let entry = program
            .blocks
            .get(region.entry.index())
            .ok_or_else(|| SemanticError::invalid("missing region entry"))?;
        if entry.id != region.entry || entry.region != region.id {
            return Err(SemanticError::invalid(
                "region entry belongs to another region",
            ));
        }
        for parameter in &region.parameters {
            let slot = program
                .slots
                .get(parameter.index())
                .ok_or_else(|| SemanticError::invalid("missing parameter slot"))?;
            if slot.id != *parameter
                || slot.region != region.id
                || slot.interface != super::Interface::Parameter
            {
                return Err(SemanticError::invalid(
                    "invalid function parameter interface",
                ));
            }
        }
    }
    for (index, slot) in program.slots.iter().enumerate() {
        if slot.id.owner() != program.id
            || slot.id.index() != index
            || program
                .regions
                .get(slot.region.index())
                .is_none_or(|region| region.id != slot.region)
        {
            return Err(SemanticError::invalid("foreign slot identity"));
        }
    }
    let mut values = BTreeMap::new();
    for (index, block) in program.blocks.iter().enumerate() {
        if block.id.owner() != program.id
            || block.id.index() != index
            || !block.sealed
            || block.terminator.is_none()
        {
            return Err(SemanticError::invalid(
                "foreign, unsealed, or unterminated block",
            ));
        }
        if program
            .regions
            .get(block.region.index())
            .is_none_or(|region| region.id != block.region)
        {
            return Err(SemanticError::invalid("foreign block region"));
        }
        for argument in &block.arguments {
            insert_value(program, &mut values, argument, block.id, 0)?;
        }
        for (position, instruction) in block.instructions.iter().enumerate() {
            let order = position
                .checked_add(1)
                .ok_or_else(|| SemanticError::budget("instruction position overflow"))?;
            for value in &instruction.results {
                insert_value(program, &mut values, value, block.id, order)?;
            }
        }
    }
    let constants = constant_values(program, &values);
    Ok(Context {
        program,
        values,
        constants,
    })
}
fn constant_values(
    program: &Program,
    definitions: &BTreeMap<ValueId, Definition>,
) -> BTreeMap<ValueId, crate::classical::ScalarValue> {
    use super::InstructionKind as K;
    let mut constants: BTreeMap<ValueId, crate::classical::ScalarValue> = BTreeMap::new();
    for (id, definition) in definitions {
        let item = definition.order.checked_sub(1).and_then(|index| {
            program
                .blocks
                .get(definition.block.index())
                .and_then(|block| block.instructions.get(index))
        });
        let value = item.and_then(|item| match &item.kind {
            K::Constant(value) => Some(*value),
            K::Unary { operator, value } => constants.get(value)?.unary(*operator).ok(),
            K::Binary {
                operator,
                left,
                right,
            } => constants
                .get(left)?
                .binary(*operator, constants.get(right)?)
                .ok(),
            K::Cast { value, ty } => constants.get(value)?.cast(*ty).ok(),
            _ => None,
        });
        if let Some(value) = value {
            constants.insert(*id, value);
        }
    }
    constants
}
fn insert_value(
    program: &Program,
    values: &mut BTreeMap<ValueId, Definition>,
    value: &super::Value,
    block: BlockId,
    order: usize,
) -> Result<(), SemanticError> {
    if value.id.owner() != program.id
        || value.ty == Type::Void
        || values
            .insert(
                value.id,
                Definition {
                    block,
                    order,
                    ty: value.ty.clone(),
                },
            )
            .is_some()
    {
        return Err(SemanticError::invalid(
            "foreign or multiply-defined SSA value",
        ));
    }
    Ok(())
}
fn edges(context: &Context<'_>) -> Result<BTreeMap<BlockId, BTreeSet<BlockId>>, SemanticError> {
    let mut predecessors: BTreeMap<_, BTreeSet<_>> = context
        .program
        .blocks
        .iter()
        .map(|block| (block.id, BTreeSet::new()))
        .collect();
    for block in &context.program.blocks {
        let terminator = block
            .terminator
            .as_ref()
            .ok_or_else(|| SemanticError::invalid("unterminated block"))?;
        for edge in terminator.edges() {
            let target = context
                .program
                .blocks
                .get(edge.target.index())
                .filter(|target| target.id == edge.target)
                .filter(|target| target.region == block.region)
                .ok_or_else(|| SemanticError::invalid("foreign or cross-region CFG edge"))?;
            if edge.arguments.len() != target.arguments.len() {
                return Err(SemanticError::invalid("block argument count mismatch"));
            }
            for (value, argument) in edge.arguments.iter().zip(&target.arguments) {
                if context.ty(*value)? != &argument.ty {
                    return Err(SemanticError::invalid("block argument type mismatch"));
                }
            }
            predecessors
                .get_mut(&target.id)
                .ok_or_else(|| SemanticError::invalid("missing predecessor table"))?
                .insert(block.id);
        }
    }
    for block in &context.program.blocks {
        let recorded = block.predecessors.iter().copied().collect::<BTreeSet<_>>();
        if recorded.len() != block.predecessors.len()
            || predecessors.get(&block.id) != Some(&recorded)
        {
            return Err(SemanticError::invalid(
                "sealed predecessors disagree with CFG",
            ));
        }
    }
    Ok(predecessors)
}
fn check_use(
    context: &Context<'_>,
    block: &Block,
    order: usize,
    value: ValueId,
    dominators: &Dominance,
) -> Result<(), SemanticError> {
    let definition = context
        .values
        .get(&value)
        .ok_or_else(|| SemanticError::invalid("use of undefined or foreign value"))?;
    if definition.block == block.id {
        if definition.order >= order && definition.order != 0 {
            return Err(SemanticError::invalid("SSA use precedes definition"));
        }
    } else if !dominators.contains(definition.block, block.id) {
        return Err(SemanticError::invalid(
            "SSA definition does not dominate use",
        ));
    }
    Ok(())
}
fn check_block(
    context: &Context<'_>,
    block: &Block,
    dominators: &Dominance,
) -> Result<(), SemanticError> {
    let mut memory = block
        .arguments
        .first()
        .filter(|value| value.ty == Type::Memory)
        .map(|value| value.id)
        .ok_or_else(|| SemanticError::invalid("block must begin with a memory-state argument"))?;
    for (position, item) in block.instructions.iter().enumerate() {
        let order = position
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("instruction position overflow"))?;
        for operand in item.kind.operands() {
            check_use(context, block, order, operand, dominators)?;
        }
        if item.effect != item.kind.effect() || item.accesses != item.kind.accesses() {
            return Err(SemanticError::invalid(
                "instruction effect or alias metadata mismatch",
            ));
        }
        if item.kind.memory().is_some_and(|input| input != memory) {
            return Err(SemanticError::invalid("effect uses stale memory state"));
        }
        instruction::check(context, block, item).map_err(|error| error.at(item.span))?;
        instruction::gate_effect(context, block, item).map_err(|error| error.at(item.span))?;
        if let Some(result) = item.results.last().filter(|value| value.ty == Type::Memory) {
            memory = result.id;
        }
    }
    let terminator = block
        .terminator
        .as_ref()
        .ok_or_else(|| SemanticError::invalid("unterminated block"))?;
    for value in terminator.operands() {
        check_use(context, block, usize::MAX, value, dominators)?;
    }
    for edge in terminator.edges() {
        if edge.arguments.first() != Some(&memory) {
            return Err(SemanticError::invalid("edge passes stale memory state"));
        }
    }
    instruction::terminator(context, block, terminator, memory)
}
fn calls(program: &Program, depth: usize) -> Result<(), SemanticError> {
    let graph: BTreeMap<RegionId, BTreeSet<RegionId>> = program
        .regions
        .iter()
        .map(|region| {
            (
                region.id,
                program
                    .blocks
                    .iter()
                    .filter(|block| block.region == region.id)
                    .flat_map(|block| &block.instructions)
                    .filter_map(|item| match item.kind {
                        super::InstructionKind::Call { region, .. } => Some(region),
                        _ => None,
                    })
                    .collect(),
            )
        })
        .collect();
    let mut remaining: BTreeMap<_, _> = graph
        .iter()
        .map(|(id, children)| (*id, children.len()))
        .collect();
    let mut parents: BTreeMap<RegionId, Vec<RegionId>> = BTreeMap::new();
    for (id, children) in &graph {
        for child in children {
            parents.entry(*child).or_default().push(*id);
        }
    }
    let mut ready = remaining
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect::<std::collections::VecDeque<_>>();
    let mut depths = BTreeMap::new();
    while let Some(id) = ready.pop_front() {
        let value = depths.entry(id).or_insert(1usize);
        let value = *value;
        if value > depth {
            return Err(SemanticError::limit(
                crate::ResourceKind::CallFrames,
                value,
                depth,
                "call depth budget exceeded",
            ));
        }
        for parent in parents.get(&id).into_iter().flatten() {
            let parent_depth = value
                .checked_add(1)
                .ok_or_else(|| SemanticError::budget("call depth overflow"))?;
            let known = depths.entry(*parent).or_insert(1);
            *known = (*known).max(parent_depth);
            let count = remaining
                .get_mut(parent)
                .ok_or_else(|| SemanticError::invalid("unknown parent region"))?;
            *count = count
                .checked_sub(1)
                .ok_or_else(|| SemanticError::invalid("call graph indegree underflow"))?;
            if *count == 0 {
                ready.push_back(*parent);
            }
        }
    }
    if remaining.values().any(|count| *count != 0) {
        return Err(SemanticError::new(
            crate::semantic::ErrorKind::ControlFlow,
            "recursive calls are unsupported",
        ));
    }
    Ok(())
}
