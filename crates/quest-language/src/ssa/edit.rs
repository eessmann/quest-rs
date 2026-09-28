//! Fresh program-owned identities for one unverified transformation transaction.
use super::{
    Block, BlockId, InstructionKind, Interface, OracleId, Program, ProgramId, Region, RegionId,
    Slot, SlotId, Terminator, Type, Value, ValueId,
};
use crate::semantic::{CompileLimits, SemanticError};

/// An edit-local identity allocator. Its outputs are unverified values.
///
/// Use one allocator per candidate transaction and publish only through `Program::verify`.
/// Two transactions derived from the same source must not be merged without verification.
#[derive(Debug)]
pub struct ValueAllocator {
    owner: ProgramId,
    next: usize,
    remaining: usize,
    limit: usize,
}
impl ValueAllocator {
    /// Allocate a fresh identity within this edit, without publishing a definition.
    /// # Errors
    /// Rejects identity overflow and exhaustion of the value budget.
    pub fn allocate(&mut self, ty: Type) -> Result<Value, SemanticError> {
        if self.remaining == 0 {
            return Err(SemanticError::limit(
                crate::ResourceKind::CompileNodes,
                self.limit.saturating_add(1),
                self.limit,
                "SSA edit value budget exhausted",
            ));
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("SSA edit identity overflow"))?;
        let id = ValueId::new(self.owner, self.next);
        self.next = next;
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| SemanticError::budget("SSA edit value budget"))?;
        Ok(Value { id, ty })
    }
}
impl Program {
    /// Append an empty coherent oracle interface to an unverified transaction.
    /// The caller supplies the separately admitted payload and must reverify the
    /// entire candidate before execution. Zero-wire oracle interfaces are invalid.
    /// # Errors
    /// Rejects foreign allocators, capture collisions and resource exhaustion.
    pub fn append_oracle_region(
        &mut self,
        capture: usize,
        width: usize,
        values: &mut ValueAllocator,
        limits: CompileLimits,
    ) -> Result<RegionId, SemanticError> {
        use crate::semantic::retained::Heap as _;
        if width == 0 || width > limits.qubits || values.owner != self.id {
            return Err(SemanticError::invalid("invalid synthetic oracle interface"));
        }
        if self.regions.iter().any(|region| region.oracle.as_ref().is_some_and(|id| id.index() == capture))
            || self.blocks.iter().flat_map(|block| &block.instructions).any(|item| matches!(item.kind, InstructionKind::Capture { index, .. } if index == capture)) {
            return Err(SemanticError::invalid("synthetic oracle capture collision"));
        }
        let slots = self
            .slots
            .len()
            .checked_add(width)
            .ok_or_else(|| SemanticError::budget("synthetic oracle slots"))?;
        let blocks = self
            .blocks
            .len()
            .checked_add(1)
            .ok_or_else(|| SemanticError::budget("synthetic oracle blocks"))?;
        let nodes = super::verify::node_count(self)?
            .checked_add(2)
            .ok_or_else(|| SemanticError::budget("synthetic oracle nodes"))?;
        let extra = width
            .checked_mul(512)
            .and_then(|n| n.checked_add(1024))
            .ok_or_else(|| SemanticError::budget("synthetic oracle bytes"))?;
        let bytes = self
            .heap()?
            .checked_add(extra)
            .ok_or_else(|| SemanticError::budget("synthetic oracle bytes"))?;
        if slots > limits.slots
            || blocks > limits.blocks
            || nodes > limits.nodes
            || bytes > limits.storage_bytes
        {
            return Err(SemanticError::budget("synthetic oracle resources"));
        }
        let region = RegionId::new(self.id, self.regions.len());
        let block = BlockId::new(self.id, self.blocks.len());
        let mut name = format!("__quest_terminal_{}", region.index());
        while self.regions.iter().any(|item| item.name == name) {
            name.push('_');
            if name.len() > limits.storage_bytes.min(1024) {
                return Err(SemanticError::budget("synthetic oracle name"));
            }
        }
        self.slots
            .try_reserve_exact(width)
            .map_err(|_| SemanticError::budget("synthetic oracle allocation"))?;
        self.blocks
            .try_reserve_exact(1)
            .map_err(|_| SemanticError::budget("synthetic oracle allocation"))?;
        self.regions
            .try_reserve_exact(1)
            .map_err(|_| SemanticError::budget("synthetic oracle allocation"))?;
        let memory = values.allocate(Type::Memory)?;
        let mut parameters = Vec::new();
        parameters
            .try_reserve_exact(width)
            .map_err(|_| SemanticError::budget("synthetic oracle allocation"))?;
        for index in 0..width {
            let id = SlotId::new(self.id, self.slots.len());
            self.slots.push(Slot {
                id,
                region,
                name: format!("q{index}"),
                ty: Type::Qubit(1),
                mutable: true,
                interface: Interface::Parameter,
                reference: true,
            });
            parameters.push(id);
        }
        self.regions.push(Region {
            id: region,
            name,
            entry: block,
            parameters,
            result: Type::Void,
            gate: true,
            oracle: Some(OracleId::new(capture)),
        });
        self.blocks.push(Block {
            id: block,
            region,
            arguments: vec![memory.clone()],
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: None,
                memory: memory.id,
            }),
            predecessors: vec![],
            sealed: true,
        });
        Ok(region)
    }
    /// Start a bounded identity allocation transaction from a verified candidate.
    /// The candidate stays unverified after edits; IDs carry no dominance/type proof.
    /// # Errors
    /// Rejects malformed source IR, budgets and identity overflow.
    pub fn value_allocator(&self, limits: CompileLimits) -> Result<ValueAllocator, SemanticError> {
        self.validate(limits)?;
        let mut next = 0usize;
        let mut count = 0usize;
        for value in self.blocks.iter().flat_map(|block| {
            block.arguments.iter().chain(
                block
                    .instructions
                    .iter()
                    .flat_map(|instruction| &instruction.results),
            )
        }) {
            next = next.max(
                value
                    .id
                    .index()
                    .checked_add(1)
                    .ok_or_else(|| SemanticError::budget("SSA edit identity overflow"))?,
            );
            count = count
                .checked_add(1)
                .ok_or_else(|| SemanticError::budget("SSA value count overflow"))?;
        }
        Ok(ValueAllocator {
            owner: self.id,
            next,
            remaining: limits
                .nodes
                .checked_sub(count)
                .ok_or_else(|| SemanticError::budget("SSA edit value budget"))?,
            limit: limits.nodes,
        })
    }
}
