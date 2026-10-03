//! Per-region immediate dominators with compact ancestry intervals.
use super::{BlockId, SemanticError};
use petgraph::{algo::dominators::simple_fast, graph::DiGraph};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Dominance {
	intervals: BTreeMap<BlockId, (usize, usize)>,
}
impl Dominance {
	pub fn new(
		entry: BlockId,
		blocks: &BTreeSet<BlockId>,
		predecessors: &BTreeMap<BlockId, BTreeSet<BlockId>>,
	) -> Result<Self, SemanticError> {
		let mut graph = DiGraph::<BlockId, (), usize>::default();
		let nodes: BTreeMap<_, _> = blocks.iter().map(|&id| (id, graph.add_node(id))).collect();
		for (&id, &node) in &nodes {
			for predecessor in predecessors.get(&id).into_iter().flatten() {
				if let Some(&previous) = nodes.get(predecessor) {
					graph.add_edge(previous, node, ());
				}
			}
		}
		let root = *nodes
			.get(&entry)
			.ok_or_else(|| SemanticError::invalid("missing dominance entry"))?;
		let dominators = simple_fast(&graph, root);
		let mut children = BTreeMap::<_, Vec<_>>::new();
		for node in graph.node_indices() {
			if let Some(parent) = dominators.immediate_dominator(node) {
				children.entry(parent).or_default().push(node);
			}
		}
		let mut intervals = BTreeMap::new();
		let mut pending = vec![(root, false)];
		let mut clock = 0usize;
		while let Some((node, leaving)) = pending.pop() {
			let &id = graph
				.node_weight(node)
				.ok_or_else(|| SemanticError::invalid("missing dominator node"))?;
			if leaving {
				let interval: &mut (usize, usize) = intervals
					.get_mut(&id)
					.ok_or_else(|| SemanticError::invalid("unvisited dominator"))?;
				interval.1 = clock;
			} else {
				intervals.insert(id, (clock, clock));
				clock = clock
					.checked_add(1)
					.ok_or_else(|| SemanticError::budget("dominance position overflow"))?;
				pending.push((node, true));
				pending.extend(
					children
						.get(&node)
						.into_iter()
						.flatten()
						.rev()
						.map(|&child| (child, false)),
				);
			}
		}
		Ok(Self { intervals })
	}

	pub fn contains(&self, ancestor: BlockId, descendant: BlockId) -> bool {
		match (
			self.intervals.get(&ancestor),
			self.intervals.get(&descendant),
		) {
			(Some(&(start, end)), Some(&(position, _))) => start <= position && position < end,
			_ => false,
		}
	}
}
