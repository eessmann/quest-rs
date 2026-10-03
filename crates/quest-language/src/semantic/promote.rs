//! Bounded scalar storage promotion, separate from the independent verifier.
use super::{CompileLimits, SemanticError};
use crate::ssa::{self, BlockId, InstructionKind as K, SlotId, Type, ValueId};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn promote(
	mut program: ssa::Program,
	limits: CompileLimits,
) -> Result<ssa::Program, SemanticError> {
	let candidates = candidates(&program);
	let entries = entries(&program, &candidates);
	let mut next = program
		.blocks
		.iter()
		.flat_map(|block| {
			block
				.arguments
				.iter()
				.chain(block.instructions.iter().flat_map(|item| &item.results))
		})
		.map(|value| value.id.index())
		.max()
		.unwrap_or(0)
		.checked_add(1)
		.ok_or_else(|| SemanticError::budget("promotion identity overflow"))?;
	let mut arguments = BTreeMap::new();
	for block in &mut program.blocks {
		let mut slots = BTreeMap::new();
		for slot in entries.get(&block.id).into_iter().flatten() {
			if next >= limits.nodes {
				return Err(SemanticError::limit(
					crate::ResourceKind::CompileNodes,
					next.saturating_add(1),
					limits.nodes,
					"scalar promotion value budget exceeded",
				));
			}
			let value = ssa::Value {
				id: ValueId::new(program.id, next),
				ty: program
					.slots
					.get(slot.index())
					.ok_or_else(|| SemanticError::invalid("missing promoted slot"))?
					.ty
					.clone(),
			};
			next = next
				.checked_add(1)
				.ok_or_else(|| SemanticError::budget("promotion identity overflow"))?;
			slots.insert(*slot, value.id);
			block.arguments.push(value);
		}
		arguments.insert(block.id, slots);
	}
	let mut replacements = BTreeMap::new();
	let mut exits = BTreeMap::new();
	for block in &mut program.blocks {
		let mut current = arguments.get(&block.id).cloned().unwrap_or_default();
		let mut retained = Vec::new();
		for item in std::mem::take(&mut block.instructions) {
			match &item.kind {
				K::Store {
					place,
					value,
					memory,
					..
				} if candidates.contains(&place.slot) => {
					current.insert(place.slot, *value);
					let result = item.results.last().ok_or_else(|| {
						SemanticError::invalid("promoted store lacks memory result")
					})?;
					replacements.insert(result.id, *memory);
				}
				K::Load { place, .. } if candidates.contains(&place.slot) => {
					let replacement = current.get(&place.slot).copied().ok_or_else(|| {
						SemanticError::invalid("promotion found unassigned scalar")
					})?;
					let result = item
						.results
						.first()
						.ok_or_else(|| SemanticError::invalid("promoted load lacks value"))?;
					replacements.insert(result.id, replacement);
				}
				_ => retained.push(item),
			}
		}
		block.instructions = retained;
		exits.insert(block.id, current);
	}
	for block in &mut program.blocks {
		for item in &mut block.instructions {
			rewrite_kind(&mut item.kind, &replacements)?;
			item.accesses = item.kind.accesses();
		}
		if let Some(term) = &mut block.terminator {
			append_edges(
				term,
				exits
					.get(&block.id)
					.ok_or_else(|| SemanticError::invalid("missing promotion exit"))?,
				&arguments,
			)?;
			rewrite_terminator(term, &replacements)?;
		}
	}
	Ok(program)
}
fn candidates(program: &ssa::Program) -> BTreeSet<SlotId> {
	let mut set = program
		.slots
		.iter()
		.filter(|slot| {
			slot.interface == ssa::Interface::Local && matches!(slot.ty, Type::Scalar(_))
		})
		.map(|slot| slot.id)
		.collect::<BTreeSet<_>>();
	for item in program.blocks.iter().flat_map(|block| &block.instructions) {
		for access in item.kind.accesses() {
			if !access.place.indices.is_empty() {
				set.remove(&access.place.slot);
			}
		}
		if let K::AllocateArray { slot, .. } = &item.kind {
			set.remove(slot);
		}
		if let K::Call { arguments, .. } = &item.kind {
			for argument in arguments {
				if let ssa::CallArgument::Reference { place, .. } = argument {
					set.remove(&place.slot);
				}
			}
		}
	}
	set
}
fn entries(
	program: &ssa::Program,
	candidates: &BTreeSet<SlotId>,
) -> BTreeMap<BlockId, BTreeSet<SlotId>> {
	let mut exits: BTreeMap<_, _> = program
		.blocks
		.iter()
		.map(|block| (block.id, candidates.clone()))
		.collect();
	let mut entries = BTreeMap::new();
	loop {
		let mut changed = false;
		for block in &program.blocks {
			let is_entry = program
				.regions
				.iter()
				.any(|region| region.entry == block.id);
			let mut predecessor_sets = block.predecessors.iter().filter_map(|id| exits.get(id));
			let mut assigned = if is_entry {
				BTreeSet::new()
			} else {
				predecessor_sets.next().cloned().unwrap_or_default()
			};
			if !is_entry {
				for set in predecessor_sets {
					assigned = assigned.intersection(set).copied().collect();
				}
			}
			entries.insert(block.id, assigned.clone());
			for item in &block.instructions {
				if let K::Store { place, .. } = &item.kind
					&& candidates.contains(&place.slot)
				{
					assigned.insert(place.slot);
				}
			}
			if exits.get(&block.id) != Some(&assigned) {
				exits.insert(block.id, assigned);
				changed = true;
			}
		}
		if !changed {
			return entries;
		}
	}
}
fn resolve(
	mut id: ValueId,
	replacements: &BTreeMap<ValueId, ValueId>,
) -> Result<ValueId, SemanticError> {
	let mut remaining = replacements.len();
	while let Some(replacement) = replacements.get(&id) {
		remaining = remaining
			.checked_sub(1)
			.ok_or_else(|| SemanticError::invalid("cyclic scalar substitution"))?;
		id = *replacement;
	}
	Ok(id)
}
fn rewrite_values(
	values: &mut [ValueId],
	replacements: &BTreeMap<ValueId, ValueId>,
) -> Result<(), SemanticError> {
	for value in values {
		*value = resolve(*value, replacements)?;
	}
	Ok(())
}
fn rewrite_place(
	place: &mut ssa::Place,
	replacements: &BTreeMap<ValueId, ValueId>,
) -> Result<(), SemanticError> {
	rewrite_values(&mut place.indices, replacements)
}
fn rewrite_modifiers(
	modifiers: &mut [ssa::GateModifier],
	replacements: &BTreeMap<ValueId, ValueId>,
) -> Result<(), SemanticError> {
	for modifier in modifiers {
		if let ssa::GateModifier::Power(value) = modifier {
			*value = resolve(*value, replacements)?;
		}
	}
	Ok(())
}
pub fn rewrite_kind(
	kind: &mut K,
	replacements: &BTreeMap<ValueId, ValueId>,
) -> Result<(), SemanticError> {
	match kind {
		K::Assert {
			condition, memory, ..
		} => {
			*condition = resolve(*condition, replacements)?;
			*memory = resolve(*memory, replacements)?;
		}
		K::RangeAdvance { current, step, end } => {
			*current = resolve(*current, replacements)?;
			*step = resolve(*step, replacements)?;
			*end = resolve(*end, replacements)?;
		}
		K::Constant(_) | K::Capture { .. } => {}
		K::Unary { value, .. } | K::Cast { value, .. } | K::GateParameter { value } => {
			*value = resolve(*value, replacements)?;
		}
		K::Binary { left, right, .. } => {
			*left = resolve(*left, replacements)?;
			*right = resolve(*right, replacements)?;
		}
		K::Array { values }
		| K::Builtin {
			arguments: values, ..
		} => rewrite_values(values, replacements)?,
		K::Index { value, index } => {
			*value = resolve(*value, replacements)?;
			*index = resolve(*index, replacements)?;
		}
		K::Input { memory, .. } | K::AllocateArray { memory, .. } | K::Allocate { memory, .. } => {
			*memory = resolve(*memory, replacements)?;
		}
		K::Load { place, memory } | K::Measure { place, memory } | K::Reset { place, memory } => {
			rewrite_place(place, replacements)?;
			*memory = resolve(*memory, replacements)?;
		}
		K::Store {
			place,
			value,
			memory,
			..
		} => {
			rewrite_place(place, replacements)?;
			*value = resolve(*value, replacements)?;
			*memory = resolve(*memory, replacements)?;
		}
		K::Call {
			arguments,
			controls,
			modifiers,
			memory,
			..
		} => {
			for place in controls {
				rewrite_place(place, replacements)?;
			}
			for argument in arguments {
				match argument {
					ssa::CallArgument::Value(value) => {
						*value = resolve(*value, replacements)?;
					}
					ssa::CallArgument::Reference { place, .. } => {
						rewrite_place(place, replacements)?;
					}
				}
			}
			rewrite_modifiers(modifiers, replacements)?;
			*memory = resolve(*memory, replacements)?;
		}
		K::Gate {
			arguments,
			operands,
			modifiers,
			memory,
			..
		} => {
			rewrite_values(arguments, replacements)?;
			for place in operands {
				rewrite_place(place, replacements)?;
			}
			rewrite_modifiers(modifiers, replacements)?;
			*memory = resolve(*memory, replacements)?;
		}
		K::Barrier { places, memory } | K::Payload { places, memory, .. } => {
			for place in places {
				rewrite_place(place, replacements)?;
			}
			*memory = resolve(*memory, replacements)?;
		}
	}
	Ok(())
}
fn append_edges(
	term: &mut ssa::Terminator,
	current: &BTreeMap<SlotId, ValueId>,
	arguments: &BTreeMap<BlockId, BTreeMap<SlotId, ValueId>>,
) -> Result<(), SemanticError> {
	let edge =
		|edge: &mut ssa::Edge| -> Result<(), SemanticError> {
			for slot in arguments
				.get(&edge.target)
				.into_iter()
				.flat_map(|slots| slots.keys())
			{
				edge.arguments.push(*current.get(slot).ok_or_else(|| {
					SemanticError::invalid("promotion edge missing assigned scalar")
				})?);
			}
			Ok(())
		};
	match term {
		ssa::Terminator::Jump(target) => edge(target)?,
		ssa::Terminator::Branch {
			then_edge,
			else_edge,
			..
		} => {
			edge(then_edge)?;
			edge(else_edge)?;
		}
		_ => {}
	}
	Ok(())
}
pub fn rewrite_terminator(
	term: &mut ssa::Terminator,
	replacements: &BTreeMap<ValueId, ValueId>,
) -> Result<(), SemanticError> {
	match term {
		ssa::Terminator::Jump(edge) => rewrite_values(&mut edge.arguments, replacements)?,
		ssa::Terminator::Branch {
			condition,
			then_edge,
			else_edge,
		} => {
			*condition = resolve(*condition, replacements)?;
			rewrite_values(&mut then_edge.arguments, replacements)?;
			rewrite_values(&mut else_edge.arguments, replacements)?;
		}
		ssa::Terminator::Return { value, memory } => {
			if let Some(id) = value {
				*id = resolve(*id, replacements)?;
			}
			*memory = resolve(*memory, replacements)?;
		}
		ssa::Terminator::End { memory } => {
			*memory = resolve(*memory, replacements)?;
		}
	}
	Ok(())
}
