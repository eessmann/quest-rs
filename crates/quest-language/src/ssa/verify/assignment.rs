//! Independent definite initialization with prefix facts for fixed arrays.
use super::Context;
use crate::{
	semantic::{ErrorKind, SemanticError},
	ssa::{self, BlockId, InstructionKind as K, Place, SlotId, Type},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, PartialEq, Eq)]
struct Assigned {
	allocated: BTreeSet<SlotId>,
	written: BTreeMap<SlotId, BTreeSet<Vec<usize>>>,
}
impl Assigned {
	fn write(&mut self, slot: SlotId, path: Vec<usize>) {
		self.allocated.insert(slot);
		self.written.entry(slot).or_default().insert(path);
	}
	fn intersection(&self, other: &Self) -> Self {
		let mut result = Self {
			allocated: self
				.allocated
				.intersection(&other.allocated)
				.copied()
				.collect(),
			written: BTreeMap::new(),
		};
		for (slot, lefts) in &self.written {
			if let Some(rights) = other.written.get(slot) {
				for left in lefts {
					for right in rights {
						if left.starts_with(right) {
							result.write(*slot, left.clone());
						} else if right.starts_with(left) {
							result.write(*slot, right.clone());
						}
					}
				}
			}
		}
		result
	}
}
pub(super) fn verify(
	context: &Context<'_>,
	predecessors: &BTreeMap<BlockId, BTreeSet<BlockId>>,
) -> Result<(), SemanticError> {
	let reachable = reachable(context.program);
	let entries = entries(context, predecessors, &reachable)?;
	for block in &context.program.blocks {
		if !reachable.contains(&block.id) {
			continue;
		}
		let mut state = entries.get(&block.id).cloned().unwrap_or_default();
		for item in &block.instructions {
			check(context, item, &state).map_err(|error| error.at(item.span))?;
			transfer(context, item, &mut state)?;
		}
		if block.region == context.program.entry
			&& matches!(
				block.terminator,
				Some(ssa::Terminator::End { .. } | ssa::Terminator::Return { .. })
			) {
			for slot in context
				.program
				.slots
				.iter()
				.filter(|slot| slot.interface == ssa::Interface::Output)
			{
				require(
					context,
					&state,
					&Place {
						slot: slot.id,
						indices: Vec::new(),
					},
				)?;
			}
		}
	}
	Ok(())
}
fn reachable(program: &ssa::Program) -> BTreeSet<BlockId> {
	let mut visited = BTreeSet::new();
	let mut pending = program
		.regions
		.iter()
		.map(|region| region.entry)
		.collect::<Vec<_>>();
	while let Some(id) = pending.pop() {
		if !visited.insert(id) {
			continue;
		}
		if let Some(terminator) = program
			.blocks
			.get(id.index())
			.and_then(|block| block.terminator.as_ref())
		{
			pending.extend(terminator.edges().iter().map(|edge| edge.target));
		}
	}
	visited
}
fn entries(
	context: &Context<'_>,
	predecessors: &BTreeMap<BlockId, BTreeSet<BlockId>>,
	reachable: &BTreeSet<BlockId>,
) -> Result<BTreeMap<BlockId, Assigned>, SemanticError> {
	let mut universe = Assigned::default();
	for slot in &context.program.slots {
		universe.write(slot.id, Vec::new());
	}
	for item in context
		.program
		.blocks
		.iter()
		.flat_map(|block| &block.instructions)
	{
		transfer(context, item, &mut universe)?;
	}
	let mut outgoing: BTreeMap<_, _> = reachable.iter().map(|id| (*id, universe.clone())).collect();
	let mut incoming = BTreeMap::new();
	loop {
		let mut changed = false;
		for block in context
			.program
			.blocks
			.iter()
			.filter(|block| reachable.contains(&block.id))
		{
			let region = context
				.program
				.regions
				.get(block.region.index())
				.ok_or_else(|| SemanticError::invalid("missing definite-assignment region"))?;
			let mut state = if region.entry == block.id {
				let mut initial = Assigned::default();
				for parameter in &region.parameters {
					initial.write(*parameter, Vec::new());
				}
				initial
			} else {
				let mut states = predecessors
					.get(&block.id)
					.into_iter()
					.flatten()
					.filter_map(|id| outgoing.get(id));
				let mut intersection = states.next().cloned().unwrap_or_default();
				for state in states {
					intersection = intersection.intersection(state);
				}
				intersection
			};
			incoming.insert(block.id, state.clone());
			for item in &block.instructions {
				transfer(context, item, &mut state)?;
			}
			if outgoing.get(&block.id) != Some(&state) {
				outgoing.insert(block.id, state);
				changed = true;
			}
		}
		if !changed {
			return Ok(incoming);
		}
	}
}
fn check(
	context: &Context<'_>,
	item: &ssa::Instruction,
	state: &Assigned,
) -> Result<(), SemanticError> {
	match &item.kind {
		K::AllocateArray { .. } | K::Allocate { .. } | K::Input { .. } => {}
		K::Store { place, .. } => {
			if !place.indices.is_empty() && !state.allocated.contains(&place.slot) {
				return Err(SemanticError::new(
					ErrorKind::DefiniteAssignment,
					"indexed assignment requires allocated storage",
				));
			}
		}
		_ => {
			for access in &item.accesses {
				require(context, state, &access.place)?;
			}
		}
	}
	Ok(())
}
fn transfer(
	context: &Context<'_>,
	item: &ssa::Instruction,
	state: &mut Assigned,
) -> Result<(), SemanticError> {
	match &item.kind {
		K::AllocateArray { slot, .. } => {
			state.allocated.insert(*slot);
		}
		K::Allocate { slot, .. } | K::Input { slot, .. } => state.write(*slot, Vec::new()),
		K::Store { place, .. } => {
			let (path, complete) = path(context, place)?;
			if complete {
				state.write(place.slot, path);
			}
		}
		_ => {}
	}
	Ok(())
}
fn shape(context: &Context<'_>, slot: SlotId) -> Result<Vec<usize>, SemanticError> {
	let slot = context
		.program
		.slots
		.get(slot.index())
		.ok_or_else(|| SemanticError::invalid("missing initialized storage"))?;
	Ok(match &slot.ty {
		Type::Array { dimensions, .. } => dimensions.clone(),
		Type::Scalar(crate::classical::ScalarType::Bit(width)) => vec![usize::from(width.value())],
		Type::Qubit(count) => vec![*count],
		_ => Vec::new(),
	})
}
fn path(context: &Context<'_>, place: &Place) -> Result<(Vec<usize>, bool), SemanticError> {
	let shape = shape(context, place.slot)?;
	let mut path = Vec::new();
	for (position, index) in place.indices.iter().enumerate() {
		let Some(value) = context.constants.get(index) else {
			return Ok((path, false));
		};
		let size = shape
			.get(position)
			.copied()
			.ok_or_else(|| SemanticError::invalid("too many storage indices"))?;
		path.push(value.to_index(size).map_err(SemanticError::from)?);
	}
	Ok((path, true))
}
fn require(context: &Context<'_>, state: &Assigned, place: &Place) -> Result<(), SemanticError> {
	let (prefix, _) = path(context, place)?;
	let facts = state.written.get(&place.slot);
	if facts.is_some_and(|facts| facts.iter().any(|path| prefix.starts_with(path))) {
		return Ok(());
	}
	let shape = shape(context, place.slot)?;
	let needed = leaves(&shape, prefix.len())?;
	let mut covered = 0usize;
	let mut counted: Vec<&Vec<usize>> = Vec::new();
	for path in facts
		.into_iter()
		.flatten()
		.filter(|path| path.starts_with(&prefix))
	{
		if counted.iter().any(|parent| path.starts_with(parent)) {
			continue;
		}
		covered = covered
			.checked_add(leaves(&shape, path.len())?)
			.ok_or_else(|| SemanticError::budget("initialization coverage overflow"))?;
		counted.push(path);
	}
	if covered == needed {
		Ok(())
	} else {
		Err(SemanticError::new(
			ErrorKind::DefiniteAssignment,
			"storage read before definite assignment on every path",
		))
	}
}
fn leaves(shape: &[usize], prefix: usize) -> Result<usize, SemanticError> {
	shape
		.get(prefix..)
		.ok_or_else(|| SemanticError::invalid("invalid initialization prefix"))?
		.iter()
		.try_fold(1usize, |size, dimension| {
			size.checked_mul(*dimension)
				.ok_or_else(|| SemanticError::budget("initialization coverage overflow"))
		})
}
