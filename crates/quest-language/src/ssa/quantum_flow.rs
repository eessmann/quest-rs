//! Finite, snapshot-bound quantum value flow derived from verified SSA.
//!
//! A physical storage location can have many value versions. Block entries and
//! CFG edges each receive distinct versions, so a loop adds a finite backedge
//! fact rather than unrolling iterations. Dynamic indexing invalidates every
//! tracked location on its storage root. This is dependency information, not a
//! license to remove effects or runtime bounds and alias checks.
use super::{
	BlockHandle, BlockId, InstructionKind, Place, RegionId, SlotId, SnapshotId, Terminator, Type,
	ValueId, VerifiedProgram,
};
use crate::{classical::ScalarValue, semantic::SemanticError};
use std::{
	collections::{BTreeMap, BTreeSet},
	mem::size_of,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // Every field is an explicit independent admission cap.
pub struct QuantumFlowLimits {
	max_facts: usize,
	max_edges: usize,
	max_alias_comparisons: usize,
	max_work: usize,
	max_bytes: usize,
}
impl QuantumFlowLimits {
	/// Construct immutable limits no greater than the hard defaults.
	/// # Errors
	/// Rejects zero, excessive, or otherwise invalid caps.
	pub fn new(
		max_facts: usize,
		max_edges: usize,
		max_alias_comparisons: usize,
		max_work: usize,
		max_bytes: usize,
	) -> Result<Self, SemanticError> {
		let hard = Self::default();
		if max_facts == 0
			|| max_facts > hard.max_facts
			|| max_edges == 0
			|| max_edges > hard.max_edges
			|| max_alias_comparisons == 0
			|| max_alias_comparisons > hard.max_alias_comparisons
			|| max_work == 0
			|| max_work > hard.max_work
			|| max_bytes == 0
			|| max_bytes > hard.max_bytes
		{
			return Err(SemanticError::budget("invalid quantum flow limits"));
		}
		Ok(Self {
			max_facts,
			max_edges,
			max_alias_comparisons,
			max_work,
			max_bytes,
		})
	}
}
impl Default for QuantumFlowLimits {
	fn default() -> Self {
		Self {
			max_facts: 1_000_000,
			max_edges: 200_000,
			max_alias_comparisons: 1_000_000,
			max_work: 10_000_000,
			max_bytes: 64 * 1024 * 1024,
		}
	}
}

/// Charged analysis allowances. Work includes alias comparisons; retained bytes
/// conservatively include temporary construction space as well as published facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantumFlowUsage {
	pub work: usize,
	pub alias_comparisons: usize,
	pub retained_bytes: usize,
	pub facts: usize,
	pub edges: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasRelation {
	Same,
	Disjoint,
	MayAlias,
}

/// `index == None` denotes an unknown element or whole storage root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QuantumStorage {
	pub slot: SlotId,
	pub index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QuantumVersion {
	snapshot: SnapshotId,
	index: usize,
}
impl QuantumVersion {
	#[must_use]
	pub const fn snapshot(self) -> SnapshotId {
		self.snapshot
	}
}

#[derive(Debug, Clone)]
pub struct QuantumFact {
	pub version: QuantumVersion,
	pub storage: QuantumStorage,
	pub inputs: Vec<QuantumVersion>,
}

#[derive(Debug, Clone)]
pub struct QuantumEvent {
	pub instruction: usize,
	pub inputs: Vec<QuantumVersion>,
	pub outputs: Vec<QuantumVersion>,
	/// A multiwire instruction is one indivisible occurrence.
	pub coupled: bool,
}

#[derive(Debug, Clone)]
pub struct QuantumEdge {
	pub from: BlockId,
	pub to: BlockId,
	/// `Some(true)` is a then edge; `Some(false)` is an else edge.
	pub arm: Option<bool>,
	pub backedge: bool,
	pub inputs: Vec<QuantumVersion>,
	pub outputs: Vec<QuantumVersion>,
}

#[derive(Debug, Clone)]
pub struct QuantumFlow {
	snapshot: SnapshotId,
	facts: Vec<QuantumFact>,
	events: BTreeMap<(BlockId, usize), QuantumEvent>,
	edges: Vec<QuantumEdge>,
	constants: BTreeMap<ValueId, ScalarValue>,
	widths: BTreeMap<SlotId, usize>,
	slot_regions: BTreeMap<SlotId, RegionId>,
	block_regions: BTreeMap<BlockId, RegionId>,
	usage: QuantumFlowUsage,
}

impl QuantumFlow {
	/// Derive bounded quantum dependencies from one immutable verified snapshot.
	///
	/// # Errors
	/// Rejects an exhausted fact, CFG edge, alias, work, or storage limit.
	#[allow(clippy::too_many_lines)] // Three bounded passes share one ledger and one immutable publication.
	pub fn analyze(
		program: &VerifiedProgram,
		limits: QuantumFlowLimits,
	) -> Result<Self, SemanticError> {
		let snapshot = program.snapshot();
		let mut work = 0usize;
		let mut bytes = 0usize;
		let instruction_count = program.blocks().iter().try_fold(0usize, |count, block| {
			count
				.checked_add(block.instructions.len())
				.ok_or_else(|| SemanticError::budget("quantum instruction count"))
		})?;
		charge(&mut work, instruction_count, limits.max_work)?;
		charge_bytes(
			&mut bytes,
			instruction_count
				.checked_mul(96)
				.and_then(|value| value.checked_add(program.slots().len().checked_mul(192)?))
				.and_then(|value| value.checked_add(program.blocks().len().checked_mul(96)?))
				.ok_or_else(|| SemanticError::budget("quantum source maps"))?,
			limits.max_bytes,
		)?;
		let constants = program
			.blocks()
			.iter()
			.flat_map(|block| &block.instructions)
			.filter_map(|item| match (&item.kind, item.results.as_slice()) {
				(InstructionKind::Constant(value), [result]) => Some((result.id, *value)),
				_ => None,
			})
			.collect();
		let widths = program
			.slots()
			.iter()
			.filter_map(|slot| match slot.ty {
				Type::Qubit(width) => Some((slot.id, width)),
				_ => None,
			})
			.collect();
		let slot_regions = program
			.slots()
			.iter()
			.map(|slot| (slot.id, slot.region))
			.collect();
		let block_regions = program
			.blocks()
			.iter()
			.map(|block| (block.id, block.region))
			.collect();
		let mut flow = Self {
			snapshot,
			facts: Vec::new(),
			events: BTreeMap::new(),
			edges: Vec::new(),
			constants,
			widths,
			slot_regions,
			block_regions,
			usage: QuantumFlowUsage {
				work: 0,
				alias_comparisons: 0,
				retained_bytes: bytes,
				facts: 0,
				edges: 0,
			},
		};
		let mut comparisons = 0usize;
		let mut region_storage = BTreeMap::<_, BTreeSet<_>>::new();
		for block in program.blocks() {
			for item in &block.instructions {
				if !quantum_effect(&item.kind) {
					continue;
				}
				for access in &item.accesses {
					charge(&mut work, 1, limits.max_work)?;
					charge_bytes(&mut flow.usage.retained_bytes, 128, limits.max_bytes)?;
					if let Some(storage) = flow.storage(&access.place) {
						region_storage
							.entry(block.region)
							.or_default()
							.insert(storage);
					}
				}
			}
		}
		let mut entries = BTreeMap::new();
		let mut exits = BTreeMap::new();
		for block in program.blocks() {
			let keys = region_storage.get(&block.region);
			let mut current = BTreeMap::new();
			for storage in keys.into_iter().flatten() {
				charge(&mut work, 1, limits.max_work)?;
				charge_bytes(&mut flow.usage.retained_bytes, 160, limits.max_bytes)?;
				let version = flow.add_fact(*storage, &[], limits)?;
				entries.insert((block.id, *storage), version);
				current.insert(*storage, version);
			}
			for (position, item) in block.instructions.iter().enumerate() {
				if !quantum_effect(&item.kind) {
					continue;
				}
				charge_bytes(
					&mut flow.usage.retained_bytes,
					footprint::<QuantumStorage>(
						item.accesses
							.len()
							.checked_mul(2)
							.ok_or_else(|| SemanticError::budget("quantum accesses"))?,
						0,
					)?,
					limits.max_bytes,
				)?;
				let accesses = item
					.accesses
					.iter()
					.filter_map(|access| flow.storage(&access.place))
					.collect::<Vec<_>>();
				if accesses.is_empty() {
					continue;
				}
				let mut affected = BTreeSet::new();
				for storage in &accesses {
					for key in keys.into_iter().flatten() {
						charge(&mut comparisons, 1, limits.max_alias_comparisons)?;
						charge(&mut work, 1, limits.max_work)?;
						charge_bytes(&mut flow.usage.retained_bytes, 64, limits.max_bytes)?;
						if storage.slot == key.slot
							&& (storage.index.is_none()
								|| key.index.is_none()
								|| storage.index == key.index)
						{
							affected.insert(*key);
						}
					}
				}
				let mut inputs = Vec::new();
				let mut outputs = Vec::new();
				charge_bytes(
					&mut flow.usage.retained_bytes,
					affected
						.len()
						.checked_mul(footprint::<QuantumVersion>(4, 64)?)
						.ok_or_else(|| SemanticError::budget("quantum event storage"))?,
					limits.max_bytes,
				)?;
				for key in &affected {
					charge(&mut work, 1, limits.max_work)?;
					let input = *current
						.get(key)
						.ok_or_else(|| SemanticError::invalid("missing quantum flow entry"))?;
					inputs.push(input);
				}
				for key in affected {
					charge(&mut work, inputs.len(), limits.max_work)?;
					let output = flow.add_fact(key, &inputs, limits)?;
					current.insert(key, output);
					outputs.push(output);
				}
				flow.events.insert(
					(block.id, position),
					QuantumEvent {
						instruction: position,
						inputs,
						outputs,
						coupled: accesses.len() > 1,
					},
				);
			}
			charge_bytes(&mut flow.usage.retained_bytes, 96, limits.max_bytes)?;
			exits.insert(block.id, current);
		}
		// One reusable two-state CFG traversal fits this allowance. Every
		// traversed node and edge is charged to the aggregate work counter.
		charge_bytes(
			&mut flow.usage.retained_bytes,
			program
				.blocks()
				.len()
				.checked_mul(256)
				.ok_or_else(|| SemanticError::budget("quantum CFG traversal storage"))?,
			limits.max_bytes,
		)?;
		for block in program.blocks() {
			let mut append_edge = |target: BlockId,
			                       arm: Option<bool>,
			                       flow: &mut Self|
			 -> Result<(), SemanticError> {
				if flow.edges.len() >= limits.max_edges {
					return Err(SemanticError::budget("quantum CFG edge limit"));
				}
				charge(&mut work, 1, limits.max_work)?;
				charge_bytes(
					&mut flow.usage.retained_bytes,
					footprint::<QuantumEdge>(1, 128)?,
					limits.max_bytes,
				)?;
				let mut inputs = Vec::new();
				let mut outputs = Vec::new();
				if let Some(current) = exits.get(&block.id) {
					for (storage, &input) in current {
						charge(&mut work, 1, limits.max_work)?;
						charge_bytes(
							&mut flow.usage.retained_bytes,
							footprint::<QuantumVersion>(4, 64)?,
							limits.max_bytes,
						)?;
						let output = flow.add_fact(*storage, &[input], limits)?;
						if let Some(&entry) = entries.get(&(target, *storage)) {
							charge_bytes(
								&mut flow.usage.retained_bytes,
								footprint::<QuantumVersion>(2, 0)?,
								limits.max_bytes,
							)?;
							flow.facts
								.get_mut(entry.index)
								.ok_or_else(|| {
									SemanticError::invalid("missing quantum entry fact")
								})?
								.inputs
								.push(output);
						}
						inputs.push(input);
						outputs.push(output);
					}
				}
				let region = program
					.regions()
					.get(block.region.index())
					.filter(|region| region.id == block.region)
					.ok_or_else(|| SemanticError::invalid("missing quantum CFG region"))?;
				let backedge = dominates_by_charged_reachability(
					program.blocks(),
					region.entry,
					block.id,
					target,
					&mut work,
					limits.max_work,
				)?;
				flow.edges.push(QuantumEdge {
					from: block.id,
					to: target,
					arm,
					backedge,
					inputs,
					outputs,
				});
				Ok(())
			};
			match &block.terminator {
				Some(Terminator::Jump(edge)) => append_edge(edge.target, None, &mut flow)?,
				Some(Terminator::Branch {
					then_edge,
					else_edge,
					..
				}) => {
					append_edge(then_edge.target, Some(true), &mut flow)?;
					append_edge(else_edge.target, Some(false), &mut flow)?;
				}
				_ => {}
			}
		}
		flow.usage.work = work;
		flow.usage.alias_comparisons = comparisons;
		flow.usage.facts = flow.facts.len();
		flow.usage.edges = flow.edges.len();
		Ok(flow)
	}

	fn add_fact(
		&mut self,
		storage: QuantumStorage,
		inputs: &[QuantumVersion],
		limits: QuantumFlowLimits,
	) -> Result<QuantumVersion, SemanticError> {
		if self.facts.len() >= limits.max_facts {
			return Err(SemanticError::budget("quantum fact limit"));
		}
		charge_bytes(
			&mut self.usage.retained_bytes,
			footprint::<QuantumVersion>(
				inputs
					.len()
					.checked_mul(2)
					.ok_or_else(|| SemanticError::budget("quantum fact inputs"))?,
				footprint::<QuantumFact>(1, 64)?,
			)?,
			limits.max_bytes,
		)?;
		let version = QuantumVersion {
			snapshot: self.snapshot,
			index: self.facts.len(),
		};
		self.facts.push(QuantumFact {
			version,
			storage,
			inputs: inputs.to_vec(),
		});
		Ok(version)
	}

	fn storage(&self, place: &Place) -> Option<QuantumStorage> {
		let &width = self.widths.get(&place.slot)?;
		let index = match place.indices.as_slice() {
			[] if width == 1 => Some(0),
			[value] => self
				.constants
				.get(value)
				.and_then(|value| value.to_index(width).ok()),
			_ => None,
		};
		Some(QuantumStorage {
			slot: place.slot,
			index,
		})
	}

	#[must_use]
	pub const fn snapshot(&self) -> SnapshotId {
		self.snapshot
	}
	#[must_use]
	pub const fn usage(&self) -> QuantumFlowUsage {
		self.usage
	}
	#[must_use]
	pub fn facts(&self) -> &[QuantumFact] {
		&self.facts
	}
	#[must_use]
	pub fn edges(&self) -> &[QuantumEdge] {
		&self.edges
	}
	#[must_use]
	pub fn fact(&self, version: QuantumVersion) -> Option<&QuantumFact> {
		(version.snapshot == self.snapshot)
			.then(|| self.facts.get(version.index))
			.flatten()
	}
	#[must_use]
	pub fn event(&self, handle: BlockHandle, instruction: usize) -> Option<&QuantumEvent> {
		(handle.snapshot() == self.snapshot)
			.then(|| self.events.get(&(handle.id(), instruction)))
			.flatten()
	}
	#[must_use]
	pub fn alias(&self, handle: BlockHandle, left: &Place, right: &Place) -> AliasRelation {
		let Some(&region) = self
			.block_regions
			.get(&handle.id())
			.filter(|_| handle.snapshot() == self.snapshot)
		else {
			return AliasRelation::MayAlias;
		};
		if self.slot_regions.get(&left.slot) != Some(&region)
			|| self.slot_regions.get(&right.slot) != Some(&region)
		{
			return AliasRelation::MayAlias;
		}
		let (Some(left), Some(right)) = (self.storage(left), self.storage(right)) else {
			return AliasRelation::MayAlias;
		};
		if left.slot != right.slot {
			return AliasRelation::Disjoint;
		}
		match (left.index, right.index) {
			(Some(a), Some(b)) if a == b => AliasRelation::Same,
			(Some(_), Some(_)) => AliasRelation::Disjoint,
			_ => AliasRelation::MayAlias,
		}
	}
}

const fn quantum_effect(kind: &InstructionKind) -> bool {
	matches!(
		kind,
		InstructionKind::Gate { .. }
			| InstructionKind::Call { .. }
			| InstructionKind::Measure { .. }
			| InstructionKind::Reset { .. }
			| InstructionKind::Payload { .. }
			| InstructionKind::Barrier { .. }
	)
}

fn dominates_by_charged_reachability(
	blocks: &[super::Block],
	entry: BlockId,
	source: BlockId,
	candidate: BlockId,
	work: &mut usize,
	limit: usize,
) -> Result<bool, SemanticError> {
	let mut pending = vec![(entry, entry == candidate)];
	charge(work, blocks.len(), limit)?;
	let mut seen = vec![(false, false); blocks.len()];
	let mut source_reached_through_candidate = false;
	while let Some((id, through_candidate)) = pending.pop() {
		charge(work, 1, limit)?;
		let flags = seen
			.get_mut(id.index())
			.ok_or_else(|| SemanticError::invalid("missing quantum CFG visit"))?;
		let visited = if through_candidate {
			&mut flags.1
		} else {
			&mut flags.0
		};
		if *visited {
			continue;
		}
		*visited = true;
		if id == source {
			if !through_candidate {
				return Ok(false);
			}
			source_reached_through_candidate = true;
		}
		let block = blocks
			.get(id.index())
			.filter(|block| block.id == id)
			.ok_or_else(|| SemanticError::invalid("missing quantum CFG block"))?;
		let mut push = |target: BlockId| -> Result<(), SemanticError> {
			charge(work, 1, limit)?;
			pending.push((target, through_candidate || target == candidate));
			Ok(())
		};
		match &block.terminator {
			Some(Terminator::Jump(edge)) => push(edge.target)?,
			Some(Terminator::Branch {
				then_edge,
				else_edge,
				..
			}) => {
				push(then_edge.target)?;
				push(else_edge.target)?;
			}
			_ => {}
		}
	}
	Ok(source_reached_through_candidate)
}

fn charge(used: &mut usize, amount: usize, limit: usize) -> Result<(), SemanticError> {
	*used = used
		.checked_add(amount)
		.ok_or_else(|| SemanticError::budget("quantum flow work overflow"))?;
	if *used > limit {
		return Err(SemanticError::budget("quantum flow work limit"));
	}
	Ok(())
}
fn charge_bytes(used: &mut usize, amount: usize, limit: usize) -> Result<(), SemanticError> {
	*used = used
		.checked_add(amount)
		.ok_or_else(|| SemanticError::budget("quantum flow storage overflow"))?;
	if *used > limit {
		return Err(SemanticError::budget("quantum flow storage limit"));
	}
	Ok(())
}
fn footprint<T>(count: usize, overhead: usize) -> Result<usize, SemanticError> {
	count
		.checked_mul(size_of::<T>())
		.and_then(|bytes| bytes.checked_add(overhead))
		.ok_or_else(|| SemanticError::budget("quantum flow byte forecast"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		SourceId, SourceSnapshot,
		semantic::{CompileLimits, admit},
		syntax::parse_source,
	};

	#[test]
	fn backedges_follow_dominance_even_when_block_numbers_are_reversed()
	-> Result<(), Box<dyn std::error::Error>> {
		let source = SourceSnapshot::new(
			SourceId::new(21),
			"loop",
			"qubit q; int n=2; while(n>0) { x q; n-=1; }",
		);
		let parsed = parse_source(&source)?;
		let verified = admit(parsed, CompileLimits::default())?.into_ssa()?;
		let original = QuantumFlow::analyze(&verified, QuantumFlowLimits::default())?;
		let mut raw = verified.into_unverified();
		let length = raw.blocks.len();
		let remap = |old: BlockId| -> Result<BlockId, SemanticError> {
			Ok(BlockId::new(
				raw.id,
				length
					.checked_sub(old.index())
					.and_then(|n| n.checked_sub(1))
					.ok_or_else(|| SemanticError::invalid("block permutation"))?,
			))
		};
		let mut blocks = raw.blocks.clone();
		blocks.reverse();
		for block in &mut blocks {
			block.id = remap(block.id)?;
			for predecessor in &mut block.predecessors {
				*predecessor = remap(*predecessor)?;
			}
			match &mut block.terminator {
				Some(Terminator::Jump(edge)) => edge.target = remap(edge.target)?,
				Some(Terminator::Branch {
					then_edge,
					else_edge,
					..
				}) => {
					then_edge.target = remap(then_edge.target)?;
					else_edge.target = remap(else_edge.target)?;
				}
				_ => {}
			}
		}
		for region in &mut raw.regions {
			region.entry = remap(region.entry)?;
		}
		raw.blocks = blocks;
		let verified = raw.verify(CompileLimits::default())?;
		let flow = QuantumFlow::analyze(&verified, QuantumFlowLimits::default())?;
		if original.edges().iter().filter(|edge| edge.backedge).count()
			!= flow.edges().iter().filter(|edge| edge.backedge).count()
		{
			return Err(
				std::io::Error::other("backedge count changed under block permutation").into(),
			);
		}
		if !flow
			.edges()
			.iter()
			.any(|edge| edge.backedge && edge.from.index() < edge.to.index())
		{
			return Err(std::io::Error::other("missing forward-numbered backedge").into());
		}
		if !flow
			.edges()
			.iter()
			.any(|edge| !edge.backedge && edge.from.index() > edge.to.index())
		{
			return Err(
				std::io::Error::other("acyclic backward-numbered edge misclassified").into(),
			);
		}
		Ok(())
	}
}
