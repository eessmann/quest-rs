//! Per-block quantum occurrence dependencies; CFG backedges never enter this DAG.
use super::{Block, InstructionKind, SlotId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantumNode {
    /// Instruction position identifies an occurrence, including repeated gates.
    pub instruction: usize,
    /// Earlier instruction positions on potentially overlapping quantum storage.
    pub predecessors: Vec<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantumDag {
    pub nodes: Vec<QuantumNode>,
}
impl Block {
    /// Build conservative storage-root dependencies within this block only.
    ///
    /// Aliases have already been resolved to slots; dynamic indices on a common
    /// root conservatively overlap. Calls are retained as indivisible effects.
    #[must_use]
    pub fn quantum_dag(&self) -> QuantumDag {
        let mut last: BTreeMap<SlotId, usize> = BTreeMap::new();
        let mut global = None;
        let mut nodes = Vec::new();
        for (position, item) in self.instructions.iter().enumerate() {
            if !matches!(
                item.kind,
                InstructionKind::Gate { .. }
                    | InstructionKind::Call { .. }
                    | InstructionKind::Measure { .. }
                    | InstructionKind::Reset { .. }
                    | InstructionKind::Payload { .. }
                    | InstructionKind::Barrier { .. }
            ) {
                continue;
            }
            let roots = item
                .accesses
                .iter()
                .map(|access| access.place.slot)
                .collect::<BTreeSet<_>>();
            let mut predecessors = global.into_iter().collect::<BTreeSet<_>>();
            if roots.is_empty() {
                predecessors.extend(last.values().copied());
                global = Some(position);
            } else {
                for root in roots {
                    if let Some(previous) = last.insert(root, position) {
                        predecessors.insert(previous);
                    }
                }
            }
            nodes.push(QuantumNode {
                instruction: position,
                predecessors: predecessors.into_iter().collect(),
            });
        }
        QuantumDag { nodes }
    }
}
