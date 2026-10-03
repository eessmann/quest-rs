//! Hoist static quantum allocation while retaining lexical declaration admission.
use super::{
	SemanticError,
	promote::{rewrite_kind, rewrite_terminator},
};
use crate::ssa::{InstructionKind as K, Program};
use std::collections::BTreeMap;
pub(super) fn hoist(program: &mut Program) -> Result<(), SemanticError> {
	let entry = program
		.regions
		.get(program.entry.index())
		.ok_or_else(|| SemanticError::invalid("missing entry region"))?
		.entry;
	let mut allocations = Vec::new();
	let mut replacements = BTreeMap::new();
	for block in &mut program.blocks {
		let mut retained = Vec::with_capacity(block.instructions.len());
		for item in std::mem::take(&mut block.instructions) {
			if let K::Allocate { memory, .. } = &item.kind {
				if block.region != program.entry {
					return Err(SemanticError::invalid(
						"quantum allocation outside entry region",
					));
				}
				let result = item
					.results
					.first()
					.ok_or_else(|| SemanticError::invalid("allocation missing memory result"))?
					.id;
				replacements.insert(result, *memory);
				allocations.push(item);
			} else {
				retained.push(item);
			}
		}
		block.instructions = retained;
	}
	for block in &mut program.blocks {
		for item in &mut block.instructions {
			rewrite_kind(&mut item.kind, &replacements)?;
			item.accesses = item.kind.accesses();
		}
		if let Some(term) = &mut block.terminator {
			rewrite_terminator(term, &replacements)?;
		}
	}
	let entry = program
		.blocks
		.get_mut(entry.index())
		.ok_or_else(|| SemanticError::invalid("missing entry block"))?;
	let initial = entry
		.arguments
		.first()
		.ok_or_else(|| SemanticError::invalid("entry memory missing"))?
		.id;
	let mut memory = initial;
	for item in &mut allocations {
		if let K::Allocate { memory: input, .. } = &mut item.kind {
			*input = memory;
		}
		memory = item
			.results
			.first()
			.ok_or_else(|| SemanticError::invalid("allocation result missing"))?
			.id;
	}
	let replacements = if memory == initial {
		BTreeMap::new()
	} else {
		BTreeMap::from([(initial, memory)])
	};
	for item in &mut entry.instructions {
		rewrite_kind(&mut item.kind, &replacements)?;
		item.accesses = item.kind.accesses();
	}
	if let Some(term) = &mut entry.terminator {
		rewrite_terminator(term, &replacements)?;
	}
	allocations.append(&mut entry.instructions);
	entry.instructions = allocations;
	Ok(())
}
